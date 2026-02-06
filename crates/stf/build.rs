use sov_address::{EthereumAddress, EvmCryptoSpec};
use sov_mock_zkvm::MockZkvm;
use sov_modules_api::configurable_spec::ConfigurableSpec;
use stf_starter_declaration::Runtime;

pub use sov_celestia_adapter::verifier::CelestiaSpec as DaSpec;

#[cfg(feature = "native")]
type ExecMode = sov_modules_api::execution_mode::Native;

#[cfg(not(feature = "native"))]
type ExecMode = sov_modules_api::execution_mode::Zk;

type S = ConfigurableSpec<DaSpec, MockZkvm, MockZkvm, EthereumAddress, ExecMode, EvmCryptoSpec>;

fn main() -> anyhow::Result<()> {
    sov_build::Options::apply_defaults::<S, Runtime<S>>()
}
