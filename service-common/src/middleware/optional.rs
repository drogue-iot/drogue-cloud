// this originates from the actix-web project: https://github.com/actix/actix-web
use actix_service::{Service, Transform};
use drogue_cloud_service_api::webapp::utils::future::Either;
use futures_util::future::{FutureExt, LocalBoxFuture};
use std::task::{Context, Poll};

pub struct Optional<T> {
    transformer: Option<T>,
}

impl<T> Optional<T> {
    pub fn new(transformer: Option<T>) -> Self {
        Self { transformer }
    }
}

impl<S, T, Req> Transform<S, Req> for Optional<T>
where
    S: Service<Req> + 'static,
    T: Transform<S, Req, Response = S::Response, Error = S::Error>,
    T::Future: 'static,
    T::InitError: 'static,
    T::Transform: 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Transform = OptionalMiddleware<T::Transform, S>;
    type InitError = T::InitError;
    type Future = LocalBoxFuture<'static, Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        if let Some(transformer) = &self.transformer {
            let fut = transformer.new_transform(service);
            async move {
                let wrapped_svc = fut.await?;
                Ok(OptionalMiddleware::Enable(wrapped_svc))
            }
            .boxed_local()
        } else {
            async move { Ok(OptionalMiddleware::Disable(service)) }.boxed_local()
        }
    }
}

pub enum OptionalMiddleware<E, D> {
    Enable(E),
    Disable(D),
}

impl<E, D, Req> Service<Req> for OptionalMiddleware<E, D>
where
    E: Service<Req>,
    D: Service<Req, Response = E::Response, Error = E::Error>,
{
    type Response = E::Response;
    type Error = E::Error;
    type Future = Either<E::Future, D::Future>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self {
            OptionalMiddleware::Enable(service) => service.poll_ready(cx),
            OptionalMiddleware::Disable(service) => service.poll_ready(cx),
        }
    }

    fn call(&self, req: Req) -> Self::Future {
        match self {
            OptionalMiddleware::Enable(service) => Either::left(service.call(req)),
            OptionalMiddleware::Disable(service) => Either::right(service.call(req)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::fut::ok;
    use actix_service::IntoService;
    use drogue_cloud_service_api::webapp::{
        dev::*,
        error::Result,
        middleware::*,
        test::{self, TestRequest},
        HttpResponse,
    };
    use http::{header::CONTENT_TYPE, HeaderValue, StatusCode};

    #[allow(clippy::unnecessary_wraps)]
    fn render_500<B>(mut res: ServiceResponse<B>) -> Result<ErrorHandlerResponse<B>> {
        res.response_mut()
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("0001"));

        Ok(ErrorHandlerResponse::Response(res.map_into_left_body()))
    }

    #[actix_rt::test]
    async fn test_handler_enabled() {
        let srv = |req: ServiceRequest| {
            ok(req.into_response(HttpResponse::InternalServerError().finish()))
        };

        let mw = Compat::new(
            ErrorHandlers::new().handler(StatusCode::INTERNAL_SERVER_ERROR, render_500),
        );

        let mw = Condition::new(true, mw)
            .new_transform(srv.into_service())
            .await
            .unwrap();
        let resp = test::call_service(&mw, TestRequest::default().to_srv_request()).await;
        assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "0001");
    }

    #[actix_rt::test]
    async fn test_handler_disabled() {
        let srv = |req: ServiceRequest| {
            ok(req.into_response(HttpResponse::InternalServerError().finish()))
        };

        let mw = Compat::new(
            ErrorHandlers::new().handler(StatusCode::INTERNAL_SERVER_ERROR, render_500),
        );

        let mw = Condition::new(false, mw)
            .new_transform(srv.into_service())
            .await
            .unwrap();

        let resp = test::call_service(&mw, TestRequest::default().to_srv_request()).await;
        assert_eq!(resp.headers().get(CONTENT_TYPE), None);
    }
}
