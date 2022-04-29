use drogue_cloud_service_api::{
    auth::user::UserInformation,
    webapp::{
        self,
        dev::{Service, ServiceResponse},
        test, HttpMessage,
    },
};
use drogue_cloud_service_common::openid::ExtendedClaims;
use serde_json::json;

pub fn user<S: AsRef<str>>(id: S) -> UserInformation {
    let claims: ExtendedClaims = serde_json::from_value(json!({
        "sub": id.as_ref(),
        "iss": "drogue:iot:test",
        "aud": "drogue",
        "exp": 0,
        "iat": 0,
    }))
    .unwrap();

    UserInformation::Authenticated(claims.into())
}

pub async fn call_http<S, B, E>(
    app: &S,
    user: &UserInformation,
    req: test::TestRequest,
) -> S::Response
where
    S: Service<webapp::http::Request, Response = ServiceResponse<B>, Error = E>,
    E: std::fmt::Debug,
{
    let req = req.to_request();
    req.extensions_mut().insert(user.clone());

    test::call_service(app, req).await
}
