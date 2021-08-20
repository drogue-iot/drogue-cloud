mod auth;
mod command;
mod downstream;
mod error;
mod response;
mod telemetry;

use crate::{auth::DeviceAuthenticator, error::CoapEndpointError, response::Responder};
use coap::Server;
use coap_lite::{CoapOption, CoapRequest, CoapResponse};
use dotenv::dotenv;
use drogue_cloud_endpoint_common::command::{
    Commands, KafkaCommandSource, KafkaCommandSourceConfig,
};
use drogue_cloud_endpoint_common::{
    error::EndpointError,
    sender::DownstreamSender,
    sink::{KafkaSink, Sink},
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    health::{HealthServer, HealthServerConfig},
};
use futures::{self, TryFutureExt};
use serde::Deserialize;
use std::{collections::LinkedList, net::SocketAddr};
use telemetry::PublishOptions;

// RFC0007 - Drogue IoT extension attributes to CoAP Option Numbers
//
// Option Number 4209 corresponds to the option assigned to carry authorization information
// in the request, which contains HTTP-like authorization information
const AUTH_OPTION: CoapOption = CoapOption::Unknown(4209);
//
// Option Number 4210 correspons to the option assigned to carry command information,
// which is meant for commands to be sent back to the device in the response
const HEADER_COMMAND: CoapOption = CoapOption::Unknown(4210);

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub bind_addr_coap: Option<String>,

    pub command_source_kafka: KafkaCommandSourceConfig,
    #[serde(default)]
    pub health: HealthServerConfig,
}

#[derive(Clone, Debug)]
pub struct App<S>
where
    S: Sink + Send,
    <S as Sink>::Error: Send,
{
    pub downstream: DownstreamSender<S>,
    pub authenticator: DeviceAuthenticator,
    pub commands: Commands,
}

fn path_parser(ll: &LinkedList<Vec<u8>>) -> Result<Vec<String>, EndpointError> {
    // UriPath can be deserialized as a linked list
    let mut linked_list = ll.iter();
    // Construct vector with channel and optional subject
    let mut option_values = Vec::new();

    // Check if first path argument is v1
    linked_list
        .next()
        .map(|x| String::from_utf8(x.clone()).ok())
        .flatten()
        .filter(|x| x.eq("v1"))
        .ok_or_else(|| EndpointError::InvalidRequest {
            details: "incorrect version number".to_string(),
        })?;

    // Get channel value
    let channel = linked_list
        .next()
        .map(|x| String::from_utf8(x.clone()).ok())
        .flatten()
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

fn params(
    request: &CoapRequest<SocketAddr>,
) -> Result<(Vec<String>, Option<&Vec<u8>>, &Vec<u8>), anyhow::Error> {
    // Get path values and extract channel and subject
    let path_segments = request
        .message
        .get_option(CoapOption::UriPath)
        .map(|paths| path_parser(paths).ok())
        .flatten()
        .ok_or_else(|| anyhow::Error::msg("Error parsing path"))?;

    // Get optional query values
    let queries = request
        .message
        .get_option(CoapOption::UriQuery)
        .map(|x| (x.front()))
        .flatten();

    // Get authentication information
    let auth = request
        .message
        .get_option(AUTH_OPTION)
        .map(|x| x.front())
        .flatten()
        .ok_or_else(|| anyhow::Error::msg("Error parsing authentication information"))?;

    Ok((path_segments, queries, auth))
}

async fn publish_handler<S>(
    mut request: CoapRequest<SocketAddr>,
    app: App<S>,
) -> Option<CoapResponse>
where
    S: Sink + Send,
    <S as Sink>::Error: Send,
{
    let mut path_segments: Vec<String> = Vec::new();
    let mut queries: Option<&Vec<u8>> = None;
    let mut auth: &Vec<u8> = &Vec::new();

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

    // Deserialize optional queries into PublishOptions
    let options = queries
        .map(|x| serde_urlencoded::from_bytes::<PublishOptions>(x).ok())
        .flatten()
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

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;
    let commands = Commands::new();
    let addr = config
        .bind_addr_coap
        .unwrap_or_else(|| "0.0.0.0:5683".to_string());
    let coap_server_commands = commands.clone();

    let app = App {
        downstream: DownstreamSender::new(KafkaSink::new("DOWNSTREAM_KAFKA_SINK")?)?,
        authenticator: DeviceAuthenticator(
            drogue_cloud_endpoint_common::auth::DeviceAuthenticator::new().await?,
        ),
        commands: coap_server_commands,
    };

    println!("Server up on {}", addr);
    let mut server = Server::new(addr).unwrap();

    let device_to_endpoint = server.run(move |request| publish_handler(request, app.clone()));
    let command_source = KafkaCommandSource::new(commands, config.command_source_kafka)?;
    let health = HealthServer::new(config.health, vec![Box::new(command_source)]);

    futures::try_join!(health.run(), device_to_endpoint.err_into())?;
    Ok(())
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
                &"some auth val".as_bytes().to_vec()
            )
        );

        // /{channel}/{subject}
        assert_eq!(
            params(&CoapRequestBuilder::new("coap://test-url/v1/Rust/test-1").req).unwrap(),
            (
                vec![(String::from("Rust")), (String::from("test-1"))],
                None,
                &"some auth val".as_bytes().to_vec()
            )
        );

        // /{channel}?param
        assert_eq!(
            params(&CoapRequestBuilder::new("coap://test-url/v1/Rust?ct=30").req).unwrap(),
            (
                vec![(String::from("Rust"))],
                Some(&"ct=30".as_bytes().to_vec()),
                &"some auth val".as_bytes().to_vec()
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
                &"some auth val".as_bytes().to_vec()
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
                &"some auth val".as_bytes().to_vec()
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
