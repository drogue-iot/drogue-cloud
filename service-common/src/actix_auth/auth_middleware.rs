use actix_service::{Service, Transform};
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    Error,
};

use crate::actix_auth::{Auth, Credentials, UsernameAndApiKey};
use crate::error::ServiceError;

use actix_web_httpauth::extractors::basic::BasicAuth;
use actix_web_httpauth::extractors::bearer::BearerAuth;
use actix_web_httpauth::extractors::AuthExtractor;
use futures_util::future;
use futures_util::future::LocalBoxFuture;
use std::rc::Rc;

pub struct AuthMiddleware<S> {
    service: Rc<S>,
    authenticator: Auth,
}

// 1. Middleware initialization
// Middleware factory is `Transform` trait from actix-service crate
// `S` - type of the next service
// `B` - type of response's body
impl<S, B> Transform<S, ServiceRequest> for Auth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = S::Error;
    type Transform = AuthMiddleware<S>;
    type InitError = ();
    type Future = future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        future::ok(AuthMiddleware {
            service: Rc::new(service),
            authenticator: self.clone(),
        })
    }
}

// 2. Middleware's call method gets called with normal request.
impl<S, B> Service<ServiceRequest> for AuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    actix_service::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = Rc::clone(&self.service);
        let auth = self.authenticator.clone();

        Box::pin(async move {
            let basic_auth = BasicAuth::from_service_request(&req).await;
            let bearer_auth = BearerAuth::from_service_request(&req).await;

            let app = req.match_info().get("application").unwrap();

            let credentials = match (basic_auth, bearer_auth) {
                // basic auth is present
                (Ok(basic), Err(_)) => Ok(Credentials::ApiKey(UsernameAndApiKey {
                    username: basic.user_id().to_string(),
                    key: basic.password().map(|k| k.to_string()),
                })),
                // bearer auth is present
                (Err(_), Ok(bearer)) => Ok(Credentials::Token(bearer.token().to_string())),
                // No headers (or both are invalid)
                // fixme : how to differentiate between invalid request and None was provided ???
                (Err(err_basic), Err(err_bearer)) => Ok(Credentials::Anonymous),
                // both headers provided and valid
                (Ok(_), Ok(_)) => Err(ServiceError::InvalidRequest(
                    "Both Basic and Bearer headers are present".to_string(),
                )),
            };

            // authentication
            let auth_result = match credentials {
                Ok(c) => auth.authenticate_and_authorize(app.to_string(), c).await,
                Err(_) => Err(ServiceError::AuthenticationError),
            };

            match auth_result {
                Ok(_) => srv.call(req).await,
                Err(e) => Err(e.into()),
            }
        })
    }
}
