use actix_web::HttpResponse;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};

use serde_json::Value;

use crate::error::ServiceError;
use crate::models::Credential;
use crate::schema;

pub type PgPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

pub fn establish_connection(database_url: String) -> Result<PgPool, r2d2::Error> {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder().build(manager)
}

pub fn pg_pool_handler(pool: &PgPool) -> Result<PgPooledConnection, HttpResponse> {
    pool.get()
        .map_err(|e| HttpResponse::InternalServerError().json(e.to_string()))
}

pub fn get_credential(id: &str, pool: &PgConnection) -> Result<Option<Credential>, ServiceError> {
    use schema::credentials::dsl::*;

    let results = credentials
        .filter(device_id.eq(id))
        .load::<Credential>(pool)?;

    control_credentials(results, id)
}

pub fn serialise_props(props: Option<Value>) -> String {
    match props {
        Some(p) => p.as_str().unwrap_or("{}").to_string(),
        None => "{}".to_string(),
    }
}

pub fn insert_credential(
    data: &Credential,
    pool: &PgConnection,
) -> Result<Credential, ServiceError> {
    use schema::credentials::dsl::*;

    Ok(diesel::insert_into(credentials)
        .values(data)
        .get_result(pool)?)
}

pub fn delete_credential(id: String, pool: &PgConnection) -> Result<usize, ServiceError> {
    use schema::credentials::dsl::*;

    Ok(diesel::delete(credentials.filter(device_id.eq(id))).execute(pool)?)
}

fn control_credentials(
    credentials: Vec<Credential>,
    id: &str,
) -> Result<Option<Credential>, ServiceError> {
    match credentials.as_slice() {
        [] => {
            log::info!("No credentials found for {}", id);
            Ok(None)
        }
        [cred] => Ok(Some(cred.clone())),
        [_, ..] => {
            log::info!("More than one credential exist for {}", id);
            Err(ServiceError::InvalidState)
        }
    }
}
