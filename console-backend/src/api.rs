use crate::auth::OpenIdClient;
use actix_web::{web, HttpRequest};
use anyhow::Context;
use serde_json::{json, Value};

const SPEC: &str = include_str!("../api/index.yaml");

pub fn spec(req: HttpRequest, client: web::Data<OpenIdClient>) -> anyhow::Result<Value> {
    let mut api: Value = serde_yaml::from_str(SPEC).context("Failed to parse OpenAPI YAML")?;

    let ci = req.connection_info();

    // server

    let url = format!("{}://{}", ci.scheme(), ci.host());
    api["servers"] = json!([{ "url": url, "description": "Drogue Cloud API" }]);

    // SSO

    let url = client.client.config().authorization_endpoint.clone();

    api["security"] = json!({"Drogue Cloud SSO": []});
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
