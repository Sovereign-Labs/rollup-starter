//! Offline migration tool that upgrades the rollup state from version 0 to version 1.
//!
//! This rewrites legacy `sov-accounts` entries into the new (multisig) account
//! layout and bumps the on-disk `chain_state.state_version` from 0 to 1. Run it
//! once, while the node is stopped, at the hard-fork boundary:
//!
//! ```text
//! # rehearse without committing
//! cargo run --bin rollup-db-migration -- --rollup-config-path configs/celestia/rollup.toml --dry-run
//! # commit the migration
//! cargo run --bin rollup-db-migration -- --rollup-config-path configs/celestia/rollup.toml
//! ```
//!
//! After it succeeds, switch to the v1 rollup binary (compiled with `STATE_VERSION = 1`).

use std::path::PathBuf;

use clap::Parser;
use rollup_starter::rollup::StarterRollup;
use sov_modules_api::{CryptoSpec, Spec};
use sov_modules_rollup_blueprint::RollupBlueprint;
use sov_rollup_interface::execution_mode::Native;
use stf_starter::Runtime;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Offline migration tool for rollup state version 0 to version 1."
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
    if let Err(err) = run() {
        eprintln!("rollup-db-migration failed: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let args = Args::parse();
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
    Ok(())
}
