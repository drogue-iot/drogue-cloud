#[cfg(feature = "actix")]
pub mod call;

use anyhow::Context;
use deadpool::managed::{PoolConfig, Timeouts};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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

        let image = match needs_podman_fix() {
            false => image.with_wait_for(WaitFor::message_on_stderr(MSG)),
            // "podman logs" (v2) merges all logs into stdout
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
        let mut cmd = Command::new("psql");
        cmd.arg("-v")
            .arg("ON_ERROR_STOP=1")
            .arg("-h")
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
            .arg(&config.dbname.as_ref().unwrap_or(&"postgres".into()));

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        log::info!("Running: {:?}", cmd);

        let mut child = cmd.spawn()?;

        // 'psql' is running, now pipe in commands

        {
            let mut stdin = child.stdin.take().expect("Failed to open input stream");
            Self::find_all_sql(
                |_| Ok(()),
                move |s, _| {
                    stdin
                        .write_all(&fs::read(s).context("Failed to read SQL file")?)
                        .context("Failed to pipe SQL content")?;
                    Ok(())
                },
            )?;
        }

        // now wait to the command to end

        let out = child.wait_with_output()?;

        log::info!("Out: {:?}", String::from_utf8(out.stdout));
        log::info!("Err: {:?}", String::from_utf8(out.stderr));
        log::info!("Status: {:?}", out.status);
        if out.status.success() {
            Ok(())
        } else {
            anyhow::bail!("Command failed: {}", out.status)
        }
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

    fn find_all_sql<I, F>(i: I, mut f: F) -> anyhow::Result<PathBuf>
    where
        I: FnOnce(&Path) -> anyhow::Result<()>,
        F: FnMut(&Path, &Path) -> anyhow::Result<()>,
    {
        let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| anyhow::anyhow!("Missing environment variable 'CARGO_MANIFEST_DIR'"))?;
        let manifest_dir = PathBuf::from(manifest_dir);

        let target = manifest_dir.join("target/sql");
        i(&target)?;

        let mut files = Vec::new();

        // gather

        files.extend(Self::find_sql(
            &manifest_dir.join("../database-common/migrations"),
            &target,
        )?);
        files.extend(Self::find_sql(&manifest_dir.join("tests/sql"), &target)?);

        // sort

        files.sort_unstable();

        // execute

        for file in files {
            log::info!("Process file: {:?}", file.0);
            f(&file.0, &file.1)?;
        }

        // done

        Ok(target)
    }

    fn find_sql(source: &Path, target: &Path) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
        let mut result = Vec::new();

        if !source.exists() {
            return Ok(result);
        }

        for up in walkdir::WalkDir::new(&source) {
            let up = up?;
            if up.file_type().is_file() && up.file_name() == "up.sql" {
                let name = up
                    .path()
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("Missing parent component"))?;
                let name = name
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| anyhow::anyhow!(""))?;
                let target = target.join(format!("{}-up.sql", name));
                log::debug!("Found SQL file: {:?} -> {:?}", up, target);

                result.push((up.path().to_owned(), target.to_owned()));
            }
        }

        Ok(result)
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

/// Check if we are using podman
fn is_podman() -> bool {
    podman_version().is_some()
}

/// Check if we need a fix for podman 2
///
/// See: https://github.com/containers/podman/issues/9159
fn needs_podman_fix() -> bool {
    podman_version().map(|major| major < 3).unwrap_or(false)
}

/// Get the podman version, or `None` if podman was not found.
fn podman_version() -> Option<u16> {
    let out = Command::new("podman")
        .args(&["version", "-f", "json"])
        .output();

    match out {
        Ok(out) if out.status.success() => {
            if let Ok(version) = serde_json::from_slice::<Value>(out.stdout.as_slice()) {
                let v = version["Client"]["Version"].as_str();
                v.and_then(|v| {
                    let v: Vec<_> = v.split('.').collect();
                    v.first().and_then(|major| major.parse().ok())
                })
            } else {
                None
            }
        }
        _ => None,
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

    PostgresRunner::new(cli, config, pc)
}
