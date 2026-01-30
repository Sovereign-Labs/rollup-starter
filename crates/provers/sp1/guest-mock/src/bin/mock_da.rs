#![no_main]
//! This binary implements the verification logic for the rollup. This is the code that runs inside
//! of the zkvm in order to generate proofs for the rollup.

use sov_mock_da::{MockDaSpec, MockDaVerifier};
pub use sov_mock_zkvm::{MockZkGuest, MockZkvm};
use sov_modules_api::configurable_spec::ConfigurableSpec;
use sov_address::{EthereumAddress, EvmCryptoSpec};
use sov_modules_api::execution_mode::Zk;
use sov_modules_stf_blueprint::StfBlueprint;
use sov_rollup_interface::zk::CryptoSpec as CryptoSpecTrait;
use sov_sp1_adapter::guest::SP1Guest;
use sov_sp1_adapter::SP1;
use sov_state::nomt::zk_storage::NomtVerifierStorage;
use sov_state::DefaultStorageSpec;
use stf_starter::runtime::Runtime;
use stf_starter::StfVerifier;

sp1_zkvm::entrypoint!(main);

type ZkStorage = NomtVerifierStorage<DefaultStorageSpec<<EvmCryptoSpec as CryptoSpecTrait>::Hasher>>;
type RollupSpec = ConfigurableSpec<
    MockDaSpec,
    SP1,
    MockZkvm,
    EthereumAddress,
    Zk,
    EvmCryptoSpec,
    ZkStorage,
>;

#[cfg_attr(feature = "bench", sov_cycle_utils::macros::cycle_tracker)]
pub fn main() {
    let guest = SP1Guest::new();
    let storage = ZkStorage::new();

    let stf: StfBlueprint<RollupSpec, Runtime<_>> = StfBlueprint::new();

    let stf_verifier = StfVerifier::<_, _, _, _, _>::new(stf, MockDaVerifier {});

    stf_verifier
        .run_block(guest, storage)
        .expect("Prover must be honest");
}
