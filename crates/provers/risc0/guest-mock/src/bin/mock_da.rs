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
use sov_risc0_adapter::guest::Risc0Guest;
use sov_risc0_adapter::Risc0;
use sov_state::nomt::zk_storage::NomtVerifierStorage;
use sov_state::DefaultStorageSpec;
use stf_starter::runtime::Runtime;
use stf_starter::StfVerifier;

#[cfg(feature = "bench")]
fn report_bench_metrics(start_cycles: usize, end_cycles: usize) {
    let cycles_per_block = (end_cycles - start_cycles) as u64;
    let tuple = ("Cycles per block".to_string(), cycles_per_block);
    let mut serialized = Vec::new();
    serialized.extend(tuple.0.as_bytes());
    serialized.push(0);
    let size_bytes = tuple.1.to_ne_bytes();
    serialized.extend(&size_bytes);

    // calculate the syscall name.
    let cycle_string = String::from("cycle_metrics\0");
    let metrics_syscall_name =
        risc0_zkvm_platform::syscall::SyscallName::from_bytes_with_nul(cycle_string.as_ptr());

    risc0_zkvm::guest::env::send_recv_slice::<u8, u8>(metrics_syscall_name, &serialized);
}

risc0_zkvm::guest::entry!(main);

type ZkStorage = NomtVerifierStorage<DefaultStorageSpec<<EvmCryptoSpec as CryptoSpecTrait>::Hasher>>;
type RollupSpec = ConfigurableSpec<
    MockDaSpec,
    Risc0,
    MockZkvm,
    EthereumAddress,
    Zk,
    EvmCryptoSpec,
    ZkStorage
>;

pub fn main() {
    let guest = Risc0Guest::new();
    let storage = ZkStorage::new();
    #[cfg(feature = "bench")]
    let start_cycles = risc0_zkvm_platform::syscall::sys_cycle_count();

    let stf: StfBlueprint<RollupSpec, Runtime<_>> = StfBlueprint::new();

    let stf_verifier = StfVerifier::<_, _, _, _, _>::new(stf, MockDaVerifier {});

    stf_verifier
        .run_block(guest, storage)
        .expect("Prover must be honest");

    #[cfg(feature = "bench")]
    {
        let end_cycles = risc0_zkvm_platform::syscall::sys_cycle_count();
        report_bench_metrics(start_cycles, end_cycles);
    }
}
