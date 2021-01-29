use deadpool::managed::{PoolConfig, Timeouts};
use std::io::{BufRead, Lines};
use std::process::Command;
use std::thread::sleep;
use std::{fs, path::PathBuf, time::Duration};
use testcontainers::{
    clients::Cli, images::generic::GenericImage, Container, Docker, Image, RunArgs,
};

fn is_containerized() -> bool {
    std::env::var_os("container").is_some()
}

pub struct PostgresRunner<'c, C: Docker, SC> {
    pub config: SC,
    db: Container<'c, C, GenericImage>,
}

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

        let args = RunArgs::default().with_mapped_port((5432, 5432));
        let args = if is_containerized() {
            args.with_network("drogue").with_name("postgres")
        } else {
            args
        };

        let db = cli.run_with_args(image, args);

        log::info!("Waiting for postgres to become ready...");
        Self::wait_startup(&db)?;
        log::info!("Waiting for postgres to become ready... done!");

        Ok(Self { config, db })
    }

    fn wait_startup<D, I>(db: &Container<D, I>) -> anyhow::Result<()>
    where
        D: Docker,
        I: Image,
    {
        // we cannot use "wait for" as we need to look for the same message twice
        // we also cannot use "log", as that drops messages every now and then

        /*
        let logs = db.logs();
        let out = logs.stdout;
        let reader = BufReader::new(out);
        let mut n = 0;
        for line in reader.lines() {
            let line = line?;
            log::debug!("{}", line);
            if line.contains("database system is ready to accept connections") {
                n += 1;
                log::debug!("Count: {}", n);
                if n > 1 {
                    return Ok(());
                }
            }
        }*/

        let mut n = 10;

        loop {
            let logs = Self::logs(db.id())?;

            if Self::is_ready(logs.lines())? {
                break;
            }

            sleep(Duration::from_secs(1));
            n -= 1;
            if n == 0 {
                anyhow::bail!("Stream aborted, not ready.")
            }
        }

        Ok(())
    }

    fn is_ready<B: BufRead>(lines: Lines<B>) -> anyhow::Result<bool> {
        let mut n = 0;
        for line in lines {
            let line = line?;
            log::debug!("{}", line);
            if line.contains("database system is ready to accept connections") {
                n += 1;
                log::debug!("Count: {}", n);
                if n > 1 {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn logs(id: &str) -> anyhow::Result<Vec<u8>> {
        let out = Command::new("docker").args(&["logs", id]).output()?;
        Ok(out.stdout)
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
    let out = Command::new("podman").args(&["version"]).output();
    match out {
        Ok(_) => Cli::podman(),
        _ => Cli::docker(),
    }
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
