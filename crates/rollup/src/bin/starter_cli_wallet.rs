//! This binary defines a cli wallet for interacting
//! with the rollup.

use rollup_starter::rollup::StarterRollup;
use sov_modules_api::cli::{FileNameArg, JsonStringArg};
use sov_modules_rollup_blueprint::WalletBlueprint;
use stf_starter::runtime::RuntimeSubcommand;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    StarterRollup::run_wallet::<
        RuntimeSubcommand<FileNameArg, _>,
        RuntimeSubcommand<JsonStringArg, _>,
    >()
    .await
}
