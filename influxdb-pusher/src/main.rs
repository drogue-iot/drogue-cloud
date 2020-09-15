use actix_web::{middleware, post, web, App, HttpRequest, HttpResponse, HttpServer};
use chrono::{DateTime, Utc};
use cloudevents_sdk_actix_web::RequestExt;
use influxdb::Client;
use influxdb::Timestamp;
use log;

use cloudevents::event::Data;
use influxdb::InfluxDbWriteable;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, PartialEq, InfluxDbWriteable, Deserialize)]
struct TemperatureReading {
    time: DateTime<Utc>,
    temperature: f64,
}

#[post("/")]
async fn forward(
    req: HttpRequest,
    payload: web::Payload,
    client: web::Data<Client>,
) -> Result<HttpResponse, actix_web::Error> {
    let request_event = req.into_event(payload).await?;

    log::info!("Received Event: {:?}", request_event);

    let data: Option<Data> = request_event.get_data();

    let temp = match data {
        Some(Data::Json(value)) => value["temp"].as_f64(),
        Some(Data::String(s)) => serde_json::from_str::<Value>(&s)
            .ok()
            .and_then(|value| value["temp"].as_f64()),
        Some(Data::Binary(b)) => serde_json::from_slice::<Value>(&b)
            .ok()
            .and_then(|value| value["temp"].as_f64()),

        _ => {
            log::info!("Invalid data format: {:?}", data);
            None
        }
    };

    log::info!("Temp: {:?}", temp);

    match temp {
        Some(temperature) => {
            let value = TemperatureReading {
                time: Timestamp::Now.into(),
                temperature,
            };

            let query = value.into_query("temperatures");

            let result = client.query(&query).await;

            log::info!("Result: {:?}", result);

            match result {
                Ok(_) => Ok(HttpResponse::Accepted().finish()),
                Err(e) => Ok(HttpResponse::InternalServerError().body(e.to_string())),
            }
        }
        None => Ok(HttpResponse::NoContent().finish()),
    }
}

const GLOBAL_MAX_JSON_PAYLOAD_SIZE: usize = 64 * 1024;

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let influxdb_uri = std::env::var("INFLUXDB_URI")?;
    let influxdb_db = std::env::var("INFLUXDB_DATABASE")?;
    let influxdb_user = std::env::var("INFLUXDB_USERNAME")?;
    let influxdb_password = std::env::var("INFLUXDB_PASSWORD")?;

    let client = Client::new(influxdb_uri, influxdb_db).with_auth(influxdb_user, influxdb_password);

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(GLOBAL_MAX_JSON_PAYLOAD_SIZE))
            .data(client.clone())
            .service(forward)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await?;

    Ok(())
}
