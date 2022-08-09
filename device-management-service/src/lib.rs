pub mod endpoints;
pub mod service;
pub mod utils;

use crate::service::management::ManagementService;
use actix_cors::Cors;
use actix_web::web;
use anyhow::Context;
use drogue_cloud_admin_service::apps;
use drogue_cloud_registry_events::sender::{KafkaEventSender, KafkaSenderConfig};
use drogue_cloud_service_api::{
    health::BoxedHealthChecked, health::HealthChecked, webapp as actix_web,
    webapp::web::ServiceConfig,
};
use drogue_cloud_service_common::{
    actix::http::{HttpBuilder, HttpConfig},
    actix_auth::authentication::AuthN,
    app::{Startup, StartupExt},
    auth::{openid, pat},
    client::UserAuthClientConfig,
    defaults,
    keycloak::{client::KeycloakAdminClient, KeycloakAdminClientConfig, KeycloakClient},
};
use serde::Deserialize;
use service::PostgresManagementServiceConfig;

#[derive(Debug)]
pub struct WebData<S: ManagementService> {
    pub service: S,
    pub authenticator: Option<openid::Authenticator>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    pub oauth: openid::AuthenticatorConfig,

    #[serde(flatten)]
    pub database_config: PostgresManagementServiceConfig,

    pub kafka_sender: KafkaSenderConfig,

    pub keycloak: KeycloakAdminClientConfig,

    #[serde(default)]
    pub http: HttpConfig,
}

#[macro_export]
macro_rules! crud {
    ($sender:ty, $keycloak:ty, $scope:ident, $base:literal, $module:path, $name:ident) => {{
        $scope
            .service({
                let resource = concat!($base, stringify!($name), "s");
                log::debug!("{}", resource);
                web::resource(resource)
                    // create resources
                    .route(web::post().to({
                        use $module as m;
                        m::create::<$sender, $keycloak>
                    }))
                    // list resources
                    .route(web::get().to({
                        use $module as m;
                        m::list::<$sender, $keycloak>
                    }))
            })
            .service({
                let resource = concat!($base, stringify!($name), "s/{", stringify!($name), "}");
                log::debug!("{}", resource);

                web::resource(resource)
                    .name(stringify!($name))
                    // "use" is required due to: https://github.com/rust-lang/rust/issues/48067
                    .route(web::get().to({
                        use $module as m;
                        m::read::<$sender, $keycloak>
                    }))
                    .route(web::put().to({
                        use $module as m;
                        m::update::<$sender, $keycloak>
                    }))
                    .route(web::delete().to({
                        use $module as m;
                        m::delete::<$sender, $keycloak>
                    }))
            })
    }};
}

#[macro_export]
macro_rules! app {
    ($cfg:expr, $sender:ty, $keycloak:ty,  $auth:expr) => {{
        let app = $cfg;

        let app = {
            let scope = web::scope("/api/registry/v1alpha1")
                .wrap($auth.clone())
                .wrap(Cors::permissive());

            let scope = crud!($sender, $keycloak, scope, "", endpoints::apps, app);

            let scope = crud!(
                $sender,
                $keycloak,
                scope,
                "apps/{app}/",
                endpoints::devices,
                device
            );

            app.service(scope)
        };

        let app = {
            let scope = web::scope("/api/admin/v1alpha1")
                .wrap($auth)
                .wrap(Cors::permissive());

            let scope = scope.service(
                web::resource("/apps/{appId}/transfer-ownership")
                    .route(web::get().to(apps::read_transfer_state::<
                        service::PostgresManagementService<$sender, $keycloak>,
                    >))
                    .route(web::put().to(apps::transfer::<
                        service::PostgresManagementService<$sender, $keycloak>,
                    >))
                    .route(web::delete().to(apps::cancel::<
                        service::PostgresManagementService<$sender, $keycloak>,
                    >)),
            );

            let scope = scope.service(
                web::resource("/apps/{appId}/accept-ownership")
                    .route(web::put().to(apps::accept::<
                        service::PostgresManagementService<$sender, $keycloak>,
                    >)),
            );

            let scope = scope.service(
                web::resource("/apps/{appId}/members")
                    .route(web::get().to(apps::get_members::<
                        service::PostgresManagementService<$sender, $keycloak>,
                    >))
                    .route(web::put().to(apps::set_members::<
                        service::PostgresManagementService<$sender, $keycloak>,
                    >)),
            );

            app.service(scope)
        };

        app
    }};
}

pub async fn configurator(
    config: Config,
) -> anyhow::Result<(
    impl Fn(&mut ServiceConfig) + Send + Sync + Clone,
    Vec<Box<dyn HealthChecked>>,
)> {
    let enable_access_token = config.enable_access_token;

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        Some(user_auth.into_client().await?)
    } else {
        None
    };

    let sender = KafkaEventSender::new(config.kafka_sender)
        .context("Unable to create Kafka event sender")?;

    let keycloak_admin_client = KeycloakAdminClient::new(config.keycloak)?;

    let service = service::PostgresManagementService::new(
        config.database_config,
        sender,
        keycloak_admin_client,
    )?;

    let data = web::Data::new(WebData {
        authenticator: authenticator.as_ref().cloned(),
        service: service.clone(),
    });

    // main server

    let db_service = service.clone();

    Ok((
        move |cfg: &mut ServiceConfig| {
            let auth = AuthN {
                openid: authenticator.as_ref().cloned(),
                token: user_auth
                    .clone()
                    .map(|user_auth| pat::Authenticator::new(user_auth)),
                enable_access_token,
            };
            app!(cfg, KafkaEventSender, KeycloakAdminClient, auth)
                // for the management service
                .app_data(data.clone())
                // for the admin service
                .app_data(web::Data::new(apps::WebData {
                    service: db_service.clone(),
                }));
        },
        vec![service.boxed()],
    ))
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
    log::info!("Running device management service!");

    let (builder, checks) = configurator(config.clone()).await?;
    HttpBuilder::new(config.http, Some(startup.runtime_config()), builder).start(startup)?;

    // run

    startup.check_iter(checks);

    // exiting

    Ok(())
}
