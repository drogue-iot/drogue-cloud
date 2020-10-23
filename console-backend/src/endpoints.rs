use crate::kube::namespace;
use async_trait::async_trait;
use console_common::{Endpoints, HttpEndpoint, MqttEndpoint};
use kube::{Api, Client};
use openshift_openapi::api::route::v1::Route;

use crate::kube::knative;
use serde_json::Value;

pub type EndpointSourceType = Box<dyn EndpointSource + Send + Sync>;

#[async_trait]
pub trait EndpointSource {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints>;
}

pub struct EnvEndpointSource;

#[async_trait]
impl EndpointSource for EnvEndpointSource {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints> {
        Ok(Endpoints {
            http: Some(HttpEndpoint {
                url: "https://http.foo.bar".into(),
            }),
            mqtt: Some(MqttEndpoint {
                host: "mqtt.foo.bar".into(),
                port: 443,
            }),
        })
    }
}

#[derive(Clone, Debug)]
pub struct OpenshiftEndpointSource {
    namespace: String,
}

impl OpenshiftEndpointSource {
    pub fn new() -> anyhow::Result<Self> {
        let ns = namespace().ok_or_else(|| {
            anyhow::anyhow!("Missing namespace. Consider setting 'NAMESPACE' variable")
        })?;

        Ok(Self { namespace: ns })
    }
}

#[async_trait]
impl EndpointSource for OpenshiftEndpointSource {
    async fn eval_endpoints(&self) -> anyhow::Result<Endpoints> {
        let client = Client::try_default().await?;
        let routes: Api<Route> = Api::namespaced(client.clone(), &self.namespace);
        let ksvc: Api<knative::Service> = Api::namespaced(client.clone(), &self.namespace);

        let mqtt = host_from_route(&routes.get("mqtt-endpoint").await?);
        let http = url_from_kservice(&ksvc.get("http-endpoint").await?);

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

fn host_from_route(route: &Route) -> Option<String> {
    route
        .status
        .ingress
        .iter()
        .find_map(|ingress| ingress.host.clone())
        .clone()
}

fn url_from_kservice(ksvc: &knative::Service) -> Option<String> {
    match &ksvc.status {
        Some(status) => match &status.0["url"] {
            Value::String(url) => Some(url.clone()),
            _ => None,
        },
        None => None,
    }
    .map(|url| url.replace("http://", "https://"))
}
