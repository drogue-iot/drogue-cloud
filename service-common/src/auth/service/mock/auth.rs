use std::pin::Pin;
use std::task::{Context, Poll};

use actix_service::{Service, Transform};
use drogue_cloud_service_api::webapp::{dev::ServiceRequest, dev::ServiceResponse, Error};
use futures::future::{ok, Ready};
use futures::Future;

#[derive(Clone)]
pub struct MockAuthenticator;

impl<S, B> Transform<S, ServiceRequest> for MockAuthenticator
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = MockAuthenticatorMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(MockAuthenticatorMiddleware { service })
    }
}

pub struct MockAuthenticatorMiddleware<S> {
    service: S,
}

type MockAuthenticatorFuture<R, E> = Pin<Box<dyn Future<Output = Result<R, E>>>>;

impl<S, B> Service<ServiceRequest> for MockAuthenticatorMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = MockAuthenticatorFuture<Self::Response, Self::Error>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);

        Box::pin(async move { fut.await })
    }
}

#[macro_export]
macro_rules! mock_auth {
    () => {
        $crate::auth::MockAuthenticator
    };
}
