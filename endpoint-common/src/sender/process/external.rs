use cloudevents::{
    binding::reqwest::{RequestBuilderExt, RequestSerializer},
    message::StructuredDeserializer,
    Event,
};
use drogue_client::registry::v1::{
    Authentication, ContentMode, ExternalEndpoint, RequestType, TlsOptions,
};
use drogue_cloud_service_common::reqwest::to_method;
use http::{header::HeaderName, HeaderMap, HeaderValue, Method};
use lru::LruCache;
use reqwest::{Certificate, Url};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::Mutex;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum ExternalError {
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Cloud event error: {0}")]
    CloudEvent(#[from] cloudevents::message::Error),
}

#[derive(Clone, Debug, Deserialize)]
pub struct ExternalClientPoolConfig {
    pub capacity: usize,
}

impl Default for ExternalClientPoolConfig {
    fn default() -> Self {
        Self { capacity: 8 }
    }
}

#[derive(Clone, Debug)]
pub struct ExternalClientPool {
    cache: Arc<Mutex<LruCache<Option<TlsOptions>, ExternalClient>>>,
}

impl Default for ExternalClientPool {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl ExternalClientPool {
    pub fn new(config: ExternalClientPoolConfig) -> Self {
        let cache = Arc::new(Mutex::new(LruCache::new(config.capacity)));
        Self { cache }
    }

    pub async fn get(&self, endpoint: &ExternalEndpoint) -> Result<ExternalClient, ExternalError> {
        let mut cache = self.cache.lock().await;
        if let Some(client) = cache.get(&endpoint.tls) {
            Ok(client.clone())
        } else {
            let client = ExternalClient::new(endpoint.tls.as_ref())?;
            cache.put(endpoint.tls.clone(), client.clone());
            Ok(client)
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExternalClient {
    client: reqwest::Client,
}

#[derive(Clone, Debug)]
pub enum RequestPayload {
    CloudEvent {
        mode: ContentMode,
        event: cloudevents::Event,
    },
}

/// A trait to easily translate type and event into payload
pub trait IntoPayload {
    fn to_payload(&self, event: cloudevents::Event) -> RequestPayload;
}

impl IntoPayload for RequestType {
    fn to_payload(&self, event: Event) -> RequestPayload {
        match self {
            Self::CloudEvent { mode } => RequestPayload::CloudEvent { event, mode: *mode },
        }
    }
}

impl ExternalClient {
    pub fn new(tls: Option<&TlsOptions>) -> Result<Self, reqwest::Error> {
        let mut client = reqwest::Client::builder();

        if let Some(tls) = tls {
            if tls.insecure {
                client = client
                    .danger_accept_invalid_certs(true)
                    .danger_accept_invalid_hostnames(true);
            }
            if let Some(cert) = &tls.certificate {
                client = client
                    .tls_built_in_root_certs(false)
                    .add_root_certificate(Certificate::from_pem(cert.as_bytes())?);
            }
        }

        Ok(Self {
            client: client.build()?,
        })
    }

    pub async fn process(
        &self,
        payload: RequestPayload,
        endpoint: &ExternalEndpoint,
    ) -> Result<reqwest::Response, ExternalError> {
        let method = to_method(endpoint.method.as_deref().unwrap_or(""))
            .map_err(ExternalError::InvalidConfiguration)?
            .unwrap_or(Method::POST);

        let url = Url::parse(&endpoint.url)
            .map_err(|err| ExternalError::InvalidConfiguration(format!("Invalid URL: {err}")))?;

        // new request

        let mut request = self.client.request(method, url);

        // headers

        let headers = endpoint.headers.len();
        if headers > 0 {
            let mut headers = HeaderMap::with_capacity(headers);
            for entry in &endpoint.headers {
                let name = HeaderName::try_from(&entry.name).map_err(|err| {
                    ExternalError::InvalidConfiguration(format!("Invalid header name: {err}"))
                })?;
                headers.insert(
                    name,
                    HeaderValue::from_str(&entry.value).map_err(|err| {
                        ExternalError::InvalidConfiguration(format!("Invalid header value: {err}"))
                    })?,
                );
            }
            request = request.headers(headers);
        }

        match &endpoint.auth {
            Authentication::None => {}
            Authentication::Basic { username, password } => {
                request = request.basic_auth(username, password.as_ref());
            }
            Authentication::Bearer { token } => {
                request = request.bearer_auth(token);
            }
        }

        request = request.timeout(endpoint.timeout.unwrap_or(DEFAULT_TIMEOUT));

        // cloud event mapping

        match payload {
            RequestPayload::CloudEvent {
                mode: ContentMode::Binary,
                event,
            } => {
                request = request.event(event)?;
            }
            RequestPayload::CloudEvent {
                mode: ContentMode::Structured,
                event,
            } => {
                request = StructuredDeserializer::deserialize_structured(
                    event,
                    RequestSerializer::new(request),
                )?;
                // request = cloudevents::binding::reqwest::event_to_request(event, request)?;
            }
        }

        // execute

        request.send().await.map_err(ExternalError::Request)
    }
}

#[cfg(test)]
mod test {
    use drogue_client::registry::v1::TlsOptions;
    use std::collections::HashMap;

    #[test]
    fn test_key() {
        let key1a = None;
        let key1b = None;
        let key2a = Some(TlsOptions {
            insecure: false,
            certificate: None,
        });
        let key2b = Some(TlsOptions {
            insecure: false,
            certificate: None,
        });
        let key3 = Some(TlsOptions {
            insecure: true,
            certificate: None,
        });

        let mut map = HashMap::new();

        map.insert(key1a.clone(), 100);
        map.insert(key1b.clone(), 1); // should override
        map.insert(key2a.clone(), 200);
        map.insert(key2b.clone(), 2); // should override
        map.insert(key3.clone(), 3);

        assert_eq!(Some(1), map.get(&key1a).cloned());
        assert_eq!(Some(1), map.get(&key1b).cloned());
        assert_eq!(Some(2), map.get(&key2a).cloned());
        assert_eq!(Some(2), map.get(&key2b).cloned());
        assert_eq!(Some(3), map.get(&key3).cloned());
    }
}
