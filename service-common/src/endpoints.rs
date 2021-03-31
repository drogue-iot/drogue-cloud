use async_trait::async_trait;
use drogue_cloud_service_api::endpoints::*;
use envconfig::Envconfig;
use futures::{stream::FuturesUnordered, StreamExt};
use kube::{Api, Client};
use openshift_openapi::api::route::v1::Route;
use std::fmt::Debug;

const DEFAULT_REALM: &str = "drogue";

pub type EndpointSourceType = Box<dyn EndpointSource + Send + Sync>;

#[async_trait]
pub trait EndpointSource: Debug {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints>;
}

/// This is the endpoint configuration when using the [`EnvEndpointSource`].
#[derive(Debug, Envconfig)]
pub struct EndpointConfig {
    #[envconfig(from = "ISSUER_URL")]
    pub issuer_url: Option<String>,
    #[envconfig(from = "SSO_URL")]
    pub sso_url: String,
    #[envconfig(from = "REDIRECT_URL")]
    pub redirect_url: Option<String>,
    #[envconfig(from = "HTTP_ENDPOINT_URL")]
    pub http_url: Option<String>,
    #[envconfig(from = "MQTT_ENDPOINT_HOST")]
    pub mqtt_host: Option<String>,
    #[envconfig(from = "MQTT_ENDPOINT_PORT", default = "8883")]
    pub mqtt_port: u16,
    #[envconfig(from = "MQTT_INTEGRATION_HOST")]
    pub mqtt_integration_host: Option<String>,
    #[envconfig(from = "MQTT_INTEGRATION_PORT", default = "8883")]
    pub mqtt_integration_port: u16,
    #[envconfig(from = "DEVICE_REGISTRY_URL")]
    pub device_registry_url: Option<String>,
    #[envconfig(from = "COMMAND_ENDPOINT_URL")]
    pub command_url: Option<String>,
    #[envconfig(from = "LOCAL_CERTS", default = "false")]
    pub local_certs: bool,
}

pub async fn eval_endpoints() -> anyhow::Result<Endpoints> {
    create_endpoint_source()?.eval_endpoints().await
}

pub fn create_endpoint_source() -> anyhow::Result<EndpointSourceType> {
    let source = std::env::var_os("ENDPOINT_SOURCE").unwrap_or_else(|| "env".into());
    match source.to_str() {
        Some("openshift") => Ok(Box::new(OpenshiftEndpointSource::new()?)),
        Some("env") => Ok(Box::new(EnvEndpointSource(Envconfig::init_from_env()?))),
        other => Err(anyhow::anyhow!(
            "Unsupported endpoint source: '{:?}'",
            other
        )),
    }
}

/// Split demo entries
///
/// Format: ENV=Label of Demo=target;Next demo=target2
fn split_demos(str: &str) -> Vec<(String, String)> {
    let mut demos = Vec::new();

    for demo in str.split(';') {
        if let [label, target] = demo.splitn(2, '=').collect::<Vec<&str>>().as_slice() {
            demos.push((label.to_string(), target.to_string()))
        }
    }

    demos
}

fn get_demos() -> Vec<(String, String)> {
    match std::env::var("DEMOS") {
        Ok(value) => split_demos(&value),
        _ => vec![],
    }
}

#[derive(Debug)]
pub struct EnvEndpointSource(pub EndpointConfig);

#[async_trait]
impl EndpointSource for EnvEndpointSource {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints> {
        let http = self
            .0
            .http_url
            .as_ref()
            .cloned()
            .map(|url| HttpEndpoint { url });
        let mqtt = self.0.mqtt_host.as_ref().map(|host| MqttEndpoint {
            host: host.clone(),
            port: self.0.mqtt_port,
        });
        let mqtt_integration = self
            .0
            .mqtt_integration_host
            .as_ref()
            .map(|host| MqttEndpoint {
                host: host.clone(),
                port: self.0.mqtt_integration_port,
            });
        let registry = self
            .0
            .device_registry_url
            .as_ref()
            .cloned()
            .map(|url| RegistryEndpoint { url });

        let sso = self.0.sso_url.clone();
        let issuer_url = self
            .0
            .issuer_url
            .as_ref()
            .cloned()
            .unwrap_or_else(|| crate::utils::sso_to_issuer_url(&sso, DEFAULT_REALM));

        Ok(Endpoints {
            http,
            mqtt,
            mqtt_integration,
            sso: Some(sso),
            issuer_url: Some(issuer_url),
            redirect_url: self.0.redirect_url.as_ref().cloned(),
            registry,
            command_url: self.0.command_url.as_ref().cloned(),
            demos: get_demos(),
            local_certs: self.0.local_certs,
        })
    }
}

#[derive(Clone, Debug)]
pub struct OpenshiftEndpointSource {
    namespace: String,
}

impl OpenshiftEndpointSource {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            namespace: namespace()?,
        })
    }
}

/// lookup a URL from a route
async fn lookup_route(
    routes: &Api<Route>,
    label: String,
    target: String,
) -> anyhow::Result<(String, Option<String>)> {
    let route = routes.get(&target).await?;
    let url = url_from_route(&route);
    Ok((label, url))
}

#[async_trait]
impl EndpointSource for OpenshiftEndpointSource {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints> {
        let client = Client::try_default().await?;
        let routes: Api<Route> = Api::namespaced(client.clone(), &self.namespace);

        let mqtt = host_from_route(&routes.get("mqtt-endpoint").await?);
        let mqtt_integration = host_from_route(&routes.get("mqtt-integration").await?);
        let http = url_from_route(&routes.get("http-endpoint").await?);
        let command = url_from_route(&routes.get("command-endpoint").await?);
        let sso = url_from_route(&routes.get("keycloak").await?);
        let frontend = url_from_route(&routes.get("console").await?);
        let registry = url_from_route(&routes.get("registry").await?);

        let demos = get_demos()
            .iter()
            .map(|(label, target)| lookup_route(&routes, label.clone(), target.clone()))
            .collect::<FuturesUnordered<_>>()
            .filter_map(|r: Result<(String, Option<String>), anyhow::Error>| async {
                // silently filter out errors
                match r {
                    Ok((label, Some(url))) => Some((label, url)),
                    _ => None,
                }
            })
            .collect::<Vec<_>>()
            .await;

        let result = Endpoints {
            http: http.map(|url| HttpEndpoint { url }),
            mqtt: mqtt.map(|mqtt| MqttEndpoint {
                host: mqtt,
                port: 443,
            }),
            mqtt_integration: mqtt_integration.map(|mqtt| MqttEndpoint {
                host: mqtt,
                port: 443,
            }),
            issuer_url: sso
                .as_ref()
                .map(|sso| crate::utils::sso_to_issuer_url(&sso, DEFAULT_REALM)),
            command_url: command,
            sso,
            redirect_url: frontend,
            registry: registry.map(|url| RegistryEndpoint { url }),
            demos,
            local_certs: false,
        };

        Ok(result)
    }
}

fn namespace() -> anyhow::Result<String> {
    crate::kube::namespace()
        .ok_or_else(|| anyhow::anyhow!("Missing namespace. Consider setting 'NAMESPACE' variable"))
}

fn host_from_route(route: &Route) -> Option<String> {
    route
        .status
        .ingress
        .iter()
        .find_map(|ingress| ingress.host.clone())
}

fn url_from_route(route: &Route) -> Option<String> {
    host_from_route(route).map(|host| format!("https://{}", host))
}
