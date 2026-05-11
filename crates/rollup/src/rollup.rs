#![deny(missing_docs)]
//! StarterRollup provides a minimal self-contained rollup implementation

use async_trait::async_trait;
use axum::extract::{ConnectInfo, State};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Json;
use sov_address::{EthereumAddress, EvmCryptoSpec, FromVmAddress};
use sov_db::ledger_db::LedgerDb;
use sov_db::storage_manager::NomtStorageManager;
use sov_eip712_auth::Eip712AuthenticatorTrait;
use sov_hyperlane_integration::HyperlaneAddress;
use sov_modules_api::capabilities::TransactionAuthenticator;
use sov_modules_api::configurable_spec::ConfigurableSpec;
use sov_modules_api::SequencerType;
use sov_modules_api::{RawTx, Spec};
use sov_modules_rollup_blueprint::pluggable_traits::PluggableSpec;
use sov_modules_rollup_blueprint::proof_sender::SovApiProofSender;
use sov_modules_rollup_blueprint::{FullNodeBlueprint, RollupBlueprint, SequencerCreationReceipt};
use sov_modules_stf_blueprint::Runtime as RuntimeTrait;
use sov_rest_utils::ApiResult;
use sov_rollup_interface::da::DaBlobHash;
use sov_rollup_interface::node::da::DaService as DaServiceTrait;
use sov_sequencer::rest_api::{AcceptTx, TxInfoWithConfirmation};
use std::net::SocketAddr;

use sov_mock_zkvm::{MockCodeCommitment, MockZkVerifier};
use sov_rollup_full_node_interface::StateUpdateReceiver;
use sov_rollup_interface::common::SlotNumber;
use sov_rollup_interface::execution_mode::Native;
use sov_rollup_interface::node::ledger_api::LedgerStateProvider;
use sov_rollup_interface::node::SyncStatus;
use sov_rollup_interface::zk::aggregated_proof::{
    AggregateProofVerifier, AggregatedProofPublicData, SerializedAggregatedProof,
};
use sov_sequencer::{ProofBlobSender, SeqConfigExtension, Sequencer, TxStatus};
use sov_state::nomt::prover_storage::NomtProverStorage;
use sov_state::DefaultStorageSpec;
use sov_state::Storage;
use sov_stf_runner::processes::{ParallelProverService, RollupProverConfig};
use sov_stf_runner::RollupConfig;
use std::sync::Arc;
use stf_starter::Runtime;
use tokio::sync::watch;

use crate::da::{new_da_service, new_verifier, DaService, DaSpec};
use crate::zkvm::{create_inner_vm, get_outer_vm, Hasher, InnerZkvm, OuterZkvm};

type NativeStorage = NomtProverStorage<
    DefaultStorageSpec<Hasher>,
    <DaSpec as sov_rollup_interface::da::DaSpec>::SlotHash,
>;
/// A configurable spec instance with EthereumAddress
pub type EthSpec<Da, InnerZkvm, OuterZkvm> = ConfigurableSpec<
    Da,
    InnerZkvm,
    OuterZkvm,
    EthereumAddress,
    Native,
    EvmCryptoSpec,
    NativeStorage,
>;

/// Starter rollup implementation.
#[derive(Default)]
pub struct StarterRollup<M> {
    phantom: std::marker::PhantomData<M>,
}

/// This is the place where all the rollup components come together, and
/// they can be easily swapped with alternative implementations as needed.
impl RollupBlueprint<Native> for StarterRollup<Native>
where
    EthSpec<DaSpec, InnerZkvm, OuterZkvm>: PluggableSpec,
    <EthSpec<DaSpec, InnerZkvm, OuterZkvm> as Spec>::Address:
        HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type Spec = EthSpec<DaSpec, InnerZkvm, OuterZkvm>;
    type Runtime = Runtime<Self::Spec>;
}

#[async_trait]
impl FullNodeBlueprint<Native> for StarterRollup<Native> {
    type DaService = DaService;

    type StorageManager = NomtStorageManager<DaSpec, Hasher, NativeStorage>;

    type ProverService = ParallelProverService<
        <Self::Spec as Spec>::Address,
        <<Self::Spec as Spec>::Storage as Storage>::Root,
        <<Self::Spec as Spec>::Storage as Storage>::Witness,
        Self::DaService,
        <Self::Spec as Spec>::InnerZkvm,
        <Self::Spec as Spec>::OuterZkvm,
    >;

    type ProofSender = SovApiProofSender<Self::Spec>;

    async fn create_endpoints(
        &self,
        state_update_receiver: StateUpdateReceiver<<Self::Spec as Spec>::Storage>,
        sync_status_receiver: watch::Receiver<SyncStatus>,
        shutdown_receiver: watch::Receiver<()>,
        ledger_db: &LedgerDb,
        sequencer: &SequencerCreationReceipt<Self::Spec>,
        _da_service: &Self::DaService,
        rollup_config: &RollupConfig<<Self::Spec as Spec>::Address, Self::DaService>,
    ) -> anyhow::Result<sov_modules_api::NodeEndpoints> {
        sov_modules_rollup_blueprint::register_endpoints::<Self, _>(
            state_update_receiver.clone(),
            sync_status_receiver,
            shutdown_receiver,
            ledger_db,
            sequencer,
            rollup_config,
        )
        .await
    }

    async fn create_da_service(
        &self,
        rollup_config: &RollupConfig<<Self::Spec as Spec>::Address, Self::DaService>,
        shutdown_receiver: tokio::sync::watch::Receiver<()>,
    ) -> Self::DaService {
        new_da_service::<Self::Spec>(rollup_config, shutdown_receiver).await
    }

    async fn create_prover_service(
        &self,
        _prover_config: RollupProverConfig,
        rollup_config: &RollupConfig<<Self::Spec as Spec>::Address, Self::DaService>,
        _da_service: &Self::DaService,
        ledger_db: &LedgerDb,
        start_fresh_outer_proof_on_resync: bool,
    ) -> (Self::ProverService, Option<SlotNumber>) {
        let previous_public_data: Option<
            AggregatedProofPublicData<
                <Self::Spec as Spec>::Address,
                DaSpec,
                <<Self::Spec as Spec>::Storage as Storage>::Root,
            >,
        > = read_latest_aggregated_proof(ledger_db).await.map(|proof| {
            AggregateProofVerifier::<MockZkVerifier>::new(MockCodeCommitment::default())
                .verify(&proof)
                .expect("Persisted aggregated proof failed verification")
        });

        let latest_proof_final_slot = previous_public_data.as_ref().map(|p| p.final_slot_number);

        let previous_for_outer = if start_fresh_outer_proof_on_resync {
            None
        } else {
            previous_public_data.as_ref()
        };

        let inner_vm = create_inner_vm().await;
        let outer_vm = get_outer_vm(previous_for_outer);
        let da_verifier = new_verifier();

        let num_threads = rollup_config.proof_manager.prover_thread_count();

        let prover = ParallelProverService::new_with_default_workers(
            inner_vm,
            outer_vm,
            da_verifier,
            rollup_config.proof_manager.prover_address,
            num_threads,
        );

        (prover, latest_proof_final_slot)
    }

    fn create_storage_manager(
        &self,
        rollup_config: &RollupConfig<<Self::Spec as Spec>::Address, Self::DaService>,
        witness_generation: bool,
    ) -> anyhow::Result<Self::StorageManager> {
        NomtStorageManager::new(rollup_config.storage.clone(), witness_generation)
    }

    fn create_proof_sender(
        &self,
        _rollup_config: &RollupConfig<<Self::Spec as Spec>::Address, Self::DaService>,
        sequence_number_provider: Arc<dyn ProofBlobSender>,
    ) -> anyhow::Result<Self::ProofSender> {
        Ok(Self::ProofSender::new(sequence_number_provider))
    }

    async fn sequencer_additional_apis<Seq>(
        &self,
        sequencer: Seq,
        rollup_config: &RollupConfig<<Self::Spec as Spec>::Address, Self::DaService>,
        shutdown_receiver: tokio::sync::watch::Receiver<()>,
        sequencer_da_address: <<Self::Spec as Spec>::Da as sov_rollup_interface::da::DaSpec>::Address,
    ) -> anyhow::Result<sov_modules_api::NodeEndpoints>
    where
        Seq: Sequencer<Spec = Self::Spec, Rt = Self::Runtime, Da = Self::DaService>,
    {
        let config = rollup_config.sequencer.extension.as_ref().unwrap();
        let eth_rpc_config = sov_ethereum::EthRpcConfig {
            extension: SeqConfigExtension {
                max_log_limit: config.max_log_limit,
                response_size_limit: config.response_size_limit, // Limit our response size to 1MB, leaving 30kb for headers, overhead, and misestimation.
            },
            sequencer_rollup_address: rollup_config.sequencer.rollup_address,
            sequencer_da_address,
            sequencer_type: SequencerType::Preferred,
            shutdown_receiver,
        };

        let axum_router = axum::Router::new()
            .route("/sequencer/eip712_tx", post(accept_eip712_tx::<Seq>))
            .with_state(sequencer.clone());

        Ok(sov_modules_api::NodeEndpoints {
            axum_router,
            jsonrpsee_module: sov_ethereum::get_ethereum_rpc(eth_rpc_config, sequencer)
                .remove_context(),
            ..Default::default()
        })
    }
}

impl sov_modules_rollup_blueprint::WalletBlueprint<Native> for StarterRollup<Native> {}

/// Handler for accepting EIP712 authenticated transactions
async fn accept_eip712_tx<Seq>(
    connect_info: ConnectInfo<SocketAddr>,
    State(sequencer): State<Seq>,
    tx: Json<AcceptTx>,
) -> ApiResult<
    TxInfoWithConfirmation<DaBlobHash<<Seq::Da as DaServiceTrait>::Spec>, Seq::Confirmation>,
>
where
    Seq: Sequencer + 'static,
    Seq::Rt: Eip712AuthenticatorTrait<Seq::Spec>,
    <Seq::Rt as RuntimeTrait<Seq::Spec>>::Auth: TransactionAuthenticator<Seq::Spec>,
{
    let raw_tx = RawTx::new(tx.0.body.blob);
    let encoded_tx = Seq::Rt::encode_with_eip712_auth(raw_tx);

    // Submit to sequencer (similar to axum_accept_tx but with EIP712 auth)
    let tx_with_hash = sequencer
        .accept_tx(encoded_tx, connect_info.0.ip())
        .await
        .map_err(|e| {
            if e.status.is_server_error() {
                tracing::error!(error = ?e, "Error accepting EIP712 transaction");
            }
            IntoResponse::into_response(e)
        })?;

    Ok(TxInfoWithConfirmation {
        id: tx_with_hash.tx_hash,
        confirmation: tx_with_hash.confirmation,
        status: TxStatus::Submitted,
    }
    .into())
}

async fn read_latest_aggregated_proof(ledger_db: &LedgerDb) -> Option<SerializedAggregatedProof> {
    Some(
        ledger_db
            .get_latest_aggregated_proof()
            .await
            .expect("Failed to read latest aggregated proof from ledger DB")?
            .proof,
    )
}
