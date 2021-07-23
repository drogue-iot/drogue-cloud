pub mod endpoints;
pub mod service;
pub mod utils;

use crate::service::management::ManagementService;
use drogue_cloud_service_common::{defaults, health::HealthServerConfig, openid::Authenticator};
use serde::Deserialize;

#[derive(Debug)]
pub struct WebData<S: ManagementService> {
    pub service: S,
    pub authenticator: Option<Authenticator>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,

    #[serde(default)]
    pub health: HealthServerConfig,
}

#[macro_export]
macro_rules! crud {
    ($sender:ty, $scope:ident, $base:literal, $module:path, $name:ident) => {{
        $scope
            .service({
                let resource = concat!($base, stringify!($name), "s");
                log::debug!("{}", resource);
                web::resource(resource)
                    // create resources
                    .route(web::post().to({
                        use $module as m;
                        m::create::<$sender>
                    }))
                    // list resources
                    .route(web::get().to({
                        use $module as m;
                        m::list::<$sender>
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
                        m::read::<$sender>
                    }))
                    .route(web::put().to({
                        use $module as m;
                        m::update::<$sender>
                    }))
                    .route(web::delete().to({
                        use $module as m;
                        m::delete::<$sender>
                    }))
            })
    }};
}

#[macro_export]
macro_rules! app {
    ($sender:ty, $enable_auth:expr, $max_json_payload_size:expr, $auth:expr) => {{
        let app = App::new()
            .wrap(actix_web::middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit($max_json_payload_size));

        let app = {
            let scope = web::scope("/api/registry/v1alpha1")
                .wrap(Condition::new($enable_auth, $auth.clone()))
                .wrap(Cors::permissive());

            let scope = drogue_cloud_device_management_service::crud!(
                $sender,
                scope,
                "",
                endpoints::apps,
                app
            );

            let scope = drogue_cloud_device_management_service::crud!(
                $sender,
                scope,
                "apps/{app}/",
                endpoints::devices,
                device
            );

            app.service(scope)
        };

        let app =
            {
                let scope = web::scope("/api/admin/v1alpha1")
                    .wrap(Condition::new($enable_auth, $auth))
                    .wrap(Cors::permissive());

                let scope = scope.service(
                    web::resource("/apps/{appId}/transfer-ownership")
                        .route(
                            web::put()
                                .to(apps::transfer::<service::PostgresManagementService<$sender>>),
                        )
                        .route(
                            web::delete()
                                .to(apps::cancel::<service::PostgresManagementService<$sender>>),
                        ),
                );

                let scope = scope.service(web::resource("/apps/{appId}/accept-ownership").route(
                    web::put().to(apps::accept::<service::PostgresManagementService<$sender>>),
                ));

                let scope =
                    scope.service(
                        web::resource("/apps/{appId}/members")
                            .route(web::get().to(apps::get_members::<
                                service::PostgresManagementService<$sender>,
                            >))
                            .route(web::put().to(apps::set_members::<
                                service::PostgresManagementService<$sender>,
                            >)),
                    );

                app.service(scope)
            };

        app
    }};
}
