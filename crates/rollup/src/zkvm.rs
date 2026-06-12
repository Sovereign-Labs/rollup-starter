mod mock_zkvm {
    pub use sov_mock_zkvm::MockZkvm as Zkvm;
    pub use sov_mock_zkvm::MockZkvmHost as ZkvmHost;
    use sov_rollup_interface::zk::CryptoSpec;
    use std::sync::Arc;

    pub fn rollup_host_args() -> Arc<()> {
        Arc::new(())
    }

    pub async fn create_inner_vm() -> ZkvmHost {
        ZkvmHost::new()
    }

    pub type Hasher = <sov_mock_zkvm::MockZkvmCryptoSpec as CryptoSpec>::Hasher;
}

pub use mock_zkvm::{
    create_inner_vm, rollup_host_args, Hasher, Zkvm as InnerZkvm, ZkvmHost as InnerZkvmHost,
};

pub use sov_mock_zkvm::MockZkvm as OuterZkvm;
pub use sov_mock_zkvm::MockZkvmHost as OuterZkvmHost;
use sov_rollup_interface::zk::aggregated_proof::SerializedAggregatedProof;

pub fn get_outer_vm(previous_outer_proof: Option<SerializedAggregatedProof>) -> OuterZkvmHost {
    OuterZkvmHost::new_non_blocking_with_previous_outer_proof(previous_outer_proof)
}
