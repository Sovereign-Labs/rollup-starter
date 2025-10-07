//! This binary runs the rollup full node.

use anyhow::Context;
use clap::Parser;
use rollup_starter::da::DaService;
use rollup_starter::rollup::StarterRollup;
use rollup_starter::zkvm::{rollup_host_args, InnerZkvm};
use sov_modules_rollup_blueprint::logging::initialize_logging;
use sov_modules_rollup_blueprint::FullNodeBlueprint;
use sov_modules_rollup_blueprint::Rollup;
use sov_rollup_interface::execution_mode::Native;
use sov_stf_runner::processes::{RollupProverConfig, RollupProverConfigDiscriminants};
use sov_stf_runner::{from_toml_path, RollupConfig};
use std::path::PathBuf;
use std::str::FromStr;

use sov_address::EthereumAddress;
use sov_modules_api::capabilities::RollupHeight;

#[cfg(all(feature = "mock_da", feature = "celestia_da"))]
compile_error!("Both mock_da and celestia_da are enabled, but only one should be.");

#[cfg(all(not(feature = "mock_da"), not(feature = "celestia_da")))]
compile_error!("Neither mock_da and celestia_da are enabled, but only one should be.");

// Ensure exactly one zkvm feature is enabled
const _: () = {
    let risc0 = cfg!(feature = "risc0") as u8;
    let sp1 = cfg!(feature = "sp1") as u8;
    let mock_zkvm = cfg!(feature = "mock_zkvm") as u8;
    let count = risc0 + sp1 + mock_zkvm;

    assert!(
        count == 1,
        "Exactly one zkvm feature must be enabled: risc0, sp1, or mock_zkvm"
    );
};

#[cfg(all(feature = "mock_da", not(feature = "celestia_da")))]
const DA_STR: &str = "mock";
#[cfg(all(feature = "celestia_da", not(feature = "mock_da")))]
const DA_STR: &str = "celestia";

fn default_genesis_path() -> PathBuf {
    PathBuf::from_str(&format!("configs/{DA_STR}/genesis.json"))
        .expect("failed to construct default genesis path")
}

fn default_rollup_config_path() -> PathBuf {
    PathBuf::from_str(&format!("configs/{DA_STR}/rollup.toml"))
        .expect("failed to construct default genesis path")
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The path to the rollup config.
    #[arg(long, default_value = default_rollup_config_path().into_os_string())]
    rollup_config_path: PathBuf,

    /// The path to the genesis config.
    #[arg(long, default_value = default_genesis_path().into_os_string())]
    genesis_path: PathBuf,

    // UDP port on 127.0.0.1 where Telegraf service suppose to listen.
    #[arg(long, default_value_t = 9845)]
    metrics: u64,

    /// Start the rollup at a given height.
    #[arg(long, default_value = None)]
    start_at_rollup_height: Option<u64>,

    /// Stops the rollup at a given height.
    #[arg(long, default_value = None)]
    stop_at_rollup_height: Option<u64>,
}

#[tokio::main]
// Not returning a result here, so the error could be logged properly.
async fn main() {
    let args = Args::parse();

    let _guard = initialize_logging();

    let metrics_port = args.metrics;
    let address = format!("127.0.0.1:{metrics_port}");
    prometheus_exporter::start(address.parse().unwrap())
        .expect("Could not start prometheus server");

    let prover_config_disc = parse_prover_config().expect("Malformed prover_config");
    tracing::info!(
        ?prover_config_disc,
        "Running demo rollup with prover config"
    );

    let prover_config =
        prover_config_disc.map(|config_disc| config_disc.into_config(rollup_host_args()));
    let rollup = new_rollup(
        args.genesis_path,
        args.rollup_config_path,
        prover_config,
        args.start_at_rollup_height.map(RollupHeight::new),
        args.stop_at_rollup_height.map(RollupHeight::new),
    )
    .await
    .expect("Couldn't start rollup");
    rollup.run().await.expect("Couldn't run rollup");
}

fn parse_prover_config() -> anyhow::Result<Option<RollupProverConfigDiscriminants>> {
    if let Some(value) = option_env!("SOV_PROVER_MODE") {
        tracing::warn!("SOV_PROVER_MODE is set to {}, but proving is not currently supported. Ignoring prover config.", value);
        Ok(None)
        // TODO: Re-enable proving once https://github.com/Sovereign-Labs/sovereign-sdk-wip/issues/2814 is resolved
        //
        // let config = std::str::FromStr::from_str(value).inspect_err(|&error| {
        //     tracing::error!(value, ?error, "Unknown `SOV_PROVER_MODE` value; aborting");
        // })?;
        // #[cfg(debug_assertions)]
        // {
        //     if config == RollupProverConfigDiscriminants::Prove {
        //         tracing::warn!(prover_config = ?config, "Given RollupProverConfig might cause slow rollup progression if not compiled in release mode.");
        //     }
        // }
        // Ok(Some(config))
    } else {
        Ok(None)
    }
}

async fn new_rollup(
    genesis_path: PathBuf,
    rollup_config_path: PathBuf,
    prover_config: Option<RollupProverConfig<InnerZkvm>>,
    start_at_rollup_height: Option<RollupHeight>,
    stop_at_rollup_height: Option<RollupHeight>,
) -> Result<Rollup<StarterRollup<Native>, Native>, anyhow::Error> {
    tracing::info!(
        ?rollup_config_path,
        ?genesis_path,
        ?start_at_rollup_height,
        ?stop_at_rollup_height,
        "Starting rollup with config"
    );

    let rollup_config: RollupConfig<EthereumAddress, DaService> =
        from_toml_path(&rollup_config_path).with_context(|| {
            format!(
                "Failed to read rollup configuration from {}",
                rollup_config_path.to_str().unwrap()
            )
        })?;

    let rollup = StarterRollup::default();

    rollup
        .create_new_rollup(
            &genesis_path,
            rollup_config,
            prover_config,
            start_at_rollup_height,
            stop_at_rollup_height,
        )
        .await
}
