mod error;

use crate::error::PusherError;
use actix::{io::SinkWrite, Actor, Addr, AsyncContext, Context, Handler};
use actix_codec::Framed;
use actix_web::web::Bytes;
use actix_web::{middleware, post, web, App, HttpRequest, HttpResponse, HttpServer};
use awc::{
    error::WsProtocolError,
    ws::{Codec, Message},
    BoxedSocket, Client,
};
use chrono::{DateTime, Utc};
use cloudevents::event::Data;
use cloudevents_sdk_actix_web::HttpRequestExt;
use drogue_cloud_endpoint_common::error::EndpointError;
use futures::stream::SplitSink;
use futures::StreamExt;
use http::StatusCode;
use log;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

const GLOBAL_MAX_JSON_PAYLOAD_SIZE: usize = 64 * 1024;

#[derive(Debug, PartialEq, Deserialize)]
struct TemperatureReading {
    time: DateTime<Utc>,
    temperature: f64,
}

#[post("/")]
async fn forward(
    req: HttpRequest,
    payload: web::Payload,
    config: web::Data<DittoConfiguration>,
) -> Result<HttpResponse, actix_web::Error> {
    let request_event = req.to_event(payload).await?;

    log::info!("Received Event: {:?}", request_event);

    let data: Option<&Data> = request_event.data();

    let data = match data {
        Some(Data::Json(value)) => Ok(value.clone()),
        Some(Data::String(s)) => serde_json::from_str::<Value>(&s).map_err(|err| PusherError {
            code: StatusCode::NOT_ACCEPTABLE,
            error: "Failed to decode JSON".into(),
            message: err.to_string(),
        }),

        Some(Data::Binary(b)) => serde_json::from_slice::<Value>(&b).map_err(|err| PusherError {
            code: StatusCode::NOT_ACCEPTABLE,
            error: "Failed to decode JSON".into(),
            message: err.to_string(),
        }),

        _ => {
            log::info!("Invalid data format: {:?}", data);
            Err(PusherError {
                code: StatusCode::NOT_ACCEPTABLE,
                error: "Invalid data format".into(),
                message: format!("Invalid data format: {:?}", data),
            })
        }
    }?;

    log::info!("Data: {:?}", data);

    let ditto = config.create_client().await.map_err(|err| PusherError {
        code: StatusCode::BAD_GATEWAY,
        error: "Failed to connect to Ditto".into(),
        message: err.to_string(),
    })?;

    Ok(match ditto.send(DittoCommand(data.to_string())).await {
        Ok(_) => HttpResponse::Accepted().body("Payload accepted"),
        Err(err) => HttpResponse::NotAcceptable()
            .json(json!({"error": "Failed to publish", "reason": err.to_string()})),
    })
}

#[derive(Clone, Debug)]
pub struct DittoConfiguration {
    pub url: String,
    pub username: String,
    pub password: String,
}

impl DittoConfiguration {
    async fn create_client(&self) -> Result<Addr<DittoClient>, anyhow::Error> {
        let client = Client::builder()
            .basic_auth(&self.username, Some(&self.password))
            .finish();

        let (_, framed) = client
            .ws(&self.url)
            .connect()
            .await
            .map_err(|err| anyhow::Error::msg(err.to_string()))?;

        let (sink, _) = framed.split();

        Ok(DittoClient::create(|ctx| {
            // DittoClient::add_stream(stream, ctx);
            DittoClient {
                sink: SinkWrite::new(sink, ctx),
            }
        }))
    }
}

struct DittoClient {
    sink: SinkWrite<awc::ws::Message, SplitSink<Framed<BoxedSocket, Codec>, awc::ws::Message>>,
}

impl Actor for DittoClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        // start heartbeats otherwise server will disconnect after 10 seconds
        self.hb(ctx)
    }

    fn stopped(&mut self, _: &mut Context<Self>) {
        log::info!("Disconnected");

        // Stop application on disconnect
        // System::current().stop();
    }
}

impl DittoClient {
    fn hb(&self, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::new(1, 0), |act, ctx| {
            act.sink
                .write(Message::Ping(Bytes::from_static(b"")))
                .unwrap();
            act.hb(ctx);

            // client should also check for a timeout here, similar to the
            // server code
        });
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
struct DittoCommand(String);

impl Handler<DittoCommand> for DittoClient {
    type Result = ();

    fn handle(&mut self, msg: DittoCommand, _ctx: &mut Context<Self>) {
        self.sink.write(Message::Text(msg.0)).unwrap();
    }
}

impl actix::io::WriteHandler<WsProtocolError> for DittoClient {}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = DittoConfiguration {
        url: std::env::var("DITTO_URL")?,
        username: std::env::var("DITTO_USERNAME")?,
        password: std::env::var("DITTO_PASSWORD")?,
    };

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(GLOBAL_MAX_JSON_PAYLOAD_SIZE))
            .data(config.clone())
            .service(forward)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await?;

    Ok(())
}
