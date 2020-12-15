use drogue_cloud_database_common::database;
use drogue_cloud_database_common::models;

use actix_cors::Cors;
use actix_web::{
    delete, get, http::header, post, put, web, web::Json, App, HttpResponse, HttpServer, Responder,
};
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use dotenv::dotenv;
use envconfig::Envconfig;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use futures::future;

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

fn password_credential(password: &str) -> Option<Value> {
    if password.is_empty() {
        return None;
    }

    let salt: String = thread_rng().sample_iter(&Alphanumeric).take(16).collect();
    let mut hasher = Sha256::new();
    hasher.input_str(password);
    hasher.input_str(&salt);

    Some(json!({
        "hash": hasher.result_str(),
        "salt": salt,
    }))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CreateDevice {
    pub device_id: String,
    pub password: String,
    #[serde(default)]
    pub properties: Option<Value>,
}

impl CreateDevice {
    fn to_credential(&self) -> models::Credential {
        models::Credential {
            device_id: self.device_id.clone(),
            properties: self.properties.clone(),
            secret: password_credential(&self.password),
        }
    }
}

#[post("")]
async fn create_device(
    data: web::Data<WebData>,
    create: Json<CreateDevice>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Creating device: '{:?}'", create);

    if create.device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let connection = database::pg_pool_handler(&data.connection_pool)?;

    let response = database::insert_credential(&create.to_credential(), &connection).map(|c| {
        HttpResponse::Created()
            .set_header(header::LOCATION, c.device_id)
            .finish()
    })?;

    Ok(response)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct UpdateDevice {
    pub password: String,
    #[serde(default)]
    pub properties: Option<Value>,
}

impl UpdateDevice {
    fn to_credential(&self, device_id: String) -> models::Credential {
        models::Credential {
            device_id,
            properties: self.properties.clone(),
            secret: password_credential(&self.password),
        }
    }
}

#[put("/{device_id}")]
async fn update_device(
    data: web::Data<WebData>,
    web::Path(device_id): web::Path<String>,
    update: Json<UpdateDevice>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Creating device: '{:?}'", update);

    if device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let connection = database::pg_pool_handler(&data.connection_pool)?;

    let response =
        database::update_credential(&update.to_credential(device_id), &connection).map(|c| {
            HttpResponse::NoContent()
                .set_header(header::LOCATION, c.device_id)
                .finish()
        })?;

    Ok(response)
}

#[delete("/{device_id}")]
async fn delete_device(
    data: web::Data<WebData>,
    web::Path(device_id): web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Deleting device: '{}'", device_id);

    if device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let connection = database::pg_pool_handler(&data.connection_pool)?;
    let response = database::delete_credential(device_id, &connection).map(|n| match n {
        0 => HttpResponse::NotFound().finish(),
        1 => HttpResponse::NoContent().finish(),
        n => HttpResponse::Ok().body(format!("{} devices deleted", n)),
    })?;

    Ok(response)
}

#[get("/{device_id}")]
async fn read_device(
    data: web::Data<WebData>,
    web::Path(device_id): web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Reading device: '{}'", device_id);

    if device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let connection = database::pg_pool_handler(&data.connection_pool)?;
    let response = match database::get_credential(device_id.as_str(), &connection)? {
        Some(credential) => HttpResponse::Ok().json(&credential),
        None => HttpResponse::NotFound().finish(),
    };

    Ok(response)
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
    #[envconfig(from = "HEALTH_BIND_ADDR", default = "127.0.0.1:9090")]
    pub health_bind_addr: String,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::init_from_env().unwrap();

    let pool = database::establish_connection(config.db_url);
    let data = WebData {
        connection_pool: pool.expect("Failed to create pool"),
    };

    let app_server = HttpServer::new(move || {
        App::new()
            .data(web::JsonConfig::default().limit(64 * 1024))
            .service(
                web::scope("/api/v1").wrap(Cors::permissive()).service(
                    web::scope("/devices")
                        .service(create_device)
                        .service(update_device)
                        .service(delete_device)
                        .service(read_device),
                ),
            )
            .data(data.clone())
    })
    .bind(config.bind_addr)?
    .run();

    let health_server = HttpServer::new(move || App::new().service(health))
        .bind(config.health_bind_addr)?
        .run();

    future::try_join(app_server, health_server).await?;
    Ok(())
}
