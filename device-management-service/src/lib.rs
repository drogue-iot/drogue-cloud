pub mod endpoints;
pub mod service;
pub mod utils;

use crate::service::management::ManagementService;
use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use anyhow::Context;
use drogue_cloud_admin_service::apps;
use drogue_cloud_registry_events::sender::KafkaEventSender;
use drogue_cloud_registry_events::sender::KafkaSenderConfig;
use drogue_cloud_service_api::webapp as actix_web;
use drogue_cloud_service_common::actix_auth::authentication::AuthN;
use drogue_cloud_service_common::client::{UserAuthClient, UserAuthClientConfig};
use drogue_cloud_service_common::openid::AuthenticatorConfig;
use drogue_cloud_service_common::{
    defaults,
    health::HealthServerConfig,
    keycloak::{client::KeycloakAdminClient, KeycloakAdminClientConfig, KeycloakClient},
};
use drogue_cloud_service_common::{health::HealthServer, openid::Authenticator};
use futures::TryFutureExt;
use serde::Deserialize;
use service::PostgresManagementServiceConfig;

#[derive(Debug)]
pub struct WebData<S: ManagementService> {
    pub service: S,
    pub authenticator: Option<Authenticator>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    pub oauth: AuthenticatorConfig,

    #[serde(flatten)]
    pub database_config: PostgresManagementServiceConfig,

    pub kafka_sender: KafkaSenderConfig,

    pub keycloak: KeycloakAdminClientConfig,

    #[serde(default)]
    pub workers: Option<usize>,
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
    ($sender:ty, $keycloak:ty, $max_json_payload_size:expr, $auth:expr) => {{
        let app = App::new()
            .wrap(drogue_cloud_service_api::webapp::opentelemetry::RequestTracing::new())
            .wrap(actix_web::middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit($max_json_payload_size));

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

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::info!("Running device management service!");

    let enable_access_token = config.enable_access_token;

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        let client = reqwest::Client::new();
        let user_auth = UserAuthClient::from_config(client, user_auth).await?;
        Some(user_auth)
    } else {
        None
    };

    let sender = KafkaEventSender::new(config.kafka_sender)
        .context("Unable to create Kafka event sender")?;

    let max_json_payload_size = 64 * 1024;

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
    let main = HttpServer::new(move || {
        let auth = AuthN {
            openid: authenticator.as_ref().cloned(),
            token: user_auth.clone(),
            enable_access_token,
        };
        app!(
            KafkaEventSender,
            KeycloakAdminClient,
            max_json_payload_size,
            auth
        )
        // for the management service
        .app_data(data.clone())
        // for the admin service
        .app_data(web::Data::new(apps::WebData {
            service: db_service.clone(),
        }))
    })
    .bind(config.bind_addr)
    .context("error starting server")?;

    let main = if let Some(workers) = config.workers {
        main.workers(workers).run()
    } else {
        main.run()
    };

    // run

    if let Some(health) = config.health {
        let health = HealthServer::new(
            health,
            vec![Box::new(service)],
            Some(prometheus::default_registry().clone()),
        );
        futures::try_join!(health.run(), main.err_into())?;
    } else {
        futures::try_join!(main)?;
    }

    // exiting

    Ok(())
}
