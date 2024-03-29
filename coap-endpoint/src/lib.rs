mod auth;
mod command;
mod downstream;
mod error;
mod response;
mod session;
mod telemetry;

use crate::session::Session;
use crate::{auth::DeviceAuthenticator, error::CoapEndpointError, response::Responder};
use coap_lite::{CoapOption, CoapRequest, CoapResponse};
use drogue_cloud_endpoint_common::psk::{set_ssl_identity, Identity, VerifiedIdentity};
use drogue_cloud_endpoint_common::{
    auth::AuthConfig,
    command::{Commands, KafkaCommandSource, KafkaCommandSourceConfig},
    error::EndpointError,
    sender::{DownstreamSender, ExternalClientPoolConfig},
    sink::KafkaSink,
};
use drogue_cloud_service_api::auth::device::authn::PreSharedKeyOutcome;
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use drogue_cloud_service_common::{
    app::{Startup, StartupExt},
    defaults,
};
use tokio_dtls_stream_sink::Server as DtlsServer;

use drogue_cloud_endpoint_common::x509::ClientCertificateChain;
use openssl::ssl::{SslContext, SslFiletype, SslMethod, SslVerifyMode};
use serde::Deserialize;
use std::{collections::LinkedList, net::SocketAddr};
use telemetry::PublishOptions;

use tokio::net::UdpSocket;
use tokio::time::{Duration, Instant};

// RFC0007 - Drogue IoT extension attributes to CoAP Option Numbers
//
// Option Number 4209 corresponds to the option assigned to carry authorization information
// in the request, which contains HTTP-like authorization information
pub const AUTH_OPTION: CoapOption = CoapOption::Unknown(4209);
//
// Option Number 4210 corresponds to the option assigned to carry command information,
// which is meant for commands to be sent back to the device in the response
pub const HEADER_COMMAND: CoapOption = CoapOption::Unknown(4210);

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub bind_addr_coap: Option<String>,

    pub command_source_kafka: KafkaCommandSourceConfig,

    pub kafka_downstream_config: KafkaClientConfig,
    pub kafka_command_config: KafkaClientConfig,

    pub instance: String,

    pub auth: AuthConfig,

    #[serde(default = "defaults::check_kafka_topic_ready")]
    pub check_kafka_topic_ready: bool,

    #[serde(default)]
    pub endpoint_pool: ExternalClientPoolConfig,

    #[serde(default)]
    pub disable_dtls: bool,

    #[serde(default)]
    pub disable_client_certificates: bool,

    #[serde(default)]
    pub disable_psk: bool,

    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub dtls_session_timeout: Option<Duration>,

    #[serde(default)]
    pub cert_bundle_file: Option<String>,

    #[serde(default)]
    pub key_file: Option<String>,
}

#[derive(Clone, Debug)]
pub struct App {
    pub downstream: DownstreamSender,
    pub authenticator: DeviceAuthenticator,
    pub commands: Commands,
    pub disable_psk: bool,
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
    let commands = Commands::new();
    let addr = config
        .bind_addr_coap
        .unwrap_or_else(|| "[::]:5683".to_string());
    let coap_server_commands = commands.clone();

    let sender = DownstreamSender::new(
        KafkaSink::from_config(
            config.kafka_downstream_config,
            config.check_kafka_topic_ready,
        )?,
        config.instance,
        config.endpoint_pool,
    )?;

    let app = App {
        downstream: sender,
        authenticator: DeviceAuthenticator(
            drogue_cloud_endpoint_common::auth::DeviceAuthenticator::new(config.auth).await?,
        ),
        commands: coap_server_commands,
        disable_psk: config.disable_psk,
    };

    let command_source = KafkaCommandSource::new(
        commands,
        config.kafka_command_config,
        config.command_source_kafka,
    )?;

    let server = UdpSocket::bind(&addr).await?;

    let dtls = if !config.disable_dtls {
        let mut ctx = SslContext::builder(SslMethod::dtls())?;
        if let Some(key_file) = &config.key_file {
            ctx.set_private_key_file(key_file, SslFiletype::PEM)?;
        }
        if let Some(cert_bundle_file) = &config.cert_bundle_file {
            ctx.set_certificate_chain_file(cert_bundle_file)?;
        }
        if !config.disable_client_certificates {
            ctx.set_verify_callback(SslVerifyMode::PEER, |_, ctx| {
                log::debug!(
                    "Accepting client certificates: {:?}",
                    ctx.current_cert()
                        .map(|cert| format!("{:?}", cert.subject_name()))
                        .unwrap_or_else(|| "<unknown>".into())
                );
                true
            });
        }

        if !config.disable_psk {
            let auth = app.authenticator.clone();
            ctx.set_psk_server_callback(move |ssl, identity, secret_mut| {
                let mut to_copy = 0;
                if let Some(Ok(identity)) = identity.map(|s| core::str::from_utf8(s)) {
                    log::trace!("PSK auth for {:?}", identity);
                    if let Ok(identity) = Identity::parse(identity) {
                        let app = identity.application().to_string();
                        let device = identity.device().to_string();
                        let auth = auth.clone();
                        // Block this thread waiting for a response.
                        let response = tokio::task::block_in_place(move || {
                            // Run a temporary executor for this request
                            futures::executor::block_on(async move {
                                auth.request_psk(app, device).await
                            })
                        });

                        if let Ok(response) = response {
                            if let PreSharedKeyOutcome::Found { app, device, key } =
                                response.outcome
                            {
                                to_copy = std::cmp::min(key.key.len(), secret_mut.len());
                                secret_mut[..to_copy].copy_from_slice(&key.key[..to_copy]);
                                set_ssl_identity(
                                    ssl,
                                    VerifiedIdentity {
                                        application: app,
                                        device,
                                    },
                                );
                            }
                        }
                    }
                }
                Ok(to_copy)
            });
        }
        ctx.check_private_key()?;
        Some(ctx.build())
    } else {
        None
    };

    const DEFAULT_EXPIRY: Duration = Duration::from_secs(60);
    let expiry = config.dtls_session_timeout.unwrap_or(DEFAULT_EXPIRY);
    log::info!("CoAP server up on {}", addr);
    startup.spawn(async move {
        let mut server = DtlsServer::new(server);
        loop {
            match server.accept(dtls.as_ref()).await {
                Ok(session) => {
                    let expired = Instant::now() + expiry;
                    let app = app.clone();
                    tokio::spawn(async move {
                        Session::new(expired, session, app).run().await;
                    });
                }
                Err(e) => {
                    log::warn!("Error when accepting session: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });
    startup.check(command_source);

    Ok(())
}

pub(crate) async fn publish_handler(
    mut request: CoapRequest<SocketAddr>,
    certs: Option<ClientCertificateChain>,
    identity: Option<VerifiedIdentity>,
    app: App,
) -> Option<CoapResponse> {
    log::debug!("CoAP request: {:?}", request);

    let mut path_segments: Vec<String> = Vec::new();
    let mut queries: Option<&Vec<u8>> = None;
    let mut auth: Option<&Vec<u8>> = None;

    // Ensuring that we don't pass identities if PSK is not enabled
    let verified_identity = if app.disable_psk { None } else { identity };

    // Obtain vec[channel,subject] via 'p', optional query string via 'q'
    // and authorization information via 'a'
    if let Ok((p, q, a)) = params(&request) {
        path_segments = p;
        queries = q;
        auth = a;
    } else if let Err(e) = params(&request) {
        let ret = Err(CoapEndpointError(EndpointError::InvalidRequest {
            details: e.to_string(),
        }))
        .respond_to(&mut request);
        return ret;
    }

    log::debug!("Request - path: {path_segments:?}, queries: {queries:?}");

    // Deserialize optional queries into PublishOptions
    let options = queries
        .and_then(|x| serde_urlencoded::from_bytes::<PublishOptions>(x).ok())
        .unwrap_or_default();

    match path_segments.len() {
        // If only channel is present
        1 => telemetry::publish_plain(
            app.downstream,
            app.authenticator,
            app.commands,
            path_segments[0].clone(),
            options,
            request.clone(),
            auth,
            certs,
            verified_identity,
        )
        .await
        .respond_to(&mut request),

        // If both channel and subject are present
        2 => telemetry::publish_tail(
            app.downstream,
            app.authenticator,
            app.commands,
            (path_segments[0].clone(), path_segments[1].clone()),
            queries
                .map(|x| serde_urlencoded::from_bytes::<PublishOptions>(x))?
                .map_err(anyhow::Error::from)
                .ok()?,
            request.clone(),
            auth,
            certs,
            verified_identity,
        )
        .await
        .respond_to(&mut request),

        // If number of path arguments don't meet requirements
        _ => Err(CoapEndpointError(EndpointError::InvalidRequest {
            details: "Invalid number of path arguments".to_string(),
        }))
        .respond_to(&mut request),
    }
}

type Params<'a> = Result<(Vec<String>, Option<&'a Vec<u8>>, Option<&'a Vec<u8>>), anyhow::Error>;

fn params(request: &CoapRequest<SocketAddr>) -> Params {
    // Get path values and extract channel and subject
    let path_segments = request
        .message
        .get_option(CoapOption::UriPath)
        .and_then(|paths| path_parser(paths).ok())
        .ok_or_else(|| anyhow::Error::msg("Error parsing path"))?;

    // Get optional query values
    let queries = request
        .message
        .get_option(CoapOption::UriQuery)
        .and_then(|x| (x.front()));

    // Get authentication information
    let auth = request
        .message
        .get_option(AUTH_OPTION)
        .and_then(|x| x.front());

    Ok((path_segments, queries, auth))
}

fn path_parser(ll: &LinkedList<Vec<u8>>) -> Result<Vec<String>, EndpointError> {
    // UriPath can be deserialized as a linked list
    let mut linked_list = ll.iter();
    // Construct vector with channel and optional subject
    let mut option_values = Vec::new();

    // Check if first path argument is v1
    linked_list
        .next()
        .and_then(|x| String::from_utf8(x.clone()).ok())
        .filter(|x| x.eq("v1"))
        .ok_or_else(|| EndpointError::InvalidRequest {
            details: "incorrect version number".to_string(),
        })?;

    // Get channel value
    let channel = linked_list
        .next()
        .and_then(|x| String::from_utf8(x.clone()).ok())
        .ok_or_else(|| EndpointError::InvalidRequest {
            details: "error parsing channel".to_string(),
        })?;

    option_values.push(channel);

    // Get optional subject
    let mut subject = String::new();
    for i in linked_list {
        subject.push_str(std::str::from_utf8(i).map_err(|err| {
            return EndpointError::InvalidRequest {
                details: format!("error parsing subject: {:?}", err),
            };
        })?);
        subject.push('/');
    }

    // pop trailing '/' in subject and push subject into vector
    if !subject.is_empty() {
        subject.pop();
        option_values.push(subject);
    }

    Ok(option_values)
}

#[cfg(test)]
pub mod test {
    use coap_lite::{CoapOption, CoapRequest};
    use regex::Regex;
    use std::io::{Error, ErrorKind, Result};
    use std::net::SocketAddr;
    use url::Url;

    use super::params;
    use super::path_parser;

    struct CoapRequestBuilder {
        req: CoapRequest<SocketAddr>,
    }

    impl CoapRequestBuilder {
        fn new(url: &str) -> Self {
            let (_, _, path, queries) = CoapRequestBuilder::parse_coap_url(url).unwrap();

            let auth = "some auth val".as_bytes().to_vec();

            let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
            request.set_path(path.as_str());
            request.message.add_option(CoapOption::Unknown(4209), auth);

            if let Some(q) = queries {
                request.message.add_option(CoapOption::UriQuery, q);
            }

            CoapRequestBuilder { req: request }
        }

        fn parse_coap_url(url: &str) -> Result<(String, u16, String, Option<Vec<u8>>)> {
            let url_params = match Url::parse(url) {
                Ok(url_params) => url_params,
                Err(_) => return Err(Error::new(ErrorKind::InvalidInput, "url error")),
            };

            let host = match url_params.host_str() {
                Some("") => return Err(Error::new(ErrorKind::InvalidInput, "host error")),
                Some(h) => h,
                None => return Err(Error::new(ErrorKind::InvalidInput, "host error")),
            };
            let host = Regex::new(r"^\[(.*?)]$")
                .unwrap()
                .replace(&host, "$1")
                .to_string();

            let port = url_params.port().unwrap_or(5683);
            let path = url_params.path().to_string();

            let queries = url_params.query().map(|q| q.as_bytes().to_vec());

            Ok((host.to_string(), port, path, queries))
        }
    }

    #[test]
    fn path_parser_ok() {
        // /{channel}
        let mut request = CoapRequestBuilder::new("coap://test-url/v1/Rust?ct=30").req;
        let mut path_segments = request
            .message
            .get_option(CoapOption::UriPath)
            .map(|paths| path_parser(paths).ok())
            .flatten()
            .unwrap();
        assert_eq!(vec!["Rust".to_string()], path_segments);

        // channel name with special character
        request = CoapRequestBuilder::new("coap://test-url/v1/Röst?ct=30").req;
        assert!(request
            .message
            .get_option(CoapOption::UriPath)
            .map(|paths| path_parser(paths))
            .unwrap()
            .is_ok());

        // /{channel}/{subject}
        request = CoapRequestBuilder::new("coap://test-url/v1/Rust/test?ct=30").req;
        path_segments = request
            .message
            .get_option(CoapOption::UriPath)
            .map(|paths| path_parser(paths).ok())
            .flatten()
            .unwrap();
        assert_eq!(vec!["Rust".to_string(), "test".to_string()], path_segments);

        // /{channel}/{urlencoded_subject}
        request = CoapRequestBuilder::new("coap://test-url/v1/Rust/test/test2?ct=30").req;
        path_segments = request
            .message
            .get_option(CoapOption::UriPath)
            .map(|paths| path_parser(paths).ok())
            .flatten()
            .unwrap();
        assert_eq!(
            vec!["Rust".to_string(), "test/test2".to_string()],
            path_segments
        );
    }

    #[test]
    fn path_parser_err() {
        // endpoint version check(should be v1)
        let request = CoapRequestBuilder::new("coap://test-url/v2/Rust?ct=30").req;
        assert!(request
            .message
            .get_option(CoapOption::UriPath)
            .map(|paths| path_parser(paths))
            .unwrap()
            .is_err());
    }

    #[test]
    fn params_ok() {
        // /{channel}
        assert_eq!(
            params(&CoapRequestBuilder::new("coap://test-url/v1/Rust").req).unwrap(),
            (
                vec![(String::from("Rust"))],
                None,
                Some(&"some auth val".as_bytes().to_vec())
            )
        );

        // /{channel}/{subject}
        assert_eq!(
            params(&CoapRequestBuilder::new("coap://test-url/v1/Rust/test-1").req).unwrap(),
            (
                vec![(String::from("Rust")), (String::from("test-1"))],
                None,
                Some(&"some auth val".as_bytes().to_vec())
            )
        );

        // /{channel}?param
        assert_eq!(
            params(&CoapRequestBuilder::new("coap://test-url/v1/Rust?ct=30").req).unwrap(),
            (
                vec![(String::from("Rust"))],
                Some(&"ct=30".as_bytes().to_vec()),
                Some(&"some auth val".as_bytes().to_vec())
            )
        );

        // /{channel}/{subject}?param1&param2
        assert_eq!(
            params(
                &CoapRequestBuilder::new("coap://test-url/v1/Rust/test?ct=30&as=device%232").req
            )
            .unwrap(),
            (
                vec![(String::from("Rust")), (String::from("test"))],
                Some(&"ct=30&as=device%232".as_bytes().to_vec()),
                Some(&"some auth val".as_bytes().to_vec())
            )
        );
    }

    #[test]
    fn params_err() {
        // /{channel}?param
        assert_ne!(
            params(&CoapRequestBuilder::new("coap://test-url/v1/Rust?ct=30").req).unwrap(),
            (
                vec![(String::from("Rust"))],
                None,
                Some(&"some auth val".as_bytes().to_vec())
            )
        );
    }

    use super::telemetry::{PublishCommonOptions, PublishOptions};
    #[test]
    fn publish_options_test() {
        // application=app1, device=device#2, data_schema=application/octet-stream, ct=30, as=device#2
        let mut req = CoapRequestBuilder::new("coap://test-url/v1/Rust/test?application=app1&device=device%232&data_schema=application%2Foctet-stream&as=device%232&ct=30").req;
        let (_, queries, _) = params(&req).unwrap();
        assert_eq!(
            queries
                .map(|x| serde_urlencoded::from_bytes::<PublishOptions>(x).ok())
                .flatten()
                .unwrap_or_default(),
            PublishOptions {
                common: PublishCommonOptions {
                    application: Some("app1".to_string()),
                    device: Some("device#2".to_string()),
                    data_schema: Some("application/octet-stream".to_string()),
                },
                r#as: Some("device#2".to_string()),
                ct: Some(30),
            }
        );

        // application=None, device=None, data_schema=None, ct=30, as=device#2
        req = CoapRequestBuilder::new("coap://test-url/v1/Rust/test?as=device%232&ct=30").req;
        let (_, queries, _) = params(&req).unwrap();
        assert_eq!(
            queries
                .map(|x| serde_urlencoded::from_bytes::<PublishOptions>(x).ok())
                .flatten()
                .unwrap_or_default(),
            PublishOptions {
                common: PublishCommonOptions {
                    application: None,
                    device: None,
                    data_schema: None,
                },
                r#as: Some("device#2".to_string()),
                ct: Some(30),
            }
        );

        // application=None, device=None, data_schema=None, as=None, ct=30
        req = CoapRequestBuilder::new("coap://test-url/v1/Rust/test?ct=30").req;
        let (_, queries, _) = params(&req).unwrap();
        assert_eq!(
            queries
                .map(|x| serde_urlencoded::from_bytes::<PublishOptions>(x).ok())
                .flatten()
                .unwrap_or_default(),
            PublishOptions {
                common: PublishCommonOptions {
                    application: None,
                    device: None,
                    data_schema: None,
                },
                r#as: None,
                ct: Some(30),
            }
        );

        // application=None, device=None, data_schema=None, as=None, ct=None
        req = CoapRequestBuilder::new("coap://test-url/v1/Rust/test").req;
        let (_, queries, _) = params(&req).unwrap();
        assert_eq!(
            queries
                .map(|x| serde_urlencoded::from_bytes::<PublishOptions>(x).ok())
                .flatten()
                .unwrap_or_default(),
            PublishOptions {
                common: PublishCommonOptions {
                    application: None,
                    device: None,
                    data_schema: None,
                },
                r#as: None,
                ct: None,
            }
        );
    }
}
