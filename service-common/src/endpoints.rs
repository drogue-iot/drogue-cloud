use crate::kube::knative;
use async_trait::async_trait;
use envconfig::Envconfig;
use kube::{Api, Client};
use openshift_openapi::api::route::v1::Route;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Debug;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpEndpoint {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MqttEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SsoEndpoint {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Endpoints {
    pub http: Option<HttpEndpoint>,
    pub mqtt: Option<MqttEndpoint>,
    pub issuer_url: Option<String>,
    pub redirect_url: Option<String>,
}

pub type EndpointSourceType = Box<dyn EndpointSource + Send + Sync>;

#[async_trait]
pub trait EndpointSource: Debug {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints>;
}

#[derive(Debug, Envconfig)]
pub struct EndpointConfig {
    #[envconfig(from = "ISSUER_URL")]
    pub issuer_url: String,
    #[envconfig(from = "REDIRECT_URL")]
    pub redirect_url: String,
    #[envconfig(from = "HTTP_ENDPOINT_URL")]
    pub http_url: Option<String>,
    #[envconfig(from = "MQTT_ENDPOINT_HOST")]
    pub mqtt_host: Option<String>,
    #[envconfig(from = "MQTT_ENDPOINT_PORT", default = "8883")]
    pub mqtt_port: u16,
}

pub fn create_endpoint_source() -> anyhow::Result<EndpointSourceType> {
    let source = std::env::var_os("ENDPOINT_SOURCE").unwrap_or_else(|| "env".into());
    match source.to_str() {
        Some("openshift") => Ok(Box::new(OpenshiftEndpointSource::new()?)),
        Some("kubernetes") => Ok(Box::new(KubernetesEndpointSource::new()?)),
        Some("env") => Ok(Box::new(EnvEndpointSource(Envconfig::init_from_env()?))),
        other => Err(anyhow::anyhow!(
            "Unsupported endpoint source: '{:?}'",
            other
        )),
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
            .map(|url| HttpEndpoint { url: url.clone() });
        let mqtt = self.0.mqtt_host.as_ref().map(|host| MqttEndpoint {
            host: host.clone(),
            port: self.0.mqtt_port,
        });

        Ok(Endpoints {
            http,
            mqtt,
            issuer_url: Some(self.0.issuer_url.clone()),
            redirect_url: Some(self.0.redirect_url.clone()),
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

#[async_trait]
impl EndpointSource for OpenshiftEndpointSource {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints> {
        let client = Client::try_default().await?;
        let routes: Api<Route> = Api::namespaced(client.clone(), &self.namespace);

        let mqtt = host_from_route(&routes.get("mqtt-endpoint").await?);
        let http = url_from_route(&routes.get("http-endpoint").await?);
        let sso = url_from_route(&routes.get("keycloak").await?);
        let frontend = url_from_route(&routes.get("console").await?);

        let result = Endpoints {
            http: http.map(|url| HttpEndpoint { url }),
            mqtt: mqtt.map(|mqtt| MqttEndpoint {
                host: mqtt,
                port: 443,
            }),
            issuer_url: sso.map(|sso| format!("{}/auth/realms/drogue", sso)),
            redirect_url: frontend,
        };

        Ok(result)
    }
}

#[derive(Clone, Debug)]
pub struct KubernetesEndpointSource {
    namespace: String,
}

impl KubernetesEndpointSource {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            namespace: namespace()?,
        })
    }
}

fn namespace() -> anyhow::Result<String> {
    crate::kube::namespace()
        .ok_or_else(|| anyhow::anyhow!("Missing namespace. Consider setting 'NAMESPACE' variable"))
}

#[async_trait]
impl EndpointSource for KubernetesEndpointSource {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints> {
        let client = Client::try_default().await?;
        let ksvc: Api<knative::Service> = Api::namespaced(client.clone(), &self.namespace);

        let http = url_from_kservice(&ksvc.get("http-endpoint").await?, false);

        let result = Endpoints {
            http: http.map(|url| HttpEndpoint { url }),
            mqtt: None,
            issuer_url: None,
            redirect_url: None,
        };

        Ok(result)
    }
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

fn url_from_kservice(ksvc: &knative::Service, force_tls: bool) -> Option<String> {
    let r = match &ksvc.status {
        Some(status) => match &status.0["url"] {
            Value::String(url) => Some(url.clone()),
            _ => None,
        },
        None => None,
    };

    if force_tls {
        r.map(|url| url.replace("http://", "https://"))
    } else {
        r
    }
}
