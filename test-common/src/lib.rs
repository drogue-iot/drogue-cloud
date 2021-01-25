use deadpool::managed::{PoolConfig, Timeouts};
use std::{env, time::Duration};
use testcontainers::{
    images::generic::{GenericImage, WaitFor},
    Container, Docker,
};

pub struct PostgresRunner<'c, C: Docker, SC> {
    pub config: SC,
    db: Container<'c, C, GenericImage>,
}

impl<'c, C: Docker, SC> PostgresRunner<'c, C, SC> {
    pub fn new(cli: &'c C, config: SC) -> anyhow::Result<Self> {
        log::info!("Starting postgres");

        let db = cli.run(
            GenericImage::new("docker.io/library/postgres:12")
                .with_mapped_port((5432, 5432))
                .with_env_var("POSTGRES_PASSWORD", "mysecretpassword")
                .with_volume(
                    env::current_dir()?
                        .join("sql")
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("Failed to generate SQL path"))?,
                    "/docker-entrypoint-initdb.d",
                )
                .with_wait_for(WaitFor::message_on_stdout(
                    "[1] LOG:  database system is ready to accept connections", // listening on pid 1
                )),
        );

        // sleep(time::Duration::from_secs(1));

        Ok(Self { config, db })
    }
}

impl<'c, C: Docker, SC> Drop for PostgresRunner<'c, C, SC> {
    fn drop(&mut self) {
        log::info!("Stopping postgres");
        self.db.stop();
    }
}

pub fn db<C, SC, F>(cli: &C, f: F) -> anyhow::Result<PostgresRunner<C, SC>>
where
    C: Docker,
    F: FnOnce(deadpool_postgres::Config) -> SC,
{
    let config = f(deadpool_postgres::Config {
        host: Some("localhost".into()),
        user: Some("postgres".into()),
        password: Some("mysecretpassword".into()),
        dbname: Some("postgres".into()),

        pool: Some(PoolConfig {
            max_size: 15,
            timeouts: Timeouts {
                wait: Some(Duration::from_secs(5)),
                ..Default::default()
            },
        }),

        ..Default::default()
    });

    Ok(PostgresRunner::new(cli, config)?)
}
