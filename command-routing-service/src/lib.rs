pub mod endpoints;
pub mod service;

use crate::service::{postgres::PostgresServiceConfiguration, CommandRoutingService};
use actix_web::web;
use drogue_client::registry;
use drogue_cloud_service_api::{
    webapp::{self as actix_web},
};
use drogue_cloud_service_common::{
    actix::http::{HttpBuilder, HttpConfig},
    app::{Startup, StartupExt},
    auth::openid::{Authenticator, AuthenticatorConfig},
    client::ClientConfig,
    openid_auth,
};
use futures::FutureExt;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub oauth: AuthenticatorConfig,

    pub service: PostgresServiceConfiguration,

    pub instance: String,

    pub registry: ClientConfig,

    #[serde(default)]
    pub http: HttpConfig,
}

#[macro_export]
macro_rules! app {
    ($cfg:expr, $data:expr, $auth: expr) => {{
        use $crate::endpoints;

        $cfg.app_data($data.clone()).service(
            web::scope("/api/routes/v1alpha1")
                .wrap($auth)
                .service(
                    web::resource("/routes/{application}/{command}")
                        .route(web::get().to(endpoints::get)),
                )
                .service(web::resource("/sessions").route(web::put().to(endpoints::init)))
                .service(
                    web::resource("/sessions/{session}").route(web::post().to(endpoints::ping)),
                )
                .service(
                    web::resource("/routes/{session}/states/{application}/{command}")
                        .route(web::put().to(endpoints::create))
                        .route(web::delete().to(endpoints::delete)),
                ),
        )
    }};
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    log::info!("Authenticator: {authenticator:?}");
    let authenticator = authenticator.map(web::Data::new);

    // set up registry client
    let registry: registry::v1::Client = config.registry.into_client().await?;

    // service

    let service =
        service::postgres::PostgresCommandRoutingService::new(config.service, registry)?;
    startup.check(service.clone());

    let pruner = service::postgres::run_pruner(service.clone()).boxed();

    let service: Arc<dyn CommandRoutingService> = Arc::new(service);
    let service: web::Data<dyn CommandRoutingService> = web::Data::from(service);

    // monitoring

    // main server

    let main = HttpBuilder::new(config.http, Some(startup.runtime_config()), move |cfg| {
        let auth = openid_auth!(req -> {
            req
                .app_data::<web::Data<Authenticator>>().as_ref().map(|s|s.get_ref())
        });
        let mut app = app!(cfg, service, auth);

        if let Some(auth) = &authenticator {
            app = app.app_data(auth.clone())
        }

        app.app_data(service.clone());
    })
    .run()?;

    // run

    startup.spawn(main);
    startup.spawn(pruner);

    // exiting

    Ok(())
}
