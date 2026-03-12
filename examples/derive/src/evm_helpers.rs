use std::marker::PhantomData;

use alloy_primitives::{Address as EvmAddress, Bytes, B256, U256};
use alloy_sol_types::{decode_revert_reason, SolError, SolValue};
use anyhow::{Result, bail};
use revm::context::result::{ExecutionResult, HaltReason};
use revm::context::{Cfg, ContextTr, JournalTr, LocalContextTr, Transaction};
use revm::handler::{execution, EvmTr, EvmTrError, FrameResult, FrameTr, Handler};
use revm::interpreter::{interpreter_action::FrameInit, FrameInput, SharedMemory};
use revm::state::{AccountInfo, Bytecode};
use revm::Database;
use revm_database_interface::DBErrorMarker;
use sov_modules_api::Spec;

use crate::{AUTHORIZER_ADDRESS, Match, MatchAuthorizerInput, MatchRejected, run_authorizer};

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
    pub fn new(code: &[u8]) -> Result<Self> {
        // Treat authorizers as plain runtime code, even if the bytes match the EIP-7702
        // delegation designator format.
        let code = Bytecode::new_legacy(Bytes::from(code.to_vec()));
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
pub(super) struct StaticAuthorizerHandler<CTX, ERROR, FRAME> {
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
            Some((
                account.code.clone().unwrap_or_default(),
                account.code_hash(),
            ))
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

pub(super) fn encode_match_authorizer_input<S: Spec>(match_msg: &Match<S>) -> Vec<u8> {
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

pub(super) fn decode_authorizer_revert(output: &[u8]) -> String {
    if let Ok(err) = MatchRejected::abi_decode(output) {
        return err.reason;
    }

    decode_revert_reason(output).unwrap_or_else(|| format!("0x{}", hex::encode(output)))
}

pub(super) fn execute_authorizer(authorizer: &[u8], calldata: Vec<u8>) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eip7702_designators_are_treated_as_legacy_runtime_code() {
        let eip7702_designator = [
            0xef, 0x01, 0x00, 0xde, 0xad, 0xbe, 0xef, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let state = EvmTxState::new(&eip7702_designator).unwrap();

        assert!(!state.code.is_eip7702());
    }
}
