use sov_cli::wallet_state::PrivateKeyAndAddress;
use std::net::SocketAddr;
use std::num::{NonZeroU64, NonZeroUsize};
use std::path::Path;

use rollup_starter::rollup::StarterRollup;
use rollup_starter::zkvm::InnerZkvm;
use sov_address::EthereumAddress;
use sov_db::config::RollupDbConfig;
use sov_mock_da::MockDaConfig;
use sov_modules_api::Spec;
use sov_modules_rollup_blueprint::FullNodeBlueprint;
use sov_sequencer::preferred::PreferredSequencerConfig;
use sov_sequencer::preferred::RecoveryStrategy;
use sov_sequencer::SeqConfigExtension;
use sov_sequencer::{SequencerConfig, SequencerKindConfig};
use sov_stf_runner::processes::RollupProverConfig;
use sov_stf_runner::{HttpServerConfig, MonitoringConfig, ProofManagerConfig};
use sov_stf_runner::{RollupConfig, RunnerConfig};
use std::str::FromStr;
use tokio::sync::oneshot;

const PROVER_ADDRESS: &str = "0x4fD62a0D0c35e1Fdcd97231A4586E65e7Eb454a5";

pub async fn start_rollup(
    rest_reporting_channel: oneshot::Sender<SocketAddr>,
    genesis_input: std::path::PathBuf,
    rollup_prover_config: Option<RollupProverConfig<InnerZkvm>>,
    da_config: MockDaConfig,
) {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    let rollup_config = RollupConfig {
        storage: RollupDbConfig::default_in_path(temp_path.to_path_buf()),
        runner: RunnerConfig {
            da_polling_interval_ms: 200,
            http_config: HttpServerConfig::localhost_on_free_port(),
            concurrent_sync_tasks: Some(1),
            save_tx_bodies: false,
            da_total_timeout_secs: 3_600,
        },
        da: da_config,
        proof_manager: ProofManagerConfig {
            aggregated_proof_block_jump: NonZeroUsize::new(1).unwrap(),
            prover_address: EthereumAddress::from_str(PROVER_ADDRESS)
                .expect("Prover address is not valid"),
            max_number_of_transitions_in_db: NonZeroU64::new(100).unwrap(),
            max_number_of_transitions_in_memory: NonZeroU64::new(20).unwrap(),
        },
        sequencer: SequencerConfig {
            max_allowed_node_distance_behind: 10,
            max_batch_size_bytes: 1048576,
            max_concurrent_blobs: 16,
            automatic_batch_production: true,
            rollup_address: EthereumAddress::from_str(PROVER_ADDRESS)
                .expect("Sequencer address is not valid"),
            admin_addresses: vec![],
            dropped_tx_ttl_secs: 0,
            blob_processing_timeout_secs: 60,
            extension: Some(SeqConfigExtension {
                max_log_limit: 20000,
            }),
            sequencer_kind_config: SequencerKindConfig::Preferred(PreferredSequencerConfig {
                recovery_strategy: RecoveryStrategy::None,
                minimum_profit_per_tx: 0,
                events_channel_size: 10,
                postgres_connection_string: None,
                disable_state_root_consistency_checks: false,
                ..Default::default()
            }),
        },
        monitoring: MonitoringConfig::standard(),
    };

    let rollup = StarterRollup::default();

    let rollup = rollup
        .create_new_rollup(
            &genesis_input,
            rollup_config,
            rollup_prover_config,
            None,
            None,
        )
        .await
        .unwrap();

    // Ensure there is a non-zero finalized block
    rollup
        .runner
        .da_service()
        .produce_n_blocks_now(5)
        .await
        .unwrap();

    rollup
        .run_and_report_addr(Some(rest_reporting_channel))
        .await
        .unwrap();

    // Close the tempdir explicitly to ensure that rustc doesn't see that it's unused and drop it unexpectedly
    temp_dir.close().unwrap();
}

pub fn read_private_keys<S: Spec>(suffix: &str) -> PrivateKeyAndAddress<S> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

    let private_keys_dir = Path::new(&manifest_dir).join("../../test-data/keys");

    let data = std::fs::read_to_string(private_keys_dir.join(suffix))
        .expect("Unable to read file to string");

    let key_and_address: PrivateKeyAndAddress<S> =
        serde_json::from_str(&data).unwrap_or_else(|_| {
            panic!("Unable to convert data {} to PrivateKeyAndAddress", &data);
        });

    assert!(
        key_and_address.is_matching_to_default(),
        "Inconsistent key data"
    );

    key_and_address
}
