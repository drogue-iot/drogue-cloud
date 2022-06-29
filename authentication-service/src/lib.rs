pub mod endpoints;
pub mod service;

use crate::service::PostgresAuthenticationService;
use actix_web::web;
use drogue_cloud_service_api::{
    health::BoxedHealthChecked,
    health::HealthChecked,
    webapp::{self as actix_web},
};
use drogue_cloud_service_common::{
    actix::{HttpBuilder, HttpConfig},
    app::run_main,
    health::HealthServerConfig,
    openid::{Authenticator, AuthenticatorConfig},
    openid_auth,
};
use serde::Deserialize;
use service::AuthenticationServiceConfig;

pub struct WebData<S>
where
    S: service::AuthenticationService,
{
    pub service: S,
    pub authenticator: Option<Authenticator>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub oauth: AuthenticatorConfig,

    #[serde(flatten)]
    pub auth_service_config: AuthenticationServiceConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default)]
    pub http: HttpConfig,
}

#[macro_export]
macro_rules! app {
    ($cfg:expr, $data:expr, $enable_auth:expr, $auth:expr) => {{
        use drogue_cloud_service_api::webapp::extras::middleware::Condition;

        $cfg.app_data($data.clone()).service(
            web::scope("/api/v1")
                .wrap(Condition::new($enable_auth, $auth))
                .service(web::resource("/auth").route(web::post().to(endpoints::authenticate)))
                .service(
                    web::resource("/authorize_as").route(web::post().to(endpoints::authorize_as)),
                ),
        )
    }};
}

/// Build the health checks used for this service.
pub fn health_checks(service: PostgresAuthenticationService) -> Vec<Box<dyn HealthChecked>> {
    vec![Box::new(service)]
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let authenticator = config.oauth.into_client().await?;
    let enable_auth = authenticator.is_some();

    let data = web::Data::new(WebData {
        authenticator,
        service: service::PostgresAuthenticationService::new(config.auth_service_config)?,
    });

    let data_service = data.service.clone();

    // main server

    let main = HttpBuilder::new(config.http, move |cfg| {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresAuthenticationService>>>()
            .as_ref()
            .and_then(|data|data.authenticator.as_ref())
        });
        app!(cfg, data, enable_auth, auth);
    })
    .run()?;

    // run

    run_main([main], config.health, [data_service.boxed()]).await?;

    // exiting

    Ok(())
}
