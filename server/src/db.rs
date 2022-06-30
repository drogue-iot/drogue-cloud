use crate::config::Database;
embed_migrations!("../database-common/migrations");

pub fn run_migrations(db: &Database) {
    use diesel::Connection;
    println!("Migrating database schema...");
    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        db.user, db.password, db.endpoint.host, db.endpoint.port, db.db
    );
    let connection = diesel::PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

    embedded_migrations::run_with_output(&connection, &mut std::io::stdout()).unwrap();
    println!("Migrating database schema... done!");
}
