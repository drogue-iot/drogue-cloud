use crate::config::Database;
use diesel_migrations::{EmbeddedMigrations, HarnessWithOutput, MigrationHarness};
use tokio::runtime::Handle;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("../database-common/migrations");

pub async fn run_migrations(db: &Database) -> anyhow::Result<()> {
    use diesel::Connection;
    println!("Migrating database schema...");
    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        db.user, db.password, db.endpoint.host, db.endpoint.port, db.db
    );

    Handle::current()
        .spawn_blocking(move || {
            let mut connection = diesel::PgConnection::establish(&database_url)
                .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

            HarnessWithOutput::new(&mut connection, std::io::stdout())
                .run_pending_migrations(MIGRATIONS)
                .unwrap();
            println!("Migrating database schema... done!");
        })
        .await?;

    Ok(())
}
