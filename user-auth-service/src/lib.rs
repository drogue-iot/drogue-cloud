pub mod endpoints;
pub mod service;

use actix_web::{web, App, HttpServer};
use drogue_cloud_api_key_service::service::KeycloakApiKeyServiceConfig;
use drogue_cloud_api_key_service::{
    endpoints::WebData as KeycloakWebData, service::KeycloakApiKeyService,
};
use drogue_cloud_service_common::{
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::Authenticator,
    openid_auth,
};
use futures::TryFutureExt;
use serde::Deserialize;
use service::AuthorizationServiceConfig;

pub struct WebData<S>
where
    S: service::AuthorizationService,
{
    pub service: S,
    pub authenticator: Option<Authenticator>,
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub service: AuthorizationServiceConfig,
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,

    pub keycloak: KeycloakApiKeyServiceConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,
}

#[macro_export]
macro_rules! app {
    ($data:expr, $api_key_ty:ty, $api_key:expr, $max_json_payload_size:expr, $enable_auth: expr, $auth: expr) => {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit($max_json_payload_size))
            .app_data($data.clone())
            .app_data($api_key.clone())
            .service(
                web::scope("/api")
                    .wrap(actix_web::middleware::Condition::new(
                        $enable_auth,
                        $auth.clone(),
                    ))
                    .service(web::scope("/v1/user").service(endpoints::authorize))
                    .service(web::resource("/user/v1alpha1/authn").route(
                        web::post().to(drogue_cloud_api_key_service::endpoints::authenticate::<
                            $api_key_ty,
                        >),
                    )),
            )
    };
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let max_json_payload_size = config.max_json_payload_size;
    let enable_auth = config.enable_auth;

    let authenticator = if enable_auth {
        Some(Authenticator::new().await?)
    } else {
        None
    };

    let data = web::Data::new(WebData {
        authenticator,
        service: service::PostgresAuthorizationService::new(config.service)?,
    });

    let api_key = web::Data::new(KeycloakWebData {
        service: KeycloakApiKeyService::new(config.keycloak)?,
    });

    let data_service = data.service.clone();

    // main server

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresAuthorizationService>>>()
            .as_ref()
            .and_then(|data|data.authenticator.as_ref())
        });
        app!(
            data,
            KeycloakApiKeyService,
            api_key,
            max_json_payload_size,
            enable_auth,
            auth
        )
    })
    .bind(config.bind_addr)?
    .run();

    // run

    if let Some(health) = config.health {
        let health = HealthServer::new(health, vec![Box::new(data_service)]);
        futures::try_join!(health.run(), main.err_into())?;
    } else {
        futures::try_join!(main)?;
    }

    // exiting

    Ok(())
}
