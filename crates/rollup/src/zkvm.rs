#[cfg(feature = "mock_zkvm")]
mod mock_zkvm {
    pub use sov_mock_zkvm::MockZkvm as Zkvm;
    pub use sov_mock_zkvm::MockZkvmHost as ZkvmHost;
    use sov_rollup_interface::zk::CryptoSpec;
    use std::sync::Arc;

    pub fn rollup_host_args() -> Arc<()> {
        Arc::new(())
    }

    pub fn create_inner_vm_from_config(
        _prover_config: sov_stf_runner::processes::RollupProverConfig<Zkvm>,
    ) -> ZkvmHost {
        // Mock zkvm doesn't need the ELF from prover config
        ZkvmHost::new()
    }

    pub type Hasher = <sov_mock_zkvm::MockZkvmCryptoSpec as CryptoSpec>::Hasher;
}

#[cfg(feature = "mock_zkvm")]
pub use mock_zkvm::{
    create_inner_vm_from_config, rollup_host_args, Hasher, Zkvm as InnerZkvm,
    ZkvmHost as InnerZkvmHost,
};

pub use sov_mock_zkvm::MockZkvm as OuterZkvm;
pub use sov_mock_zkvm::MockZkvmHost as OuterZkvmHost;

pub fn get_outer_vm() -> OuterZkvmHost {
    OuterZkvmHost::new_non_blocking()
}
