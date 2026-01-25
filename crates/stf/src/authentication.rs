use borsh::{BorshDeserialize, BorshSerialize};
use sov_address::{EthereumAddress, FromVmAddress};
use sov_eip712_auth::{SchemaProvider, Secp256k1CryptoSpec};
use sov_modules_api::capabilities::{
    self, BatchFromUnregisteredSequencer, TransactionAuthenticator, UnregisteredAuthenticationError,
};
use sov_modules_api::runtime::capabilities::AuthenticationError;
use sov_modules_api::{
    DispatchCall, FullyBakedTx, GetGasPrice, ProvableStateReader, RawTx, Runtime, Spec,
};
use sov_state::User;
use std::marker::PhantomData;

/// See [`TransactionAuthenticator::Input`].
#[derive(std::fmt::Debug, Clone, BorshDeserialize, BorshSerialize)]
pub enum EvmAndEip712AuthenticatorInput<T = RawTx, U = RawTx> {
    /// Authenticate using the `EVM` authenticator, which expects a standard EVM transaction
    /// (i.e. an rlp-encoded payload signed using secp256k1 and hashed using keccak256).
    Evm(T),
    /// Authenticate using an EIP712 signature, which expects a transaction encoded the same way as
    /// a standard sov transaction but the signature generated according to the EIP712 spec.
    Eip712(U),
    /// Authenticate using the standard `sov-module` authenticator, which uses the default
    /// signature scheme and hashing algorithm defined in the rollup's [`Spec`].
    Standard(U),
}

/// EVM-compatible transaction authenticator. See [`TransactionAuthenticator`].
pub struct EvmAndEip712Authenticator<S, Rt, SP>(PhantomData<(S, Rt, SP)>);

impl<S, Rt, SP> TransactionAuthenticator<S> for EvmAndEip712Authenticator<S, Rt, SP>
where
    S: Spec<CryptoSpec: Secp256k1CryptoSpec>,
    S::Address: FromVmAddress<EthereumAddress>,
    Rt: Runtime<S> + DispatchCall<Spec = S>,
    SP: SchemaProvider,
{
    type Decodable =
        EvmAndEip712AuthenticatorInput<sov_evm::CallMessage<S>, <Rt as DispatchCall>::Decodable>;
    type Input = EvmAndEip712AuthenticatorInput;

    fn authenticate<Accessor>(
        tx: &FullyBakedTx,
        state: &mut Accessor,
    ) -> Result<
        capabilities::AuthenticationOutput<S, Self::Decodable>,
        capabilities::AuthenticationError,
    >
    where
        Accessor: ProvableStateReader<User, Spec = S>
            + GetGasPrice<Spec = S>
            + sov_modules_api::VersionReader,
    {
        let input: EvmAndEip712AuthenticatorInput = borsh::from_slice(&tx.data).map_err(|e| {
            sov_modules_api::capabilities::fatal_deserialization_error::<_, S, _>(
                &tx.data, e, state,
            )
        })?;

        match input {
            EvmAndEip712AuthenticatorInput::Evm(tx) => {
                let (tx_and_raw_hash, auth_data, runtime_call) =
                    sov_evm::authenticate::<_, _>(&tx.data, state)?;

                Ok((
                    tx_and_raw_hash,
                    auth_data,
                    EvmAndEip712AuthenticatorInput::Evm(runtime_call),
                ))
            }
            EvmAndEip712AuthenticatorInput::Eip712(tx) => {
                let (tx_and_raw_hash, auth_data, runtime_call) =
                    sov_eip712_auth::authenticate::<_, S, Rt, SP>(&tx.data, state)?;

                Ok((
                    tx_and_raw_hash,
                    auth_data,
                    EvmAndEip712AuthenticatorInput::Eip712(runtime_call),
                ))
            }
            EvmAndEip712AuthenticatorInput::Standard(tx) => {
                let (tx_and_raw_hash, auth_data, runtime_call) =
                    sov_modules_api::capabilities::authenticate::<_, S, Rt>(
                        &tx.data,
                        &Rt::CHAIN_HASH,
                        state,
                    )?;

                Ok((
                    tx_and_raw_hash,
                    auth_data,
                    EvmAndEip712AuthenticatorInput::Standard(runtime_call),
                ))
            }
        }
    }

    #[cfg(feature = "native")]
    fn compute_tx_hash(
        tx: &sov_modules_api::FullyBakedTx,
    ) -> anyhow::Result<sov_modules_api::TxHash> {
        let input: EvmAndEip712AuthenticatorInput = borsh::from_slice(&tx.data)?;

        match input {
            EvmAndEip712AuthenticatorInput::Evm(tx) => {
                let (_rlp, tx) = sov_evm::decode_evm_tx(&tx.data)?;
                Ok(sov_rollup_interface::TxHash::new(**tx.hash()))
            }
            EvmAndEip712AuthenticatorInput::Eip712(tx)
            | EvmAndEip712AuthenticatorInput::Standard(tx) => {
                Ok(capabilities::calculate_hash::<S>(&tx.data))
            }
        }
    }

    #[cfg(feature = "native")]
    fn decode_serialized_tx(
        tx: &FullyBakedTx,
    ) -> Result<Self::Decodable, sov_modules_api::capabilities::FatalError> {
        let auth_variant: EvmAndEip712AuthenticatorInput =
            borsh::from_slice(&tx.data).map_err(|e| {
                sov_modules_api::capabilities::FatalError::DeserializationFailed(e.to_string())
            })?;

        match &auth_variant {
            EvmAndEip712AuthenticatorInput::Evm(raw_tx) => {
                let (call, _tx) = sov_evm::decode_evm_tx(&raw_tx.data)?;
                Ok(EvmAndEip712AuthenticatorInput::Evm(
                    sov_evm::CallMessage::<S>::Call(call),
                ))
            }
            EvmAndEip712AuthenticatorInput::Standard(raw_tx) => {
                let call = capabilities::decode_sov_tx::<S, Rt>(&raw_tx.data)?;
                Ok(EvmAndEip712AuthenticatorInput::Standard(call))
            }
            EvmAndEip712AuthenticatorInput::Eip712(raw_tx) => {
                let call = sov_modules_api::capabilities::decode_sov_tx_with_cryptospec::<
                    S,
                    Rt,
                    <<S as Spec>::CryptoSpec as Secp256k1CryptoSpec>::CryptoSpec,
                >(&raw_tx.data)?;
                Ok(EvmAndEip712AuthenticatorInput::Eip712(call))
            }
        }
    }

    fn authenticate_unregistered<
        Accessor: ProvableStateReader<User, Spec = S> + sov_modules_api::VersionReader,
    >(
        batch: &BatchFromUnregisteredSequencer,
        state: &mut Accessor,
    ) -> Result<
        capabilities::AuthenticationOutput<S, Self::Decodable>,
        capabilities::UnregisteredAuthenticationError,
    > {
        let Self::Input::Standard(input) = borsh::from_slice(&batch.tx.data)
            .map_err(|_| UnregisteredAuthenticationError::InvalidAuthenticationDiscriminant)?
        else {
            return Err(UnregisteredAuthenticationError::InvalidAuthenticationDiscriminant);
        };

        let (tx_and_raw_hash, auth_data, runtime_call) =
            sov_modules_api::capabilities::authenticate::<_, S, Rt>(
                &input.data,
                &Rt::CHAIN_HASH,
                state,
            )
            .map_err(|e| match e {
                AuthenticationError::FatalError(err, hash) => {
                    UnregisteredAuthenticationError::FatalError(err, hash)
                }
                AuthenticationError::OutOfGas(err) => {
                    UnregisteredAuthenticationError::OutOfGas(err)
                }
            })?;

        Ok((
            tx_and_raw_hash,
            auth_data,
            EvmAndEip712AuthenticatorInput::Standard(runtime_call),
        ))
    }

    fn add_standard_auth(tx: RawTx) -> Self::Input {
        EvmAndEip712AuthenticatorInput::Standard(tx)
    }
}
