use alloy_primitives::{address, Address as EvmAddress, Bytes, B256, U256};
use alloy_sol_types::{decode_revert_reason, sol, SolError, SolValue};
use anyhow::{anyhow, bail, Context as _, Result};
use borsh::{BorshDeserialize, BorshSerialize};
use revm::context::{
    result::{EVMError, ExecutionResult, HaltReason, InvalidTransaction},
    Cfg, ContextTr, JournalTr, LocalContextTr, Transaction, TxEnv,
};
use revm::handler::{execution, EvmTr, EvmTrError, FrameResult, FrameTr, Handler};
use revm::interpreter::{interpreter_action::FrameInit, FrameInput, SharedMemory};
use revm::primitives::hardfork::SpecId;
use revm::state::{AccountInfo, Bytecode};
use revm::{Context as EvmContext, Database, ExecuteEvm, MainBuilder, MainContext};
use revm_database_interface::DBErrorMarker;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sov_modules_api::macros::{serialize, UniversalWallet};
use sov_modules_api::{
    Context as CallContext, EventEmitter, Module, ModuleId, ModuleInfo, ModuleRestApi, Spec,
    StateMap, TxState,
};
use sov_state::BcsCodec;
use std::marker::PhantomData;

const AUTHORIZER_GAS_LIMIT: u64 = 100_000;
const AUTHORIZER_MEMORY_LIMIT: u64 = 50 * 1024 * 1024;
const AUTHORIZER_ADDRESS: EvmAddress = address!("0000000000000000000000000000000000001000");
const AUTHORIZER_CALLER: EvmAddress = address!("0000000000000000000000000000000000000000");

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
    ) -> Result<()> {
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
                Ok(())
            }
            CallMessage::SetAuthorizer(authorizer) => {
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

pub struct EvmTxState {
    code: Bytecode,
    code_hash: B256,
}

#[derive(thiserror::Error, Debug)]
pub enum EvmTxStateError {
    #[error("Code not found")]
    CodeNotFound,
}

impl DBErrorMarker for EvmTxStateError {}

impl EvmTxState {
    fn new(code: &[u8]) -> Result<Self> {
        let code = Bytecode::new_raw_checked(Bytes::from(code.to_vec()))
            .context("Authorizer bytecode is invalid")?;
        let code_hash = code.hash_slow();

        Ok(Self { code, code_hash })
    }

    fn authorizer_account(&self) -> AccountInfo {
        AccountInfo::new(U256::ZERO, 0, self.code_hash, self.code.clone())
    }
}

impl Database for EvmTxState {
    type Error = EvmTxStateError;

    fn basic(&mut self, address: EvmAddress) -> Result<Option<AccountInfo>, Self::Error> {
        if address == AUTHORIZER_ADDRESS {
            Ok(Some(self.authorizer_account()))
        } else {
            Ok(None)
        }
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        if code_hash == self.code_hash {
            Ok(self.code.clone())
        } else {
            Err(EvmTxStateError::CodeNotFound)
        }
    }

    fn storage(&mut self, _address: EvmAddress, _index: U256) -> Result<U256, Self::Error> {
        Ok(U256::ZERO)
    }

    fn block_hash(&mut self, _number: u64) -> Result<B256, Self::Error> {
        Ok(B256::ZERO)
    }
}

#[derive(Debug, Clone)]
struct StaticAuthorizerHandler<CTX, ERROR, FRAME> {
    _phantom: PhantomData<(CTX, ERROR, FRAME)>,
}

impl<EVM, ERROR, FRAME> Handler for StaticAuthorizerHandler<EVM, ERROR, FRAME>
where
    EVM: EvmTr<Context: ContextTr<Journal: JournalTr>, Frame = FRAME>,
    ERROR: EvmTrError<EVM>,
    FRAME: FrameTr<FrameResult = FrameResult, FrameInit = FrameInit>,
{
    type Evm = EVM;
    type Error = ERROR;
    type HaltReason = HaltReason;

    fn first_frame_input(
        &mut self,
        evm: &mut Self::Evm,
        gas_limit: u64,
    ) -> Result<FrameInit, Self::Error> {
        let ctx = evm.ctx_mut();
        let mut memory = SharedMemory::new_with_buffer(ctx.local().shared_memory_buffer().clone());
        memory.set_memory_limit(ctx.cfg().memory_limit());

        let (tx, journal) = ctx.tx_journal_mut();
        let bytecode = if let Some(&to) = tx.kind().to() {
            let account = &journal.load_account_with_code(to)?.info;

            if let Some(Bytecode::Eip7702(eip7702_bytecode)) = &account.code {
                let delegated_address = eip7702_bytecode.delegated_address;
                let account = &journal.load_account_with_code(delegated_address)?.info;
                Some((
                    account.code.clone().unwrap_or_default(),
                    account.code_hash(),
                ))
            } else {
                Some((
                    account.code.clone().unwrap_or_default(),
                    account.code_hash(),
                ))
            }
        } else {
            None
        };

        let mut frame_input = execution::create_init_frame(tx, bytecode, gas_limit);
        let FrameInput::Call(inputs) = &mut frame_input else {
            unreachable!("authorizer execution must be a call");
        };
        inputs.is_static = true;

        Ok(FrameInit {
            depth: 0,
            memory,
            frame_input,
        })
    }
}

impl<CTX, ERROR, FRAME> Default for StaticAuthorizerHandler<CTX, ERROR, FRAME> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

fn encode_match_authorizer_input<S: Spec>(match_msg: &Match<S>) -> Vec<u8> {
    MatchAuthorizerInput {
        id: match_msg.id,
        price: match_msg.price,
        quantity: match_msg.quantity,
        longAccount: Bytes::from(match_msg.long_account.as_ref().to_vec()),
        shortAccount: Bytes::from(match_msg.short_account.as_ref().to_vec()),
        timestamp: match_msg.timestamp,
        longAccountCalldata: Bytes::from(match_msg.long_account_calldata.clone()),
        shortAccountCalldata: Bytes::from(match_msg.short_account_calldata.clone()),
    }
    .abi_encode()
}

fn decode_authorizer_revert(output: &[u8]) -> String {
    if let Ok(err) = MatchRejected::abi_decode(output) {
        return err.reason;
    }

    decode_revert_reason(output).unwrap_or_else(|| format!("0x{}", hex::encode(output)))
}

fn execute_authorizer(authorizer: &[u8], calldata: Vec<u8>) -> Result<()> {
    match run_authorizer(authorizer, calldata)? {
        ExecutionResult::Success { .. } => Ok(()),
        ExecutionResult::Revert { output, .. } => {
            bail!(
                "Authorizer rejected the match: {}",
                decode_authorizer_revert(&output)
            )
        }
        ExecutionResult::Halt { reason, .. } => {
            bail!("Authorizer halted during evaluation: {reason:?}")
        }
    }
}

fn run_authorizer(authorizer: &[u8], calldata: Vec<u8>) -> Result<ExecutionResult> {
    let db = EvmTxState::new(authorizer)?;
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

    let result = StaticAuthorizerHandler::<_, AuthorizerEvmError, _>::default()
        .run_system_call(&mut evm)
        .map_err(|err| anyhow!("Failed to execute authorizer: {err:?}"));
    let _ = evm.finalize();
    result
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
pub enum CallMessage<S: Spec> {
    Match(Match<S>),
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
