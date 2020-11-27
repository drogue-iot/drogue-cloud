use actix_web::{middleware, post, web, App, HttpRequest, HttpResponse, HttpServer};
use chrono::{DateTime, Utc};
use cloudevents::event::Data;
use cloudevents_sdk_actix_web::HttpRequestExt;
use envconfig::Envconfig;
use influxdb::{Client, InfluxDbWriteable, Timestamp};
use log;
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
    let request_event = req.to_event(payload).await?;

    log::info!("Received Event: {:?}", request_event);

    let data: Option<&Data> = request_event.data();

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

#[derive(Envconfig, Clone, Debug)]
struct InfluxDb {
    #[envconfig(from = "INFLUXDB_URI")]
    pub uri: String,
    #[envconfig(from = "INFLUXDB_DATABASE")]
    pub db: String,
    #[envconfig(from = "INFLUXDB_USERNAME")]
    pub user: String,
    #[envconfig(from = "INFLUXDB_PASSWORD")]
    pub password: String,
}

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let influx = InfluxDb::init_from_env()?;
    let client = Client::new(influx.uri, influx.db).with_auth(influx.user, influx.password);

    let config = Config::init_from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .data(client.clone())
            .service(forward)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
