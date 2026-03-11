#![allow(unused_imports)]
use anyhow::Result;
use revm::state::{AccountInfo, Bytecode};
use borsh::{BorshDeserialize, BorshSerialize};
use revm::Database;
use revm_database_interface::DBErrorMarker;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sov_modules_api::macros::{serialize, UniversalWallet};
use sov_modules_api::{
    Context, EventEmitter, GenesisState, Module, ModuleId, ModuleInfo, ModuleRestApi, Spec, StateMap,
    StateValue, TxState,
};
use sov_state::BcsCodec;
use std::marker::PhantomData;
use alloy_primitives::{Address, B256, U256};

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
        context: &Context<Self::Spec>,
        state: &mut impl TxState<S>,
    ) -> Result<()> {
        match msg {
            CallMessage::Match(match_msg) => {
                let evm = todo!();

                Ok(())
            },
            CallMessage::SetAuthorizer(authorizer) => {
                self.authorizers.set(context.sender(), &authorizer, state)?;
                self.emit_event(state, Event::AuthorizerSet(*context.sender(), hex::encode(authorizer)));
                Ok(())
            }
        }
    }
}


pub struct EvmTxState {
    code: Vec<u8>
}

#[derive(thiserror::Error, Debug)]
pub enum EvmTxStateError {
    #[error("Code not found")]
    CodeNotFound,
    #[error("Block hash not found")]
    BlockHashNotFound,
}
impl DBErrorMarker for EvmTxStateError {}


impl Database for EvmTxState {
    type Error = EvmTxStateError;

    fn basic(&mut self,_address:Address) -> Result<Option<AccountInfo> ,Self::Error>  {
        Ok(None)
    }

    fn code_by_hash(&mut self,_code_hash:B256) -> Result<Bytecode,Self::Error>  {
        Err(EvmTxStateError::CodeNotFound)
    }

    fn storage(&mut self,_address:Address,_index:U256) -> Result<U256,Self::Error>  {
        Ok(U256::ZERO)
    }

    fn block_hash(&mut self,_number:u64) -> Result<B256,Self::Error>  {
        Err(EvmTxStateError::BlockHashNotFound)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, UniversalWallet)]
#[derive(Deserialize, Serialize, BorshDeserialize, BorshSerialize)]
#[schemars(rename = "call_message")]
#[schemars(bound = "S: Spec")]
#[serde(bound = "S: Spec")]
pub enum CallMessage<S: Spec> {
    Match(Match<S>),
    SetAuthorizer(Vec<u8>),
}


#[derive(Debug, Clone, PartialEq, Eq, JsonSchema)]
#[derive(Deserialize, Serialize, BorshDeserialize, BorshSerialize)]
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
    pub price: u64,          // fixed-point (e.g., price * 1e6)                                                                                                     
    pub quantity: u64,       // fixed-point
    pub long_account: S::Address,  // buyer
    pub short_account: S::Address, // seller
    pub timestamp: u64,
    pub long_account_calldata: Vec<u8>,
    pub short_account_calldata: Vec<u8>,
}
