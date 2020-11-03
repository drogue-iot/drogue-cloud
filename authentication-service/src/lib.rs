pub mod models;
pub mod schema;

use actix_web::{HttpResponse};

use diesel::prelude::*;
use diesel::r2d2::{Pool, ConnectionManager, PooledConnection};
use diesel::pg::PgConnection;


use crate::models::Credential;
use crate::schema::credentials::dsl::*;


pub type PgPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;


pub fn establish_connection() -> PgPool {

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .build(manager)
        .expect("Failed to create pool.")
}

pub fn pg_pool_handler(pool: &PgPool) -> Result<PgPooledConnection, HttpResponse> {
    pool
    .get()
    .map_err(|e| {
        HttpResponse::InternalServerError().json(e.to_string())
    })
}

pub fn get_credentials(id: &str, pool: &PgConnection) -> Vec<Credential> {
    
    let results = credentials.filter(device_id.eq(id))
            .load::<Credential>(pool)
            .expect("Error loading credentials");

    results
}

pub fn read_private_key_file(path: String) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

