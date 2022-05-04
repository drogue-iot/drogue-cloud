use actix_service::{Service, Transform};
use drogue_cloud_service_api::webapp::{
    dev::{ServiceRequest, ServiceResponse},
    Error, HttpMessage,
};

use crate::actix_auth::authorization::AuthZ;

use crate::error::ServiceError;
use drogue_cloud_service_api::auth::user::UserInformation;
use futures::future;
use futures::future::LocalBoxFuture;
use std::rc::Rc;

pub struct AuthMiddleware<S> {
    service: Rc<S>,
    authenticator: AuthZ,
}

// 1. Middleware initialization
// Middleware factory is `Transform` trait from actix-service crate
// `S` - type of the next service
// `B` - type of response's body
impl<S, B> Transform<S, ServiceRequest> for AuthZ
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
        let param = self.authenticator.app_param.clone();

        Box::pin(async move {
            // extract user information and application from the request
            let user = req
                .extensions()
                .get::<UserInformation>()
                .cloned()
                .unwrap_or(UserInformation::Anonymous);

            match req.match_info().get(param.as_str()) {
                // authorize
                Some(app) => match auth.authorize(app, user).await {
                    Ok(_) => {
                        // forward request to the next service
                        srv.call(req).await
                    }
                    Err(e) => Err(e.into()),
                },

                // Missing application parameter, cannot authorize
                None => Err(ServiceError::InvalidRequest(String::from(
                    "Missing application parameter",
                ))
                .into()),
            }
        })
    }
}
