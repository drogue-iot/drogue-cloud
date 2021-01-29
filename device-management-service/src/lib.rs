pub mod endpoints;
pub mod service;

use crate::service::ManagementService;
use drogue_cloud_service_common::openid::Authenticator;
use envconfig::Envconfig;

#[derive(Debug)]
pub struct WebData<S: ManagementService> {
    pub service: S,
    pub authenticator: Authenticator,
}

#[derive(Envconfig)]
pub struct Config {
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "HEALTH_BIND_ADDR", default = "127.0.0.1:9090")]
    pub health_bind_addr: String,
    #[envconfig(from = "ENABLE_AUTH", default = "true")]
    pub enable_auth: bool,
}

#[macro_export]
macro_rules! crud {
    ($scope:ident, $base:literal, $module:path, $name:ident) => {{
        $scope
            .service({
                let resource = concat!($base, stringify!($name), "s");
                log::debug!("{}", resource);
                web::resource(resource).route(web::post().to({
                    use $module as m;
                    m::create
                }))
            })
            .service({
                let resource = concat!($base, stringify!($name), "s/{", stringify!($name), "_id}");
                log::debug!("{}", resource);

                web::resource(resource)
                    .name(stringify!($name))
                    // "use" is required due to: https://github.com/rust-lang/rust/issues/48067
                    .route(web::get().to({
                        use $module as m;
                        m::read
                    }))
                    .route(web::put().to({
                        use $module as m;
                        m::update
                    }))
                    .route(web::delete().to({
                        use $module as m;
                        m::delete
                    }))
            })
    }};
}

#[macro_export]
macro_rules! app {
    ($data:expr, $enable_auth:expr, $max_json_payload_size:expr) => {{
        let auth_middleware = HttpAuthentication::bearer(|req, auth| {
            let token = auth.token().to_string();

            async {
                let app_data = req.app_data::<web::Data<WebData<PostgresManagementService>>>();
                let app_data = app_data
                    .ok_or_else(|| ServiceError::Internal("Missing app_data instance".into()))?;

                match app_data.authenticator.validate_token(token).await {
                    Ok(_) => Ok(req),
                    Err(AuthenticatorError::Missing) => {
                        Err(ServiceError::Internal("Missing authenticator".into()).into())
                    }
                    Err(AuthenticatorError::Failed) => Err(ServiceError::NotAuthorized.into()),
                }
            }
        });

        let app = App::new()
            .data(web::JsonConfig::default().limit($max_json_payload_size))
            // FIXME: bind to a different port
            .service(endpoints::health::health)
            .app_data($data.clone());

        let app = {
            let scope = web::scope("/api/v1")
                .wrap(Cors::permissive())
                .wrap(Condition::new($enable_auth, auth_middleware));

            let scope = drogue_cloud_device_management_service::crud!(
                scope,
                "/",
                endpoints::tenants,
                tenant
            );

            let scope = drogue_cloud_device_management_service::crud!(
                scope,
                "/tenants/{tenant_id}/",
                endpoints::devices,
                device
            );

            app.service(scope)
        };

        app
    }};
}
