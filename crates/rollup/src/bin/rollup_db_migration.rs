//! Offline migration tool that upgrades rollup-starter state from version 0 to version 1.

use std::path::PathBuf;

use clap::Parser;
use rollup_starter::rollup::StarterRollup;
use sov_modules_api::execution_mode::Native;
use sov_modules_api::{CryptoSpec, Spec};
use sov_modules_rollup_blueprint::logging::initialize_logging;
use sov_modules_rollup_blueprint::RollupBlueprint;
use stf_starter::Runtime;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Offline migration tool for rollup-starter state version 0 to version 1."
)]
struct Args {
    /// Path to the rollup config file used by the running node.
    #[arg(long)]
    rollup_config_path: PathBuf,

    /// Override `storage.path` from the rollup config.
    #[arg(long)]
    db_path: Option<PathBuf>,

    /// Compute the post-migration state root but do not commit changes.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

type RollupSpec = <StarterRollup<Native> as RollupBlueprint<Native>>::Spec;
type Hasher = <<RollupSpec as Spec>::CryptoSpec as CryptoSpec>::Hasher;

fn main() {
    let _guard = initialize_logging();
    if let Err(err) = run() {
        tracing::error!(error = format!("{err:#}"), "rollup-db-migration failed");
        eprintln!("rollup-db-migration failed: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    tracing::info!(
        rollup_config_path = %args.rollup_config_path.display(),
        db_path = ?args.db_path,
        dry_run = args.dry_run,
        "Starting rollup-db-migration (state version 0 -> 1)"
    );
    let start = std::time::Instant::now();
    let mut runtime = Runtime::<RollupSpec>::default();
    let runtime_inner = &mut *runtime;
    sov_migrations::v1::run::<RollupSpec, Hasher>(
        sov_migrations::MigrationArgs {
            rollup_config_path: args.rollup_config_path,
            db_path: args.db_path,
            dry_run: args.dry_run,
        },
        &mut runtime_inner.accounts,
        &mut runtime_inner.chain_state,
    )?;
    tracing::info!(
        elapsed = ?start.elapsed(),
        "rollup-db-migration completed successfully"
    );
    Ok(())
}
