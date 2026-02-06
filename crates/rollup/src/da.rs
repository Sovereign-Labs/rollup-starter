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
        shutdown_receiver: Receiver<()>,
    ) -> DaService {
        DaService::new(
            rollup_config.da.clone(),
            RollupParams {
                rollup_batch_namespace: ROLLUP_BATCH_NAMESPACE,
                rollup_proof_namespace: ROLLUP_PROOF_NAMESPACE,
            },
            shutdown_receiver,
        )
        .await
    }
}

pub use celestia::{new_da_service, new_verifier, DaService, DaSpec};
