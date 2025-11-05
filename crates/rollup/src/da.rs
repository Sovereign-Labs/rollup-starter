#[cfg(feature = "celestia_da")]
mod celestia {
    pub use sov_celestia_adapter::verifier::CelestiaSpec as DaSpec;
    pub use sov_celestia_adapter::CelestiaService as DaService;
    use sov_modules_api::macros::config_value;

    use sov_celestia_adapter::{
        types::Namespace,
        verifier::{CelestiaVerifier, RollupParams},
    };
    use sov_modules_api::{prelude::tokio::sync::watch::Receiver, Spec};
    use sov_rollup_interface::da::DaVerifier;
    use sov_stf_runner::RollupConfig;

    pub const ROLLUP_BATCH_NAMESPACE: Namespace =
        Namespace::const_v0(config_value!("BATCH_NAMESPACE"));

    pub const ROLLUP_PROOF_NAMESPACE: Namespace =
        Namespace::const_v0(config_value!("PROOF_NAMESPACE"));

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

#[cfg(feature = "mock_da_external")]
mod mock_external {
    pub use sov_mock_da::storable::rpc::StorableMockDaClient as DaService;
    pub use sov_mock_da::MockDaSpec as DaSpec;
    use sov_mock_da::MockDaVerifier;
    use sov_modules_api::{prelude::tokio::sync::watch::Receiver, Spec};
    use sov_stf_runner::RollupConfig;

    pub fn new_verifier() -> MockDaVerifier {
        MockDaVerifier::default()
    }

    pub async fn new_da_service<S: Spec>(
        rollup_config: &RollupConfig<S::Address, DaService>,
        _shutdown_receiver: Receiver<()>,
    ) -> DaService {
        DaService::from_config(rollup_config.da.clone())
            .expect("Failed to create DA service: Invalid URLs")
    }
}

#[cfg(feature = "celestia_da")]
pub use celestia::{new_da_service, new_verifier, DaService, DaSpec};

#[cfg(feature = "mock_da")]
pub use mock::{new_da_service, new_verifier, DaService, DaSpec};

#[cfg(feature = "mock_da_external")]
pub use mock_external::{new_da_service, new_verifier, DaService, DaSpec};
