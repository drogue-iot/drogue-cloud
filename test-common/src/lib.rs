use deadpool::managed::{PoolConfig, Timeouts};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use testcontainers::{
    clients::Cli, core::WaitFor, images::generic::GenericImage, Container, Docker, RunArgs,
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
    pub fn new(cli: &'c C, config: SC, pc: deadpool_postgres::Config) -> anyhow::Result<Self> {
        log::info!("Starting postgres (containerized: {})", is_containerized());

        let image = GenericImage::new("docker.io/library/postgres:12")
            .with_env_var("POSTGRES_PASSWORD", "mysecretpassword");

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

        Self::init_db(pc)?;

        Ok(Self { config, db })
    }

    /// Init database by executing all found SQL statement
    fn init_db(config: deadpool_postgres::Config) -> anyhow::Result<()> {
        // FIXME: this logic currently doesn't sort files, which might result in a problem
        Self::find_all_sql(
            |_| Ok(()),
            |s, _| {
                let mut cmd = Command::new("psql");
                cmd.arg("-h")
                    .arg(&config.host.as_ref().unwrap_or(&"localhost".into()))
                    .arg("-p")
                    .arg(config.port.as_ref().unwrap_or(&5432).to_string())
                    .arg("-U")
                    .arg(&config.user.as_ref().unwrap_or(&"postgres".into()))
                    .env(
                        "PGPASSWORD",
                        &config.password.as_ref().unwrap_or(&"postgres".into()),
                    )
                    .arg("-d")
                    .arg(&config.dbname.as_ref().unwrap_or(&"postgres".into()))
                    .arg("-f")
                    .arg(s.as_os_str());
                log::info!("Running: {:?}", cmd);
                let out = cmd.output()?;
                log::info!("Out: {:?}", String::from_utf8(out.stdout));
                log::info!("Err: {:?}", String::from_utf8(out.stderr));
                if out.status.success() {
                    Ok(())
                } else {
                    anyhow::bail!("Command failed: {}", out.status)
                }
            },
        )?;
        Ok(())
    }

    /// Used to gather all required SQL scripts in a directory
    #[allow(dead_code)]
    fn gather_sql() -> anyhow::Result<PathBuf> {
        Self::find_all_sql(
            |t| {
                if t.exists() {
                    std::fs::remove_dir_all(&t)?;
                }
                std::fs::create_dir_all(&t)?;
                Ok(())
            },
            |s, t| fs::copy(s, t).map_err(|err| err.into()).map(|_| ()),
        )
    }

    fn find_all_sql<I, F>(i: I, f: F) -> anyhow::Result<PathBuf>
    where
        I: FnOnce(&Path) -> anyhow::Result<()>,
        F: Fn(&Path, &Path) -> anyhow::Result<()>,
    {
        let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| anyhow::anyhow!("Missing environment variable 'CARGO_MANIFEST_DIR'"))?;
        let manifest_dir = PathBuf::from(manifest_dir);

        let target = manifest_dir.join("target/sql");
        i(&target)?;

        Self::find_sql(
            &manifest_dir.join("../database-common/migrations"),
            &target,
            &f,
        )?;
        Self::find_sql(&manifest_dir.join("tests/sql"), &target, &f)?;

        // done

        Ok(target)
    }

    fn find_sql<F>(source: &PathBuf, target: &PathBuf, f: &F) -> anyhow::Result<()>
    where
        F: Fn(&Path, &Path) -> anyhow::Result<()>,
    {
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

            f(up.path(), &target)?;
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

    let pc = deadpool_postgres::Config {
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
    };
    let config = f(pc.clone());

    Ok(PostgresRunner::new(cli, config, pc)?)
}
