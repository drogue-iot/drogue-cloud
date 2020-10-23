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
        let http =
            std::env::var_os("HTTP_ENDPOINT_URL").and_then(|s| s.to_str().map(|s| s.to_string()));
        let mqtt = (
            std::env::var_os("MQTT_ENDPOINT_HOST").and_then(|s| s.to_str().map(|s| s.to_string())),
            std::env::var_os("MQTT_ENDPOINT_PORT")
                .and_then(|s| s.to_str().and_then(|s| s.parse::<u16>().ok())),
        );

        Ok(Endpoints {
            http: http.map(|url| HttpEndpoint { url }),

            /*
             * Later on, we can do this with:
             * mqtt: try {
             *   MqttEndpoint {
             *       host: mqtt.0?,
             *       port: mqtt.1?,
             *   }
             * }
             */
            mqtt: match mqtt {
                (Some(host), Some(port)) => Some(MqttEndpoint { host, port }),
                _ => None,
            },
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
        let ns = namespace().ok_or_else(|| {
            anyhow::anyhow!("Missing namespace. Consider setting 'NAMESPACE' variable")
        })?;

        Ok(Self { namespace: ns })
    }
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
        .clone()
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
