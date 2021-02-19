use crate::command::wait_for_command;
use actix_web::{web, HttpResponse};
use async_trait::async_trait;
use drogue_cloud_endpoint_common::commands::Commands;
use drogue_cloud_endpoint_common::{
    downstream::{DownstreamSender, Publish},
    error::HttpEndpointError,
};
use drogue_cloud_service_common::Id;

#[async_trait]
pub trait HttpCommandSender {
    async fn publish_and_await<B>(
        &self,
        publish: Publish,
        commands: web::Data<Commands>,
        ttd: Option<u64>,
        //command: CommandWait,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send;
}

#[async_trait]
impl HttpCommandSender for DownstreamSender {
    async fn publish_and_await<B>(
        &self,
        publish: Publish,
        commands: web::Data<Commands>,
        ttd: Option<u64>,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send,
    {
        self.publish_http(publish.clone(), body, |_| async move {
            wait_for_command(commands, Id::new(&publish.app_id, &publish.device_id), ttd).await
        })
        .await
    }
}
