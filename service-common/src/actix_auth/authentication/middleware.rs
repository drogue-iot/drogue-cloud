use actix_service::{Service, Transform};
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    web::Query,
    Error, HttpMessage,
};

use crate::actix_auth::authentication::{AuthN, Credentials, UsernameAndToken};
use crate::error::ServiceError;

use actix_web_httpauth::extractors::basic::BasicAuth;
use actix_web_httpauth::extractors::bearer::BearerAuth;
use actix_web_httpauth::extractors::AuthExtractor;
use futures_util::future;
use futures_util::future::LocalBoxFuture;
use serde::Deserialize;
use std::rc::Rc;

pub struct AuthMiddleware<S> {
    service: Rc<S>,
    authenticator: AuthN,
}

// 1. Middleware initialization
// Middleware factory is `Transform` trait from actix-service crate
// `S` - type of the next service
// `B` - type of response's body
impl<S, B> Transform<S, ServiceRequest> for AuthN
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

#[derive(Deserialize, Debug)]
struct Token {
    token: String,
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

            // This match a "token" query parameter
            let query_str = req.query_string();
            let token_query_param = Query::<Token>::from_query(query_str);

            log::debug!(
                "Basic: {:?}, Bearer: {:?}, Query: {:?}",
                basic_auth,
                bearer_auth,
                token_query_param
            );

            let credentials = match (basic_auth, bearer_auth, token_query_param) {
                // basic auth is present
                (Ok(basic), Err(_), _) => Ok(Credentials::AccessToken(UsernameAndToken {
                    username: basic.user_id().to_string(),
                    access_token: basic.password().map(|k| k.to_string()),
                })),
                // bearer auth is present
                (Err(_), Ok(bearer), _) => Ok(Credentials::OpenIDToken(bearer.token().to_string())),
                // token query param is present
                (Err(_basic), Err(_bearer), Ok(query)) => {
                    Ok(Credentials::OpenIDToken(query.0.token))
                }

                // No headers and no query param (or both headers are invalid, but both invalid should be met with a Bad request anyway)
                (Err(_basic), Err(_bearer), Err(_query)) => Ok(Credentials::Anonymous),
                // both headers provided and valid -> This never happens, the NGINX load balancer sends back 400 Bad request.
                (Ok(_), Ok(_), _) => Err(ServiceError::InvalidRequest(
                    "Both Basic and Bearer headers are present".to_string(),
                )),
            };

            // authentication
            let auth_result = match credentials {
                Ok(c) => auth.authenticate(c).await,
                Err(_) => Err(ServiceError::AuthenticationError),
            };

            match auth_result {
                Ok(u) => {
                    // insert the UserInformation in the request
                    req.extensions_mut().insert(u);
                    // then forward it to the next service
                    srv.call(req).await
                }
                Err(e) => Err(e.into()),
            }
        })
    }
}
