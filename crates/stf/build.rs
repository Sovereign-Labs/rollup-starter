use sov_address::{EthereumAddress, EvmCryptoSpec};
use sov_mock_zkvm::MockZkvm;
use sov_modules_api::configurable_spec::ConfigurableSpec;
use stf_starter_declaration::Runtime;

#[cfg(all(feature = "mock_da", feature = "celestia_da"))]
compile_error!(
    "The `mock_da` and `celestia_da` features are mutually exclusive. Please choose one."
);

#[cfg(all(feature = "mock_da", feature = "mock_da_external"))]
compile_error!(
    "The `mock_da` and `mock_da_external` features are mutually exclusive. Please choose one."
);

#[cfg(all(feature = "mock_da_external", feature = "celestia_da"))]
compile_error!(
    "The `mock_da_external` and `celestia_da` features are mutually exclusive. Please choose one."
);

#[cfg(not(any(
    feature = "mock_da",
    feature = "celestia_da",
    feature = "mock_da_external"
)))]
compile_error!(
    "Either the `mock_da` or `celestia_da` or `mock_da_external` feature must be enabled."
);

#[cfg(all(
    feature = "mock_da",
    not(any(feature = "mock_da_external", feature = "celestia_da"))
))]
use sov_mock_da::MockDaSpec as DaSpec;

#[cfg(all(
    feature = "mock_da_external",
    not(any(feature = "mock_da", feature = "celestia_da"))
))]
use sov_mock_da::MockDaSpec as DaSpec;

#[cfg(all(
    feature = "celestia_da",
    not(any(feature = "mock_da", feature = "mock_da_external"))
))]
pub use sov_celestia_adapter::verifier::CelestiaSpec as DaSpec;

#[cfg(feature = "native")]
type ExecMode = sov_modules_api::execution_mode::Native;

#[cfg(not(feature = "native"))]
type ExecMode = sov_modules_api::execution_mode::Zk;

type S = ConfigurableSpec<DaSpec, MockZkvm, MockZkvm, EthereumAddress, ExecMode, EvmCryptoSpec>;

fn main() -> anyhow::Result<()> {
    sov_build::Options::apply_defaults::<S, Runtime<S>>()
}
