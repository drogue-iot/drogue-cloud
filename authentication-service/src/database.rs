use actix_web::{HttpResponse};

use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};

use serde_json::Value;

use crate::schema;
use crate::models::Credential;

pub(super) type PgPool = Pool<ConnectionManager<PgConnection>>;
pub(super) type PgPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

pub(super) fn establish_connection(database_url: String) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .build(manager)
        .expect("Failed to create pool.")
}

pub(super) fn pg_pool_handler(pool: &PgPool) -> Result<PgPooledConnection, HttpResponse> {
    pool.get()
        .map_err(|e| HttpResponse::InternalServerError().json(e.to_string()))
}

pub(super) fn get_credential(id: &str, pool: &PgConnection) -> Result<Credential, HttpResponse> {
    let results = schema::credentials::dsl::credentials
        .filter(schema::credentials::dsl::device_id.eq(id))
        .load::<Credential>(pool)
        .expect("Error loading credentials");

    control_credentials(results, id)
}

pub(super) fn serialise_props(props: Option<Value>) -> String {
    match props {
        Some(p) => p.as_str().unwrap_or("{}").to_string(),
        None => "{}".to_string()
    }
}

fn control_credentials(creds: Vec<Credential>, id: &str) -> Result<Credential, HttpResponse>{
    if creds.len() > 1 {
        log::info!("More than one credential exist for {}", id);
        return Err(HttpResponse::InternalServerError().finish())
    } else if creds.len() == 1 {
        Ok(creds[0].clone())
    } else if creds.len() == 0 {
        log::info!("No credentials found for {}", id);
        return Err(HttpResponse::NotFound().finish())
    } else {
        return Err(HttpResponse::InternalServerError().finish())
    }
}