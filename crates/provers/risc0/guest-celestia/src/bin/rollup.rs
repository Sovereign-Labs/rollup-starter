// TODO: Rename this file to change the name of this method from METHOD_NAME

#![no_main]

use sov_celestia_adapter::types::Namespace;
use sov_celestia_adapter::verifier::CelestiaSpec;
use sov_celestia_adapter::verifier::CelestiaVerifier;
use sov_celestia_adapter::verifier::RollupParams;
use sov_rollup_interface::da::DaVerifier;
use sov_mock_zkvm::MockZkvm;
use sov_modules_api::configurable_spec::ConfigurableSpec;
use sov_address::{EthereumAddress, EvmCryptoSpec};
use sov_modules_api::execution_mode::Zk;
use sov_modules_stf_blueprint::StfBlueprint;
use sov_risc0_adapter::guest::Risc0Guest;
use sov_risc0_adapter::Risc0;
use sov_state::ZkStorage;
use stf_starter::runtime::Runtime;
use stf_starter::StfVerifier;

/// The namespace for the rollup on Celestia. Must be kept in sync with the "rollup/src/lib.rs"
const ROLLUP_BATCH_NAMESPACE: Namespace = Namespace::const_v0(config_value!("BATCH_NAMESPACE"));
const ROLLUP_PROOF_NAMESPACE: Namespace = Namespace::const_v0(config_value!("PROOF_NAMESPACE"));

risc0_zkvm::guest::entry!(main);

pub fn main() {
    let guest = Risc0Guest::new();
    let storage = ZkStorage::new();
    let stf: StfBlueprint<ConfigurableSpec<CelestiaSpec, Risc0, MockZkvm, EthereumAddress, Zk, EvmCryptoSpec>, Runtime<_>> =
        StfBlueprint::new();

    let stf_verifier = StfVerifier::<_, _, _, _, _>::new(
        stf,
        CelestiaVerifier::new(RollupParams {
            rollup_batch_namespace: ROLLUP_BATCH_NAMESPACE,
            rollup_proof_namespace: ROLLUP_PROOF_NAMESPACE,
        }),
    );
    stf_verifier
        .run_block(guest, storage)
        .expect("Prover must be honest");
}
