#![allow(unused_imports)]
use anyhow::Result;
use schemars::JsonSchema;
use sov_modules_api::macros::{serialize, UniversalWallet};
use sov_modules_api::{
    Context, EventEmitter, GenesisState, Module, ModuleId, ModuleInfo, ModuleRestApi, Spec,
    StateValue, TxState,
};
use std::marker::PhantomData;

/// A new module:
/// - Must derive `ModuleInfo`
/// - Must contain `[id]` field
/// - Can contain any number of ` #[state]` or `[module]` fields
/// - Can derive ModuleRestApi to automatically generate Rest API endpoints
#[derive(Clone, ModuleInfo, ModuleRestApi)]
pub struct ValueSetter<S: Spec> {
    /// Id of the module.
    #[id]
    pub id: ModuleId,

    /// Some value kept in the state.
    #[state]
    pub value: StateValue<u32>,

    /// You can disregard this, as its only used to satisfy
    /// the compiler for the type parameter `S` not being used.
    #[phantom]
    pub phantom: PhantomData<S>,
}

impl<S: Spec> Module for ValueSetter<S> {
    type Spec = S;

    type Config = ();

    type CallMessage = CallMessage;

    type Event = ();

    type Error = anyhow::Error;

    fn call(
        &mut self,
        msg: Self::CallMessage,
        _context: &Context<Self::Spec>,
        state: &mut impl TxState<S>,
    ) -> Result<()> {
        match msg {
            CallMessage::SetValue(new_value) => {
                self.value.set(&new_value, state)?;

                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, UniversalWallet)]
#[serialize(Borsh, Serde)]
#[serde(rename_all = "snake_case")]
pub enum CallMessage {
    SetValue(u32),
}
