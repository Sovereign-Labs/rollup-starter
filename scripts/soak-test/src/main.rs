use std::time::Duration;

use clap::Parser;
use rollup_starter::rollup::StarterRollup;
use sov_modules_rollup_blueprint::RollupBlueprint;
use sov_rollup_interface::execution_mode::Native;
use sov_soak_testing_lib::{SoakTestRunner, ValidityProfile};
use tokio::signal::unix::SignalKind;
use tokio::sync::watch::Receiver;
use tokio::task::JoinSet;

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

type Runtime = <StarterRollup<Native> as RollupBlueprint<Native>>::Runtime;
type Spec = <StarterRollup<Native> as RollupBlueprint<Native>>::Spec;

async fn worker_task(
    client: sov_api_spec::Client,
    rx: Receiver<bool>,
    worker_id: u128,
    num_workers: u32,
) -> anyhow::Result<()> {
    let runner = SoakTestRunner::<Runtime, Spec>::new().with_bank();
    let result = runner
        .run(
            client,
            rx,
            worker_id,
            num_workers,
            ValidityProfile::Clean.get_validity(),
        )
        .await;

    if let Err(e) = result {
        tracing::error!("Worker task {worker_id} failed: {}", e);
        std::process::exit(1);
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    let _guard = sov_modules_rollup_blueprint::logging::initialize_logging();
    let mut worker_set = JoinSet::new();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let reqwest_client = reqwest::ClientBuilder::new()
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(60))
        .read_timeout(Duration::from_secs(120))
        .build()?;
    let client = sov_api_spec::Client::new_with_client(&args.api_url, reqwest_client);

    for i in 0..args.num_workers {
        worker_set.spawn(worker_task(
            client.clone(),
            rx.clone(),
            (i + args.salt) as u128,
            args.num_workers,
        ));
    }

    let mut terminate = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("Failed to set up SIGTERM handler");
    let mut quit =
        tokio::signal::unix::signal(SignalKind::quit()).expect("Failed to set up SIGQUIT handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("Received Ctrl+C"),
        _ = terminate.recv() => tracing::info!("Received SIGTERM"),
        _ = quit.recv() => tracing::info!("Received SIGQUIT"),
    }

    tx.send(true)?;
    _ = worker_set.join_all();

    Ok(())
}
