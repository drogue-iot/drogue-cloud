use deadpool::managed::{PoolConfig, Timeouts};
use std::process::Command;
use std::{fs, path::PathBuf, time::Duration};
use testcontainers::{
    clients::Cli,
    images::generic::{GenericImage, WaitFor},
    Container, Docker, RunArgs,
};

fn is_containerized() -> bool {
    std::env::var_os("container").is_some()
}

pub struct PostgresRunner<'c, C: Docker, SC> {
    pub config: SC,
    db: Container<'c, C, GenericImage>,
}

const MSG: &str = "[1] LOG:  database system is ready to accept connections";

impl<'c, C: 'c + Docker, SC> PostgresRunner<'c, C, SC> {
    pub fn new(cli: &'c C, config: SC) -> anyhow::Result<Self> {
        log::info!("Starting postgres (containerized: {})", is_containerized());

        let image = GenericImage::new("docker.io/library/postgres:12")
            .with_env_var("POSTGRES_PASSWORD", "mysecretpassword")
            .with_volume(
                Self::gather_sql()?
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Failed to generate SQL path"))?,
                "/docker-entrypoint-initdb.d",
            );

        let image = match is_podman() {
            false => image.with_wait_for(WaitFor::message_on_stderr(MSG)),
            // "podman logs" merges all logs into stdout
            // see: https://github.com/containers/podman/issues/9159
            true => image.with_wait_for(WaitFor::message_on_stdout(MSG)),
        };

        let args = RunArgs::default().with_mapped_port((5432, 5432));
        let args = if is_containerized() {
            args.with_network("drogue").with_name("postgres")
        } else {
            args
        };

        let db = cli.run_with_args(image, args);

        Ok(Self { config, db })
    }

    fn gather_sql() -> anyhow::Result<PathBuf> {
        let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| anyhow::anyhow!("Missing environment variable 'CARGO_MANIFEST_DIR'"))?;
        let manifest_dir = PathBuf::from(manifest_dir);

        let target = manifest_dir.join("target/sql");

        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::create_dir_all(&target)?;

        Self::copy_sql(&manifest_dir.join("../database-common/migrations"), &target)?;
        Self::copy_sql(&manifest_dir.join("tests/sql"), &target)?;

        // done
        Ok(target)
    }

    fn copy_sql(source: &PathBuf, target: &PathBuf) -> anyhow::Result<()> {
        if !source.exists() {
            return Ok(());
        }

        for up in walkdir::WalkDir::new(&source)
            .contents_first(true)
            .into_iter()
            .filter_entry(|entry| entry.file_name() == "up.sql")
        {
            let up = up?;
            let name = up
                .path()
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Missing parent component"))?;
            let name = name
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!(""))?;
            let target = target.join(format!("{}-up.sql", name));
            log::debug!("Add SQL file: {:?} -> {:?}", up, target);

            fs::copy(up.path(), target)?;
        }

        Ok(())
    }
}

impl<'c, C: Docker, SC> Drop for PostgresRunner<'c, C, SC> {
    fn drop(&mut self) {
        log::info!("Stopping postgres");
        self.db.stop();
    }
}

pub fn client() -> Cli {
    match is_podman() {
        true => Cli::podman(),
        false => Cli::docker(),
    }
}

fn is_podman() -> bool {
    let out = Command::new("podman").args(&["version"]).output();
    matches!(out, Ok(_))
}

pub fn db<C, SC, F>(cli: &C, f: F) -> anyhow::Result<PostgresRunner<C, SC>>
where
    C: Docker,
    F: FnOnce(deadpool_postgres::Config) -> SC,
{
    let host = match is_containerized() {
        true => "postgres",
        false => "localhost",
    };

    let config = f(deadpool_postgres::Config {
        host: Some(host.into()),
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
