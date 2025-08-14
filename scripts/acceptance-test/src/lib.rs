use rand::distributions::Alphanumeric;
use rand::Rng;
use std::path::PathBuf;
use std::{env, fs, process::Command, thread, time::Duration};
use tracing::{debug, info};
pub mod fetch_and_compare;

pub const POSTGRES_CONTAINER_NAME: &str = "postgres-acceptance-test";

pub fn start_and_wait_for_postgres_ready(
    container_name: &str,
    password: &str,
) -> Result<(), anyhow::Error> {
    info!("Starting postgres container");
    let postgres_env = format!("POSTGRES_PASSWORD={}", password);
    let start_postgres = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            "postgres-acceptance-test",
            "-e",
            &postgres_env,
            "-p",
            "5432:5432",
            "postgres",
        ])
        .output()?;
    assert!(
        start_postgres.status.success(),
        "Failed to start postgres container"
    );

    info!("Waiting for postgres to be ready");
    let max_attempts = 30; // 30 seconds max

    for attempt in 0..max_attempts {
        let ready_check = Command::new("docker")
            .args(["exec", container_name, "pg_isready", "-U", "postgres"])
            .output()?;

        if ready_check.status.success() {
            info!("Postgres is ready");
            return Ok(());
        }

        debug!(
            "Postgres not ready yet, waiting... (attempt {}/{})",
            attempt, max_attempts
        );
        thread::sleep(Duration::from_secs(1));
    }
    Err(anyhow::anyhow!(
        "Postgres failed to become ready after {} seconds",
        max_attempts
    ))
}

pub fn cleanup_postgres_container(container_name: &str) -> Result<(), anyhow::Error> {
    // Cleanup postgres before returning
    info!("Cleaning up postgres container");
    let end_postgres = Command::new("docker")
        .args(["stop", container_name])
        .output()?;
    anyhow::ensure!(
        end_postgres.status.success(),
        "Failed to stop postgres container"
    );
    let remove_postgres = Command::new("docker")
        .args(["rm", "-f", container_name])
        .output()?;
    anyhow::ensure!(
        remove_postgres.status.success(),
        "Failed to remove postgres container"
    );
    Ok(())
}

pub fn generate_postgres_password() -> Result<String, anyhow::Error> {
    let password = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    Ok(password)
}

pub struct Directories {
    pub rollup_root: PathBuf,
    pub acceptance_test_dir: PathBuf,
    pub output_dir: PathBuf,
    pub rollup_data_path: PathBuf,
}

impl Directories {
    pub fn new() -> Result<Self, anyhow::Error> {
        let acceptance_test_dir = env::var("CARGO_MANIFEST_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));

        let rollup_root = acceptance_test_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        let output_dir = acceptance_test_dir.join("acceptance-test-data");
        fs::create_dir_all(&output_dir)?;
        let rollup_data_path = output_dir.join("rollup-starter-data");
        fs::create_dir_all(&rollup_data_path)?;

        Ok(Self {
            rollup_root,
            acceptance_test_dir,
            output_dir,
            rollup_data_path,
        })
    }
}

pub fn interpolate_config(password: &str, directories: &Directories) -> Result<(), anyhow::Error> {
    // Read and interpolate config file
    let config_path = directories.acceptance_test_dir.join("rollup_config.toml");
    info!("Reading config from: {}", config_path.display());
    let config_content = fs::read_to_string(config_path)?;

    // Make sqlite path absolute
    let sqlite_path = directories.output_dir.join("mock_da.sqlite");
    let sqlite_connection_string = format!("sqlite://{}?mode=rwc", sqlite_path.display());

    let interpolated_config = config_content
        .replace("{password}", &password)
        .replace("{sqlite_connection_string}", &sqlite_connection_string)
        .replace(
            "{rollup_data_path}",
            &directories.rollup_data_path.display().to_string(),
        );

    // Write interpolated config to new file
    let output_path = directories.output_dir.join("config.toml");
    info!("Writing interpolated config to: {}", output_path.display());
    fs::write(output_path, interpolated_config)?;
    Ok(())
}
