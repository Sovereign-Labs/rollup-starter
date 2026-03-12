use alloy_primitives::{address, Address as EvmAddress, Bytes};
use alloy_sol_types::sol;
use anyhow::{anyhow, bail};
use borsh::{BorshDeserialize, BorshSerialize};
use revm::handler::Handler;
use revm::context::{
    result::{EVMError, ExecutionResult, InvalidTransaction},
    TxEnv,
};
use revm::primitives::hardfork::SpecId;
use revm::{Context as EvmContext, ExecuteEvm, MainBuilder, MainContext};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sov_modules_api::macros::{serialize, UniversalWallet};
use sov_modules_api::{
    Context as CallContext, EventEmitter, Module, ModuleId, ModuleInfo, ModuleRestApi, Spec,
    StateMap, TxState,
};
use sov_state::BcsCodec;
use std::marker::PhantomData;

use crate::evm_helpers::{
    encode_match_authorizer_input, execute_authorizer, EvmTxState, EvmTxStateError,
    StaticAuthorizerHandler,
};

mod evm_helpers;

const AUTHORIZER_GAS_LIMIT: u64 = 100_000;
const AUTHORIZER_MEMORY_LIMIT: u64 = 50 * 1024 * 1024;
const AUTHORIZER_ADDRESS: EvmAddress = address!("0000000000000000000000000000000000001000");
const AUTHORIZER_CALLER: EvmAddress = address!("0000000000000000000000000000000000000000");

/// A new module:
/// - Must derive `ModuleInfo`
/// - Must contain `[id]` field
/// - Can contain any number of ` #[state]` or `[module]` fields
/// - Can derive ModuleRestApi to automatically generate Rest API endpoints
#[derive(Clone, ModuleInfo, ModuleRestApi)]
pub struct Derive<S: Spec> {
    /// Id of the module.
    #[id]
    pub id: ModuleId,

    /// Some value kept in the state.
    #[state]
    pub authorizers: StateMap<S::Address, Vec<u8>, BcsCodec>,

    /// You can disregard this, as its only used to satisfy
    /// the compiler for the type parameter `S` not being used.
    #[phantom]
    pub phantom: PhantomData<S>,
}

impl<S: Spec> Module for Derive<S> {
    type Spec = S;

    type Config = ();

    type CallMessage = CallMessage<S>;

    type Event = Event<S>;

    type Error = anyhow::Error;

    fn call(
        &mut self,
        msg: Self::CallMessage,
        context: &CallContext<Self::Spec>,
        state: &mut impl TxState<S>,
    ) -> anyhow::Result<()> {
        match msg {
            CallMessage::Match(match_msg) => {
                let Some(long_authorizer) = self.authorizers.get(&match_msg.long_account, state)?
                else {
                    bail!(
                        "No authorizer configured for long account {}",
                        match_msg.long_account
                    );
                };

                let Some(short_authorizer) =
                    self.authorizers.get(&match_msg.short_account, state)?
                else {
                    bail!(
                        "No authorizer configured for long account {}",
                        match_msg.short_account
                    );
                };
                let calldata = encode_match_authorizer_input(&match_msg);
                execute_authorizer(&long_authorizer, calldata.clone())?;
                execute_authorizer(&short_authorizer, calldata)?;
                // Note: Code to actually update exchange state based on the match is omitted here.
                Ok(())
            }
            CallMessage::SetAuthorizer(authorizer) => {
                // Allows the transaction sender to set the EVM bytecode for transactions against their own account.
                self.authorizers.set(context.sender(), &authorizer, state)?;
                self.emit_event(
                    state,
                    Event::AuthorizerSet(*context.sender(), hex::encode(authorizer)),
                );
                Ok(())
            }
        }
    }
}


#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    JsonSchema,
    UniversalWallet,
    Deserialize,
    Serialize,
    BorshDeserialize,
    BorshSerialize,
)]
#[schemars(rename = "call_message")]
#[schemars(bound = "S: Spec")]
#[serde(bound = "S: Spec")]
#[serde(rename_all = "snake_case")]
/// we support two transaction types
pub enum CallMessage<S: Spec> {
    /// Match simulates the result of a match on the exchange
    Match(Match<S>),
    /// SetAuthorizer allows the transaction sender to set the EVM bytecode which is used to accept or reject matches against their account.
    SetAuthorizer(Vec<u8>),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    JsonSchema,
    Deserialize,
    Serialize,
    BorshDeserialize,
    BorshSerialize,
)]
#[schemars(rename = "call_message")]
#[schemars(bound = "S: Spec")]
pub enum Event<S: Spec> {
    AuthorizerSet(S::Address, String),
}

fn run_authorizer(authorizer: &[u8], calldata: Vec<u8>) -> anyhow::Result<ExecutionResult> {
    let db = EvmTxState::new(authorizer)?;
    // Cratft a dummy transaction for the authorizer that calls the authorizer precompile
    let tx = TxEnv::builder()
        .caller(AUTHORIZER_CALLER)
        .call(AUTHORIZER_ADDRESS)
        .gas_limit(AUTHORIZER_GAS_LIMIT)
        .data(Bytes::from(calldata))
        .chain_id(Some(0))
        .build()
        .map_err(|err| anyhow!("Failed to build the authorizer transaction: {err:?}"))?;

    let mut evm = EvmContext::mainnet()
        .with_db(db)
        .with_tx(tx)
        .modify_cfg_chained(|cfg| {
            cfg.chain_id = 0;
            cfg.tx_chain_id_check = false;
            cfg.spec = SpecId::CANCUN;
            cfg.disable_nonce_check = true;
            cfg.disable_balance_check = true;
            cfg.disable_block_gas_limit = true;
            cfg.disable_eip3607 = true;
            cfg.disable_base_fee = true;
            cfg.memory_limit = AUTHORIZER_MEMORY_LIMIT;
        })
        .build_mainnet();

    // Use revm `run_system_call` to execute the authorizer without extra setup or teardown
    let result = StaticAuthorizerHandler::<_, AuthorizerEvmError, _>::default()
        .run_system_call(&mut evm)
        .map_err(|err| anyhow!("Failed to execute authorizer: {err:?}"));
    let _ = evm.finalize();
    result
}


type AuthorizerEvmError = EVMError<EvmTxStateError, InvalidTransaction>;

sol! {
    struct MatchAuthorizerInput {
        uint64 id;
        uint64 price;
        uint64 quantity;
        bytes longAccount;
        bytes shortAccount;
        uint64 timestamp;
        bytes longAccountCalldata;
        bytes shortAccountCalldata;
    }

    error MatchRejected(string reason);
}

#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, UniversalWallet)]
#[serialize(Borsh, Serde)]
#[serde(rename = "match")]
#[schemars(rename = "match")]
#[schemars(bound = "S: Spec")]
pub struct Match<S: Spec> {
    pub id: u64,
    pub price: u64,                // fixed-point (e.g., price * 1e6)
    pub quantity: u64,             // fixed-point
    pub long_account: S::Address,  // buyer
    pub short_account: S::Address, // seller
    pub timestamp: u64,
    pub long_account_calldata: Vec<u8>,
    pub short_account_calldata: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sov_test_utils::TestSpec;

    fn sample_match() -> Match<TestSpec> {
        Match {
            id: 7,
            price: 1_500,
            quantity: 3,
            long_account: <TestSpec as Spec>::Address::from([1; 28]),
            short_account: <TestSpec as Spec>::Address::from([2; 28]),
            timestamp: 123_456,
            long_account_calldata: vec![0xaa, 0xbb],
            short_account_calldata: vec![0xcc, 0xdd],
        }
    }

    #[test]
    fn stop_authorizer_succeeds() {
        execute_authorizer(&[0x00], encode_match_authorizer_input(&sample_match())).unwrap();
    }

    #[test]
    fn revert_authorizer_returns_a_rejection() {
        let err = execute_authorizer(
            &[0x60, 0x2a, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xfd],
            vec![],
        )
        .unwrap_err();

        assert!(err.to_string().contains("0x"));
    }

    #[test]
    fn custom_match_rejection_is_decoded() {
        let encoded = MatchRejected {
            reason: "Only asset 4 is allowed".to_owned(),
        }
        .abi_encode();

        assert_eq!(
            decode_authorizer_revert(&encoded),
            "Only asset 4 is allowed"
        );
    }

    #[test]
    fn static_execution_rejects_sstore() {
        let err = execute_authorizer(&[0x60, 0x01, 0x60, 0x00, 0x55], vec![]).unwrap_err();

        assert!(err.to_string().contains("StateChangeDuringStaticCall"));
    }

    #[test]
    fn storage_reads_return_zero() {
        execute_authorizer(
            &[
                0x60, 0x00, 0x54, 0x60, 0x00, 0x14, 0x60, 0x0e, 0x57, 0x60, 0x00, 0x60, 0x00, 0xfd,
                0x5b, 0x00,
            ],
            vec![],
        )
        .unwrap();
    }

    #[test]
    fn blockhash_reads_return_zero() {
        execute_authorizer(
            &[
                0x60, 0x00, 0x40, 0x60, 0x00, 0x14, 0x60, 0x0e, 0x57, 0x60, 0x00, 0x60, 0x00, 0xfd,
                0x5b, 0x00,
            ],
            vec![],
        )
        .unwrap();
    }

    #[test]
    fn non_precompile_accounts_have_no_code() {
        execute_authorizer(
            &[
                0x73, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
                0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x3b, 0x60, 0x00, 0x14, 0x60, 0x21, 0x57,
                0x60, 0x00, 0x60, 0x00, 0xfd, 0x5b, 0x00,
            ],
            vec![],
        )
        .unwrap();
    }
}
