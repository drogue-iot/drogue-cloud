#![type_length_limit = "6000000"]

mod auth;
mod mqtt;
mod server;

use crate::server::{build, build_tls};
use drogue_cloud_endpoint_common::downstream::DownstreamSender;
use envconfig::Envconfig;

#[derive(Clone, Debug, Envconfig)]
struct Config {
    #[envconfig(from = "DISABLE_TLS", default = "false")]
    pub disable_tls: bool,
    #[envconfig(from = "BIND_ADDR")]
    pub bind_addr: Option<String>,
}

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = Config::init_from_env()?;

    // test to see if we can create one, although we don't use it now, we would fail early
    let downstream = DownstreamSender::new()?;

    let builder = ntex::server::Server::build();
    let addr = config.bind_addr.as_deref();

    let builder = if !config.disable_tls {
        build_tls(addr, builder, downstream)?
    } else {
        build(addr, builder, downstream)?
    };

    Ok(builder.workers(1).run().await?)
}
