//! Forwards the required trait implementations to the inner non-authenticated runtime.
use price_oracle::PriceReports;
use sov_address::{EthereumAddress, FromVmAddress};
use sov_bank::Amount;
use sov_capabilities::StandardProvenRollupCapabilities as StandardCapabilities;
use sov_eip712_auth::{Eip712AuthenticatorTrait, Secp256k1CryptoSpec};
use sov_evm::EthereumAuthenticator;
use sov_hyperlane_integration::HyperlaneAddress;
use sov_kernels::soft_confirmations::SoftConfirmationsKernel;
use sov_modules_api::capabilities::AuthorizationData;
use sov_modules_api::capabilities::GasEnforcer;
#[cfg(feature = "native")]
use sov_modules_api::capabilities::KernelWithSlotMapping;
use sov_modules_api::capabilities::ProofProcessor;
use sov_modules_api::capabilities::SequencerAuthorization;
use sov_modules_api::capabilities::SequencerRemuneration;
use sov_modules_api::capabilities::TransactionAuthorizer;
use sov_modules_api::capabilities::{Guard, HasCapabilities, HasKernel};
use sov_modules_api::capabilities::{SequencingDataHandler, TransactionAuthenticator};
use sov_modules_api::transaction::ProverReward;
use sov_modules_api::transaction::RemainingFunds;
use sov_modules_api::transaction::SequencerReward;
use sov_modules_api::AggregatedProofPublicData;
use sov_modules_api::ExecutionContext;
use sov_modules_api::Gas;
use sov_modules_api::GetGasPrice;
use sov_modules_api::InfallibleStateAccessor;
use sov_modules_api::InvalidProofError;
use sov_modules_api::OperatingMode;
use sov_modules_api::Rewards;
use sov_modules_api::SequencerType;
use sov_modules_api::SerializedAggregatedProof;
use sov_modules_api::SerializedAttestation;
use sov_modules_api::SerializedChallenge;
use sov_modules_api::SovAttestation;
use sov_modules_api::SovStateTransitionPublicData;
use sov_modules_api::StateReader;
use sov_modules_api::StateWriter;
use sov_modules_api::VersionReader;
use sov_modules_api::{prelude::*, RawTx};
use sov_modules_api::{
    AuthenticatedTransactionData, BlockHooks, DispatchCall, EncodeCall, Genesis, GenesisState,
    RuntimeEventProcessor, Spec, StateCheckpoint, Storage, TxHooks, TxState, TypeErasedEvent,
};
use sov_modules_api::{ModuleError, ModuleId, ModuleInfo, NestedEnumUtils};
use sov_rollup_interface::da::DaSpec;
use sov_state::Kernel;
use sov_state::User;
use std::convert::Infallible;

use crate::authentication::EvmAndEip712AuthenticatorInput;
use crate::Runtime;
use stf_starter_declaration::GenesisConfig;
use stf_starter_declaration::Runtime as RuntimeInner;
use stf_starter_declaration::RuntimeCall;

impl<S: Spec> Genesis for Runtime<S>
where
    <S as Spec>::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type Spec = S;
    type Config = GenesisConfig<S>;

    fn genesis(
        &mut self,
        genesis_rollup_header: &<<Self::Spec as Spec>::Da as DaSpec>::BlockHeader,
        config: &Self::Config,
        state: &mut impl GenesisState<Self::Spec>,
    ) -> Result<(), ModuleError> {
        self.0.genesis(genesis_rollup_header, config, state)
    }
}

impl<S: Spec> DispatchCall for Runtime<S>
where
    <S as Spec>::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type Spec = S;
    type Decodable = RuntimeCall<S>;

    fn encode(decodable: &Self::Decodable) -> Vec<u8> {
        RuntimeInner::<S>::encode(decodable)
    }

    fn dispatch_call<I: StateProvider<Self::Spec>>(
        &mut self,
        message: Self::Decodable,
        state: &mut WorkingSet<Self::Spec, I>,
        context: &Context<Self::Spec>,
    ) -> Result<(), ModuleError> {
        self.0.dispatch_call(message, state, context)
    }

    fn module_id(&self, message: &Self::Decodable) -> &ModuleId {
        self.0.module_id(message)
    }

    fn module_info(
        &self,
        discriminant: <Self::Decodable as NestedEnumUtils>::Discriminants,
    ) -> &dyn ModuleInfo<Spec = Self::Spec> {
        self.0.module_info(discriminant)
    }
}

impl<S: Spec> EncodeCall<sov_bank::Bank<S>> for Runtime<S>
where
    <S as Spec>::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    fn encode_call(data: <sov_bank::Bank<S> as sov_modules_api::Module>::CallMessage) -> Vec<u8> {
        <RuntimeInner<S> as EncodeCall<sov_bank::Bank<S>>>::encode_call(data)
    }

    fn to_decodable(
        data: <sov_bank::Bank<S> as sov_modules_api::Module>::CallMessage,
    ) -> Self::Decodable {
        <RuntimeInner<S> as EncodeCall<sov_bank::Bank<S>>>::to_decodable(data)
    }
}

#[cfg(feature = "acceptance-testing")]
impl<S: Spec> EncodeCall<sov_test_state_consistency::StateConsistency<S>> for Runtime<S>
where
    <S as Spec>::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    fn encode_call(
        data: <sov_test_state_consistency::StateConsistency<S> as sov_modules_api::Module>::CallMessage,
    ) -> Vec<u8> {
        <RuntimeInner<S> as EncodeCall<sov_test_state_consistency::StateConsistency<S>>>::encode_call(data)
    }

    fn to_decodable(
        data: <sov_test_state_consistency::StateConsistency<S> as sov_modules_api::Module>::CallMessage,
    ) -> Self::Decodable {
        <RuntimeInner<S> as EncodeCall<sov_test_state_consistency::StateConsistency<S>>>::to_decodable(data)
    }
}

impl<S: Spec> BlockHooks for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type Spec = S;

    fn begin_rollup_block_hook(
        &mut self,
        visible_hash: &<<Self::Spec as Spec>::Storage as Storage>::Root,
        state: &mut StateCheckpoint<Self::Spec>,
    ) {
        self.0.begin_rollup_block_hook(visible_hash, state)
    }

    fn end_rollup_block_hook(&mut self, state: &mut StateCheckpoint<Self::Spec>) {
        self.0.end_rollup_block_hook(state)
    }
}

impl<S: Spec> TxHooks for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type Spec = S;

    fn pre_dispatch_tx_hook<T: TxState<Self::Spec>>(
        &mut self,
        tx: &AuthenticatedTransactionData<Self::Spec>,
        state: &mut T,
    ) -> anyhow::Result<()> {
        self.0.pre_dispatch_tx_hook(tx, state)
    }

    fn post_dispatch_tx_hook<T: TxState<Self::Spec>>(
        &mut self,
        tx: &AuthenticatedTransactionData<Self::Spec>,
        ctx: &Context<Self::Spec>,
        state: &mut T,
    ) -> anyhow::Result<()> {
        self.0.post_dispatch_tx_hook(tx, ctx, state)
    }
}

#[cfg(feature = "native")]
impl<S: Spec> sov_modules_api::FinalizeHook for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type Spec = S;

    fn finalize_hook(
        &mut self,
        root_hash: &<<Self::Spec as Spec>::Storage as Storage>::Root,
        state: &mut impl sov_modules_api::AccessoryStateReaderAndWriter,
    ) {
        self.0.finalize_hook(root_hash, state)
    }
}

impl<S: Spec> RuntimeEventProcessor for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type RuntimeEvent = stf_starter_declaration::RuntimeEvent<S>;

    fn convert_to_runtime_event(event: TypeErasedEvent) -> Option<Self::RuntimeEvent> {
        RuntimeInner::<S>::convert_to_runtime_event(event)
    }
}

#[cfg(feature = "native")]
impl<S: Spec> sov_modules_api::CliWallet for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type CliStringRepr<T> = stf_starter_declaration::RuntimeMessage<T, S>;
}

#[cfg(feature = "native")]
impl<S: Spec> sov_modules_api::rest::HasRestApi<S> for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    fn rest_api(&self, state: sov_modules_api::rest::ApiState<S>) -> axum::Router<()> {
        self.0.rest_api(state)
    }

    fn openapi_spec(&self) -> Option<utoipa::openapi::OpenApi> {
        self.0.openapi_spec()
    }
}

impl<S: Spec> HasCapabilities<S> for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type Capabilities<'a> = RelayChainCapabilities<'a, S>;
    type SequencingData = PriceReports;

    fn capabilities(&mut self) -> Guard<Self::Capabilities<'_>> {
        Guard::new(RelayChainCapabilities {
            standard: StandardCapabilities {
                bank: &mut self.0.bank,
                sequencer_registry: &mut self.0.sequencer_registry,
                accounts: &mut self.0.accounts,
                uniqueness: &mut self.0.uniqueness,
                gas_payer: &mut self.0.paymaster,
                chain_state: &mut self.0.chain_state,
                operator_incentives: &mut self.0.operator_incentives,
                attester_incentives: &mut self.0.attester_incentives,
                prover_incentives: &mut self.0.prover_incentives,
            },
        })
    }
}

impl<S: Spec> HasKernel<S> for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
{
    type Kernel<'a> = SoftConfirmationsKernel<'a, S>;

    fn inner(&mut self) -> Guard<Self::Kernel<'_>> {
        Guard::new(SoftConfirmationsKernel {
            chain_state: &mut self.0.chain_state,
            blob_storage: &mut self.0.blob_storage,
        })
    }

    #[cfg(feature = "native")]
    fn kernel_with_slot_mapping(&self) -> std::sync::Arc<dyn KernelWithSlotMapping<S>> {
        std::sync::Arc::new(self.0.chain_state.clone())
    }
}

#[cfg(feature = "native")]
impl<T, S> sov_modules_api::cli::CliFrontEnd<Runtime<S>>
    for stf_starter_declaration::RuntimeSubcommand<T, S>
where
    T: clap::Args,
    S: Spec + for<'de> serde::Deserialize<'de>,
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
    stf_starter_declaration::RuntimeSubcommand<T, S>:
        sov_modules_api::cli::CliFrontEnd<RuntimeInner<S>>,
{
    type CliIntermediateRepr<U> =
        <stf_starter_declaration::RuntimeSubcommand<T, S> as sov_modules_api::cli::CliFrontEnd<
            RuntimeInner<S>,
        >>::CliIntermediateRepr<U>;
}

impl<S: Spec> EthereumAuthenticator<S> for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
    S::CryptoSpec: Secp256k1CryptoSpec,
{
    fn add_ethereum_auth(tx: RawTx) -> <Self::Auth as TransactionAuthenticator<S>>::Input {
        EvmAndEip712AuthenticatorInput::Evm(tx)
    }
}

impl<S: Spec> Eip712AuthenticatorTrait<S> for Runtime<S>
where
    S::Address: HyperlaneAddress + FromVmAddress<EthereumAddress>,
    S::CryptoSpec: Secp256k1CryptoSpec,
{
    fn add_eip712_auth(tx: RawTx) -> <Self::Auth as TransactionAuthenticator<S>>::Input {
        EvmAndEip712AuthenticatorInput::Eip712(tx)
    }
}

pub struct RelayChainCapabilities<'a, S: Spec> {
    standard: StandardCapabilities<'a, S, &'a mut sov_paymaster::Paymaster<S>>,
}

impl<S: Spec> SequencingDataHandler<S> for RelayChainCapabilities<'_, S> {
    type SequencingData = PriceReports;

    fn handle_sequencing_data(
        &mut self,
        _data: Self::SequencingData,
        _context: &Context<S>,
        _state: &mut impl TxState<S>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    #[cfg(feature = "native")]
    fn create_sequencing_data(&self) -> Self::SequencingData {
        // This snapshot is attached to every transaction and pruned later by
        // finalize_sequencing_data before it reaches the DA layer.
        // The SDK sizes transactions for batch limits using this pre-pruned size.
        // Its best to keep the snapshot bounded to avoid reducing batch throughput.
        crate::prices::snapshot_prices()
    }

    #[cfg(feature = "native")]
    fn finalize_sequencing_data(
        &mut self,
        data: Self::SequencingData,
        scratchpad: Option<sov_rollup_interface::Bytes>,
    ) -> Self::SequencingData {
        price_oracle::prune_unused(data, scratchpad)
    }
}

impl<S: Spec> GasEnforcer<S> for RelayChainCapabilities<'_, S> {
    fn try_reserve_gas(
        &mut self,
        tx: &AuthenticatedTransactionData<S>,
        gas_price: <S::Gas as Gas>::Price,
        ctx: &mut Context<S>,
        state: &mut impl StateAccessor,
    ) -> anyhow::Result<()> {
        self.standard.try_reserve_gas(tx, gas_price, ctx, state)
    }

    fn try_reserve_gas_for_proof(
        &mut self,
        tx: &AuthenticatedTransactionData<S>,
        gas_price: <S::Gas as Gas>::Price,
        sender: &S::Address,
        state: &mut impl StateAccessor,
    ) -> anyhow::Result<()> {
        self.standard
            .try_reserve_gas_for_proof(tx, gas_price, sender, state)
    }

    fn reward_prover(
        &mut self,
        prover_rewards: &ProverReward,
        operating_mode: OperatingMode,
        state: &mut impl InfallibleStateAccessor,
    ) {
        self.standard
            .reward_prover(prover_rewards, operating_mode, state);
    }

    fn refund_remaining_gas(
        &mut self,
        recipient: &S::Address,
        remaining_funds: &RemainingFunds,
        state: &mut impl InfallibleStateAccessor,
    ) {
        self.standard
            .refund_remaining_gas(recipient, remaining_funds, state);
    }

    fn reward_prover_from_sequencer_balance(
        &mut self,
        amount: Amount,
        sequencer: &S::Address,
        operating_mode: OperatingMode,
        state: &mut impl InfallibleStateAccessor,
    ) -> anyhow::Result<()> {
        self.standard
            .reward_prover_from_sequencer_balance(amount, sequencer, operating_mode, state)
    }

    fn return_escrowed_funds_to_sequencer<
        Accessor: StateReader<Kernel, Error = Infallible>
            + StateWriter<Kernel, Error = Infallible>
            + StateWriter<User, Error = Infallible>
            + StateReader<User, Error = Infallible>
            + VersionReader,
    >(
        &mut self,
        bond_amount: Amount,
        reward: Rewards,
        sequencer: &<S::Da as DaSpec>::Address,
        state: &mut Accessor,
    ) {
        self.standard
            .return_escrowed_funds_to_sequencer(bond_amount, reward, sequencer, state);
    }
}

impl<S: Spec> SequencerAuthorization<S> for RelayChainCapabilities<'_, S> {
    fn is_preferred_sequencer(
        &self,
        sequencer: &<S::Da as DaSpec>::Address,
        state: &mut impl InfallibleStateAccessor,
    ) -> bool {
        self.standard.is_preferred_sequencer(sequencer, state)
    }
}

impl<S: Spec> TransactionAuthorizer<S> for RelayChainCapabilities<'_, S> {
    fn resolve_context(
        &mut self,
        auth_data: &AuthorizationData<S>,
        sequencer: &<S::Da as DaSpec>::Address,
        sequencer_rollup_address: S::Address,
        state: &mut impl StateAccessor,
        sequencing_data: Option<sov_rollup_interface::Bytes>,
        execution_context: ExecutionContext,
        sequencer_type: SequencerType,
    ) -> anyhow::Result<Context<S>> {
        self.standard.resolve_context(
            auth_data,
            sequencer,
            sequencer_rollup_address,
            state,
            sequencing_data,
            execution_context,
            sequencer_type,
        )
    }

    fn resolve_unregistered_context(
        &mut self,
        auth_data: &AuthorizationData<S>,
        sequencer: &<S::Da as DaSpec>::Address,
        state: &mut impl StateAccessor,
        execution_context: ExecutionContext,
    ) -> anyhow::Result<Context<S>> {
        self.standard
            .resolve_unregistered_context(auth_data, sequencer, state, execution_context)
    }

    fn check_uniqueness(
        &self,
        auth_data: &AuthorizationData<S>,
        context: &Context<S>,
        execution_context: &ExecutionContext,
        state: &mut impl StateAccessor,
    ) -> anyhow::Result<()> {
        self.standard
            .check_uniqueness(auth_data, context, execution_context, state)
    }

    fn mark_tx_attempted(
        &mut self,
        auth_data: &AuthorizationData<S>,
        sequencer: &<S::Da as DaSpec>::Address,
        state: &mut impl StateAccessor,
    ) -> anyhow::Result<()> {
        self.standard.mark_tx_attempted(auth_data, sequencer, state)
    }
}

impl<'a, S: Spec> ProofProcessor<S> for RelayChainCapabilities<'a, S> {
    type BondingProofService<K: HasKernel<S>> = <StandardCapabilities<
        'a,
        S,
        &'a mut sov_paymaster::Paymaster<S>,
    > as ProofProcessor<S>>::BondingProofService<K>;

    fn create_bonding_proof_service<K: HasKernel<S>>(
        &self,
        attester_address: S::Address,
        storage_receiver: tokio::sync::watch::Receiver<S::Storage>,
    ) -> Self::BondingProofService<K> {
        self.standard
            .create_bonding_proof_service::<K>(attester_address, storage_receiver)
    }

    fn process_aggregated_proof<ST: TxState<S> + GetGasPrice<Spec = S>>(
        &mut self,
        proof: SerializedAggregatedProof,
        prover_address: &S::Address,
        execution_context: ExecutionContext,
        state: &mut ST,
    ) -> Result<
        (
            AggregatedProofPublicData<S::Address, S::Da, <S::Storage as Storage>::Root>,
            SerializedAggregatedProof,
        ),
        InvalidProofError,
    > {
        self.standard
            .process_aggregated_proof(proof, prover_address, execution_context, state)
    }

    fn process_attestation<ST: TxState<S> + GetGasPrice<Spec = S>>(
        &mut self,
        proof: SerializedAttestation,
        prover_address: &S::Address,
        state: &mut ST,
    ) -> Result<SovAttestation<S>, InvalidProofError> {
        self.standard
            .process_attestation(proof, prover_address, state)
    }

    fn process_challenge<ST: TxState<S> + GetGasPrice<Spec = S>>(
        &mut self,
        proof: SerializedChallenge,
        rollup_height: sov_rollup_interface::common::SlotNumber,
        prover_address: &S::Address,
        state: &mut ST,
    ) -> Result<SovStateTransitionPublicData<S>, InvalidProofError> {
        self.standard
            .process_challenge(proof, rollup_height, prover_address, state)
    }
}

impl<S: Spec> SequencerRemuneration<S> for RelayChainCapabilities<'_, S> {
    fn reward_sequencer_or_refund<
        Accessor: StateReader<Kernel, Error = Infallible>
            + StateWriter<Kernel, Error = Infallible>
            + StateWriter<User, Error = Infallible>
            + StateReader<User, Error = Infallible>,
    >(
        &mut self,
        sequencer: &<S::Da as DaSpec>::Address,
        sequencer_rollup_address: &S::Address,
        reward: SequencerReward,
        state: &mut Accessor,
    ) {
        self.standard.reward_sequencer_or_refund(
            sequencer,
            sequencer_rollup_address,
            reward,
            state,
        );
    }

    fn preferred_sequencer(
        &self,
        state: &mut impl InfallibleStateAccessor,
    ) -> Option<<S::Da as DaSpec>::Address> {
        self.standard.preferred_sequencer(state)
    }
}
