use acceptance_test::{
    cleanup_postgres_container, generate_postgres_password, interpolate_config,
    start_and_wait_for_postgres_ready, Directories, POSTGRES_CONTAINER_NAME,
};
use clap::Parser;
use std::{process::Command, thread, time::Duration};
use tracing::info;

fn main() -> Result<(), anyhow::Error> {
    // Initialize tracing subscriber with RUST_LOG environment variable, fallback to info
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting acceptance test");

    // Run the test
    let result = run_test();
    cleanup_postgres_container(POSTGRES_CONTAINER_NAME)?;

    info!("Acceptance test completed");
    result
}

fn run_test() -> Result<(), anyhow::Error> {
    // Generate a config file with our db password and all paths set relative to the workspace root
    let password = generate_postgres_password()?;
    let directories = Directories::new()?;
    interpolate_config(&password, &directories)?;

    // Start the sequencer postgres and wait for it to be ready
    start_and_wait_for_postgres_ready(POSTGRES_CONTAINER_NAME, &password)?;

    // Start the rollup. Run for 10 seconds
    info!(
        "Starting rollup from rollup workspace root: {}",
        directories.rollup_root.display()
    );
    let rollup = Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "--rollup-config-path",
            &directories
                .output_dir
                .join("config.toml")
                .display()
                .to_string(),
        ])
        .current_dir(directories.rollup_root)
        .spawn()
        .expect("Failed to start rollup");
    info!("Rollup started, waiting 10 seconds");
    thread::sleep(Duration::from_secs(45));

    // Shutdown the rollup )
    info!("Sending SIGINT to rollup process");
    let mut interrupt = Command::new("kill")
        .args(["-s", "SIGINT", &rollup.id().to_string()])
        .spawn()?;
    interrupt.wait()?;
    let output = rollup.wait_with_output()?;
    info!("Rollup process finished");
    println!("{}", String::from_utf8(output.stdout)?);
    Ok(())
}

#[derive(Parser)]
struct Args {
    #[arg(short, long, default_value = "http://localhost:12346")]
    /// The URL of the rollup node to connect to. Defaults to http://localhost:12346.
    api_url: String,

    #[arg(short, long, default_value = "5")]
    /// The number of workers to spawn - this controls the number of concurrent transactions. Defaults to 5.
    num_workers: u32,

    #[arg(short, long, default_value = "0")]
    /// The salt to use for RNG. Use this value if you're restarting the generator and want to ensure that the generated
    /// transactions don't overlap with the previous run.
    salt: u32,
}
