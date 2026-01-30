// TODO: Rename this file to change the name of this method from METHOD_NAME

#![no_main]

use sov_celestia_adapter::types::Namespace;
use sov_celestia_adapter::verifier::CelestiaSpec;
use sov_celestia_adapter::verifier::CelestiaVerifier;
use sov_celestia_adapter::verifier::RollupParams;
use sov_rollup_interface::da::DaVerifier;
use sov_rollup_interface::zk::CryptoSpec as CryptoSpecTrait;
use sov_mock_zkvm::MockZkvm;
use sov_modules_api::configurable_spec::ConfigurableSpec;
use sov_address::{EthereumAddress, EvmCryptoSpec};
use sov_modules_api::execution_mode::Zk;
use sov_modules_stf_blueprint::StfBlueprint;
use sov_sp1_adapter::guest::SP1Guest;
use sov_sp1_adapter::SP1;
use sov_state::nomt::zk_storage::NomtVerifierStorage;
use sov_state::DefaultStorageSpec;
use stf_starter::runtime::Runtime;
use stf_starter::StfVerifier;

/// The namespace for the rollup on Celestia. Must be kept in sync with the "rollup/src/lib.rs"
const ROLLUP_BATCH_NAMESPACE: Namespace = Namespace::const_v0(*b"sov-test-b");
const ROLLUP_PROOF_NAMESPACE: Namespace = Namespace::const_v0(*b"sov-test-p");

type ZkStorage = NomtVerifierStorage<DefaultStorageSpec<<EvmCryptoSpec as CryptoSpecTrait>::Hasher>>;
type RollupSpec = ConfigurableSpec<
    CelestiaSpec,
    SP1,
    MockZkvm,
    EthereumAddress,
    Zk,
    EvmCryptoSpec,
    ZkStorage,
>;

sp1_zkvm::entrypoint!(main);

pub fn main() {
    let guest = SP1Guest::new();
    let storage = ZkStorage::new();
    let stf: StfBlueprint<RollupSpec, Runtime<_>> = StfBlueprint::new();

    let stf_verifier = StfVerifier::new(
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
