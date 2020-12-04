use drogue_cloud_database_common::database;
use drogue_cloud_database_common::models;

use actix_web::{delete, get, http::header, post, web, App, HttpResponse, HttpServer};
use futures::StreamExt;

use actix_web::web::Buf;
use dotenv::dotenv;
use envconfig::Envconfig;

#[post("/device/{device_id}")]
async fn create_device(
    data: web::Data<WebData>,
    web::Path(device_id): web::Path<String>,
    mut body: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Creating device: '{}'", device_id);

    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let bytes = bytes.freeze();

    let device_data: models::Credential = serde_json::from_slice(bytes.bytes())?;

    if device_data.device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let connection = database::pg_pool_handler(&data.connection_pool)?;

    match database::insert_credential(device_data, &connection) {
        Ok(c) => Ok(HttpResponse::Created()
            .set_header(header::LOCATION, c.device_id)
            .finish()),
        Err(e) => Ok(e),
    }
}

#[delete("/device/{device_id}")]
async fn delete_device(
    data: web::Data<WebData>,
    web::Path(device_id): web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Deleting device: '{}'", device_id);

    if device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let connection = database::pg_pool_handler(&data.connection_pool)?;
    match database::delete_credential(device_id, &connection) {
        Ok(n) => {
            if n == 0 {
                Ok(HttpResponse::NotFound().finish())
            } else if n == 1 {
                Ok(HttpResponse::NoContent().finish())
            } else {
                Ok(HttpResponse::Ok().body(format!("{} devices deleted", n)))
            }
        }
        Err(e) => Ok(e),
    }
}

#[get("/device/{device_id}")]
async fn read_device(
    data: web::Data<WebData>,
    web::Path(device_id): web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Reading device: '{}'", device_id);

    if device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let connection = database::pg_pool_handler(&data.connection_pool)?;
    match database::get_credential(device_id.as_str(), &connection) {
        Ok(res) => Ok(HttpResponse::Ok().body(serde_json::to_string(&res)?)),
        Err(e) => Ok(e),
    }
}

#[derive(Clone)]
struct WebData {
    connection_pool: database::PgPool,
}

#[derive(Envconfig)]
struct Config {
    #[envconfig(from = "DATABASE_URL")]
    pub db_url: String,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::init_from_env().unwrap();

    let pool = database::establish_connection(config.db_url);
    let data = WebData {
        connection_pool: pool,
    };

    HttpServer::new(move || {
        App::new()
            .service(create_device)
            .service(delete_device)
            .service(read_device)
            .data(data.clone())
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
