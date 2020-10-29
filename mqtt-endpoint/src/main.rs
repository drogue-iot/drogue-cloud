#![type_length_limit = "6000000"]

use crate::server::{build, build_tls};
use drogue_cloud_endpoint_common::downstream::DownstreamSender;

mod mqtt;
mod server;

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // test to see if we can create one, although we don't use it now, we would fail early
    DownstreamSender::new()?;

    let builder = ntex::server::Server::build();

    let tls = !std::env::var_os("DISABLE_TLS")
        .map(|s| s == "true")
        .unwrap_or(false);

    let addr = std::env::var("BIND_ADDR").ok();
    let addr = addr.as_deref();

    let builder = if tls {
        build_tls(addr, builder)?
    } else {
        build(addr, builder)?
    };

    Ok(builder.workers(1).run().await?)
}
