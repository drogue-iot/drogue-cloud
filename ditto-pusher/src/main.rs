mod error;

use crate::error::PusherError;
use actix::{io::SinkWrite, Actor, ActorContext, Addr, AsyncContext, Context, Handler};
use actix_codec::Framed;
use actix_web::web::Bytes;
use actix_web::{middleware, post, web, App, HttpRequest, HttpResponse, HttpServer};
use awc::{
    error::WsProtocolError,
    ws::{Codec, Message},
    BoxedSocket, Client,
};
use cloudevents::event::Data;
use cloudevents_sdk_actix_web::HttpRequestExt;
use futures::stream::SplitSink;
use futures::StreamExt;
use http::StatusCode;
use log;
use serde_json::{json, Value};
use std::time::Duration;

const GLOBAL_MAX_JSON_PAYLOAD_SIZE: usize = 64 * 1024;

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

    Ok(match ditto.send(DittoUpdate(data.to_string())).await {
        Ok(_) => {
            log::info!("Payload accepted");
            ditto.send(DittoClose).await.ok();
            HttpResponse::Accepted().body("Payload accepted")
        }
        Err(err) => {
            ditto.send(DittoClose).await.ok();
            log::info!("Failed to handle data: {}", err);
            HttpResponse::NotAcceptable()
                .json(json!({"error": "Failed to publish", "reason": err.to_string()}))
        }
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

        let (response, framed) = client.ws(&self.url).connect().await.map_err(|err| {
            log::info!("Failed to connect: {}", err);
            anyhow::Error::msg(err.to_string())
        })?;

        log::info!("WS connect response: {:?}", response);

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
    }
}

impl DittoClient {
    fn hb(&self, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::new(1, 0), |act, ctx| {
            act.sink.write(Message::Ping(Bytes::from_static(b"")));
            act.hb(ctx);

            // client should also check for a timeout here, similar to the
            // server code
        });
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
struct DittoUpdate(String);

impl Handler<DittoUpdate> for DittoClient {
    type Result = ();

    fn handle(&mut self, msg: DittoUpdate, _ctx: &mut Context<Self>) {
        self.sink.write(Message::Text(msg.0));
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
struct DittoClose;

impl Handler<DittoClose> for DittoClient {
    type Result = ();

    fn handle(&mut self, _msg: DittoClose, ctx: &mut Context<Self>) {
        ctx.stop();
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
