use crate::OpenIdClient;
use actix_web::{web, HttpRequest};
use anyhow::Context;
use drogue_cloud_service_api::endpoints::Endpoints;
use serde_json::{json, Value};
use std::borrow::Cow;

const SPEC: &str = include_str!("../api/index.yaml");

pub fn spec(
    req: HttpRequest,
    endpoints: &Endpoints,
    client: web::Data<OpenIdClient>,
) -> anyhow::Result<Value> {
    let mut api: Value = serde_yaml::from_str(SPEC).context("Failed to parse OpenAPI YAML")?;

    let url = endpoints.api.as_ref().map(Cow::from).unwrap_or_else(|| {
        let ci = req.connection_info();
        Cow::from(format!("{}://{}", ci.scheme(), ci.host()))
    });

    // server

    api["servers"] = json!([{ "url": url, "description": "Drogue Cloud API" }]);

    // SSO

    let url = client.client.config().authorization_endpoint.clone();

    api["security"] = json!([{"Drogue Cloud SSO": []}]);
    api["components"]["securitySchemes"] = json!({
        "Drogue Cloud SSO": {
            "type": "oauth2",
            "description": "SSO",
            "flows": {
                "implicit": {
                    "authorizationUrl": url,
                    "scopes": {
                        "openid": "OpenID Connect",
                    }
                }
            }
        },
    });

    // render

    Ok(api)
}
