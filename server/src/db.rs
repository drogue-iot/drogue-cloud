use crate::config::Database;
use tokio::runtime::Handle;
embed_migrations!("../database-common/migrations");

pub async fn run_migrations(db: &Database) -> anyhow::Result<()> {
    use diesel::Connection;
    println!("Migrating database schema...");
    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        db.user, db.password, db.endpoint.host, db.endpoint.port, db.db
    );

    Handle::current()
        .spawn_blocking(move || {
            let connection = diesel::PgConnection::establish(&database_url)
                .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

            embedded_migrations::run_with_output(&connection, &mut std::io::stdout()).unwrap();
            println!("Migrating database schema... done!");
        })
        .await?;

    Ok(())
}
