pub mod endpoints;
pub mod service;

use envconfig::Envconfig;

#[derive(Clone)]
pub struct WebData<S>
where
    S: service::AuthenticationService,
{
    pub service: S,
}

#[derive(Clone, Envconfig)]
pub struct Config {
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "HEALTH_BIND_ADDR", default = "127.0.0.1:9090")]
    pub health_bind_addr: String,
}

#[macro_export]
macro_rules! openid_middleware {
    ($auth_var: ident) => {
        let $auth_var = HttpAuthentication::bearer(|req, auth| {
            let token = auth.token().to_string();

            async {
                let authenticator = req.app_data::<web::Data<Authenticator>>();
                log::info!("Authenticator: {:?}", &authenticator);
                let authenticator = authenticator.ok_or_else(|| ServiceError::InternalError {
                    message: "Missing authenticator instance".into(),
                })?;

                match authenticator.validate_token(token).await {
                    Ok(_) => Ok(req),
                    Err(AuthenticatorError::Missing) => Err(ServiceError::InternalError {
                        message: "Missing authenticator".into(),
                    }
                    .into()),
                    Err(AuthenticatorError::Failed) => {
                        Err(ServiceError::AuthenticationError.into())
                    }
                }
            }
        });
    };
}

#[macro_export]
macro_rules! app {
    ($data:expr, $max_json_payload_size:expr, $auth_data: expr, $auth_var: ident) => {
        App::new()
            .data(web::JsonConfig::default().limit($max_json_payload_size))
            .app_data($auth_data.clone())
            .service(
                web::scope("/api/v1")
                    .wrap($auth_var)
                    .service(endpoints::authenticate),
            )
            //fixme : bind to a different port
            .service(endpoints::health)
            .data($data.clone())
    };
}
