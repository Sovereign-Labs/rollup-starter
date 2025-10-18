//! This is a technical only module to forward all necessary implementations to inner, non-authenticated Runtime
use sov_address::{EthereumAddress, FromVmAddress};
use sov_capabilities::StandardProvenRollupCapabilities as StandardCapabilities;
use sov_evm::{EthereumAuthenticator, EvmAuthenticatorInput};
use sov_hyperlane_integration::HyperlaneAddress;
use sov_kernels::soft_confirmations::SoftConfirmationsKernel;
#[cfg(feature = "native")]
use sov_modules_api::capabilities::KernelWithSlotMapping;
use sov_modules_api::capabilities::TransactionAuthenticator;
use sov_modules_api::capabilities::{Guard, HasCapabilities, HasKernel};
use sov_modules_api::{prelude::*, RawTx};
use sov_modules_api::{
    AuthenticatedTransactionData, BlockHooks, DispatchCall, EncodeCall, Genesis, GenesisState,
    RuntimeEventProcessor, Spec, StateCheckpoint, Storage, TxHooks, TxState, TypeErasedEvent,
};
use sov_modules_api::{ModuleError, ModuleId, ModuleInfo, NestedEnumUtils};
use sov_rollup_interface::da::DaSpec;

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
    type Capabilities<'a> = StandardCapabilities<'a, S, &'a mut sov_paymaster::Paymaster<S>>;

    fn capabilities(&mut self) -> Guard<Self::Capabilities<'_>> {
        Guard::new(StandardCapabilities {
            bank: &mut self.0.bank,
            sequencer_registry: &mut self.0.sequencer_registry,
            accounts: &mut self.0.accounts,
            uniqueness: &mut self.0.uniqueness,
            gas_payer: &mut self.0.paymaster,
            operator_incentives: &mut self.0.operator_incentives,
            attester_incentives: &mut self.0.attester_incentives,
            prover_incentives: &mut self.0.prover_incentives,
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
{
    fn add_ethereum_auth(tx: RawTx) -> <Self::Auth as TransactionAuthenticator<S>>::Input {
        EvmAuthenticatorInput::Evm(tx)
    }
}
