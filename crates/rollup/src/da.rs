#[cfg(feature = "celestia_da")]
mod celestia {
    pub use sov_celestia_adapter::verifier::CelestiaSpec as DaSpec;
    pub use sov_celestia_adapter::CelestiaService as DaService;
    use sov_celestia_adapter::{
        types::Namespace,
        verifier::{CelestiaVerifier, RollupParams},
    };
    use sov_modules_api::{prelude::tokio::sync::watch::Receiver, Spec};
    use sov_rollup_interface::da::DaVerifier;
    use sov_stf_runner::RollupConfig;

    /// The rollup stores its data in the namespace "sov-test-b" on Celestia.
    /// You can change this constant to point your rollup at a different namespace.
    const ROLLUP_BATCH_NAMESPACE: Namespace = Namespace::const_v0(*b"sov-test-b");

    /// The rollup stores the zk proofs in the namespace "sov-test-p" on Celestia.
    const ROLLUP_PROOF_NAMESPACE: Namespace = Namespace::const_v0(*b"sov-test-p");

    pub fn new_verifier() -> CelestiaVerifier {
        CelestiaVerifier::new(RollupParams {
            rollup_batch_namespace: ROLLUP_BATCH_NAMESPACE,
            rollup_proof_namespace: ROLLUP_PROOF_NAMESPACE,
        })
    }

    pub async fn new_da_service<S: Spec>(
        rollup_config: &RollupConfig<S::Address, DaService>,
        _shutdown_receiver: Receiver<()>,
    ) -> DaService {
        DaService::new(
            rollup_config.da.clone(),
            RollupParams {
                rollup_batch_namespace: ROLLUP_BATCH_NAMESPACE,
                rollup_proof_namespace: ROLLUP_PROOF_NAMESPACE,
            },
        )
        .await
    }
}

#[cfg(feature = "mock_da")]
mod mock {
    pub use sov_mock_da::storable::local_service::StorableMockDaService as DaService;
    pub use sov_mock_da::MockDaSpec as DaSpec;
    use sov_mock_da::MockDaVerifier;
    use sov_modules_api::{prelude::tokio::sync::watch::Receiver, Spec};
    use sov_stf_runner::RollupConfig;

    pub fn new_verifier() -> MockDaVerifier {
        MockDaVerifier::default()
    }

    pub async fn new_da_service<S: Spec>(
        rollup_config: &RollupConfig<S::Address, DaService>,
        shutdown_receiver: Receiver<()>,
    ) -> DaService {
        DaService::from_config(rollup_config.da.clone(), shutdown_receiver).await
    }
}

#[cfg(feature = "celestia_da")]
pub use celestia::{new_da_service, new_verifier, DaService, DaSpec};

#[cfg(feature = "mock_da")]
pub use mock::{new_da_service, new_verifier, DaService, DaSpec};
