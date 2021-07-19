pub mod endpoints;
pub mod service;

use crate::service::AuthorizationServiceConfig;
use drogue_cloud_api_key_service::service::KeycloakApiKeyServiceConfig;
use drogue_cloud_service_common::{defaults, health::HealthServerConfig, openid::Authenticator};
use serde::Deserialize;

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
    pub health: HealthServerConfig,
}

#[macro_export]
macro_rules! app {
    ($data:expr, $api_key_ty:ty, $api_key:expr, $max_json_payload_size:expr, $enable_auth: expr, $auth: expr) => {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .app_data(web::Data::new(
                web::JsonConfig::default().limit($max_json_payload_size),
            ))
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
