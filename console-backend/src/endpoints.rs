use crate::kube::knative;

use async_trait::async_trait;
use drogue_cloud_console_common::{Endpoints, HttpEndpoint, MqttEndpoint};
use envconfig::Envconfig;
use kube::{Api, Client};
use openshift_openapi::api::route::v1::Route;
use serde_json::Value;
use std::fmt::Debug;

pub type EndpointSourceType = Box<dyn EndpointSource + Send + Sync>;

#[async_trait]
pub trait EndpointSource: Debug {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints>;
}

#[derive(Debug, Envconfig)]
pub struct EndpointConfig {
    #[envconfig(from = "ENDPOINT_SOURCE", default = "env")]
    pub source: String,
    #[envconfig(from = "HTTP_ENDPOINT_URL")]
    pub http_url: Option<String>,
    #[envconfig(from = "MQTT_ENDPOINT_HOST")]
    pub mqtt_host: Option<String>,
    #[envconfig(from = "MQTT_ENDPOINT_PORT", default = "8883")]
    pub mqtt_port: u16,
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

        Ok(Endpoints { http, mqtt })
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
        let ksvc: Api<knative::Service> = Api::namespaced(client.clone(), &self.namespace);

        let mqtt = host_from_route(&routes.get("mqtt-endpoint").await?);
        let http = url_from_kservice(&ksvc.get("http-endpoint").await?, true);

        let result = Endpoints {
            http: http.map(|url| HttpEndpoint { url }),
            mqtt: mqtt.map(|mqtt| MqttEndpoint {
                host: mqtt,
                port: 443,
            }),
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
