#![type_length_limit = "6000000"]

mod error;
mod mqtt;
mod server;

use crate::server::{build, build_tls};
use bytes::Bytes;
use bytestring::ByteString;
use drogue_cloud_endpoint_common::auth::DeviceProperties;
use drogue_cloud_endpoint_common::{
    auth::{AuthConfig, DeviceAuthenticator, Outcome as AuthOutcome},
    downstream::DownstreamSender,
    error::EndpointError,
};
use envconfig::Envconfig;
use serde_json::json;
use std::convert::TryInto;

#[derive(Clone, Debug, Envconfig)]
struct Config {
    #[envconfig(from = "DISABLE_TLS", default = "false")]
    pub disable_tls: bool,
    #[envconfig(from = "ENABLE_AUTH", default = "true")]
    pub enable_auth: bool,
    #[envconfig(from = "BIND_ADDR")]
    pub bind_addr: Option<String>,
}

#[derive(Clone, Debug)]
pub struct App {
    pub downstream: DownstreamSender,
    pub authenticator: Option<DeviceAuthenticator>,
}

impl App {
    async fn authenticate(
        &self,
        username: &Option<ByteString>,
        password: &Option<Bytes>,
        _: &ByteString,
    ) -> Result<AuthOutcome, EndpointError> {
        match (&self.authenticator, username, password) {
            (None, ..) => Ok(AuthOutcome::Pass(DeviceProperties(json!({})))),
            (Some(authenticator), Some(username), Some(password)) => {
                authenticator
                    .authenticate(
                        &username,
                        &String::from_utf8(password.to_vec())
                            .map_err(|_| EndpointError::AuthenticationError)?,
                    )
                    .await
            }
            (Some(_), _, _) => Ok(AuthOutcome::Fail),
        }
    }
}

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = Config::init_from_env()?;

    // test to see if we can create one, although we don't use it now, we would fail early
    let app = App {
        downstream: DownstreamSender::new()?,
        authenticator: match config.enable_auth {
            true => Some(AuthConfig::init_from_env()?.try_into()?),
            false => None,
        },
    };

    let builder = ntex::server::Server::build();
    let addr = config.bind_addr.as_deref();

    let builder = if !config.disable_tls {
        build_tls(addr, builder, app)?
    } else {
        build(addr, builder, app)?
    };

    Ok(builder.workers(1).run().await?)
}
