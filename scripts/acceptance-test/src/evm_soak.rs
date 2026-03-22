use crate::evm_contracts::StateConsistencyTester;
use crate::{Directories, API_ADDR};
use alloy::signers::local::PrivateKeySigner;
use alloy_primitives::{Address, Bytes, U256};
use alloy_rpc_types::TransactionRequest;
use alloy_sol_types::SolCall;
use anyhow::{anyhow, Context};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sov_eth_client::RpcClient;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

const STATE_CONSISTENCY_METADATA: &str = "state_consistency_contracts.json";
pub const NUM_PINNED_CONTRACTS: usize = 5;
pub const NUM_UNPINNED_CONTRACTS: usize = 5;

const DEPLOY_GAS_LIMIT: u64 = 5_000_000;
const UPDATE_GAS_LIMIT: u64 = 200_000;
const MAX_FEE_PER_GAS: u128 = 100;
const MAX_PRIORITY_FEE_PER_GAS: u128 = 1;

const STOP_HEIGHT_ERROR_MARKER: &str = "The preferred sequencer has reached the stop height";

const PRIVILEGED_DEPLOYER_KEY: &str =
    "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const UNPRIVILEGED_DEPLOYER_OFFSET: u8 = 1;
const PINNED_WORKER_KEY_OFFSET: u8 = 10;
const UNPINNED_WORKER_KEY_OFFSET: u8 = 60;
const DEFAULT_BUCKET_SIZE_LIMIT: usize = 100 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
struct StateConsistencyMetadata {
    pinned_addresses: Vec<String>,
    unpinned_addresses: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EvmPinnedCacheConfig {
    #[serde(default)]
    preferred_sequencer_publish_reverted_txs: bool,
    #[serde(default = "default_bucket_size_limit")]
    default_bucket_size_limit: usize,
    #[serde(default)]
    privileged_deployer_addresses: Vec<Address>,
    #[serde(default)]
    known_contracts_and_limits: std::collections::BTreeMap<Address, usize>,
}

impl Default for EvmPinnedCacheConfig {
    fn default() -> Self {
        Self {
            preferred_sequencer_publish_reverted_txs: false,
            default_bucket_size_limit: default_bucket_size_limit(),
            privileged_deployer_addresses: Vec::new(),
            known_contracts_and_limits: std::collections::BTreeMap::new(),
        }
    }
}

fn evm_rpc_addr() -> SocketAddr {
    API_ADDR.parse().expect("Invalid API_ADDR")
}

fn evm_artifacts_dir(directories: &Directories) -> PathBuf {
    directories.output_dir.join("evm")
}

fn evm_pinned_cache_path(directories: &Directories) -> PathBuf {
    directories.output_dir.join("evm_pinned_cache.json")
}

fn state_consistency_metadata_path(directories: &Directories) -> PathBuf {
    evm_artifacts_dir(directories).join(STATE_CONSISTENCY_METADATA)
}

fn default_bucket_size_limit() -> usize {
    DEFAULT_BUCKET_SIZE_LIMIT
}

fn derive_worker_key(root_key: &str, worker_idx: u8) -> anyhow::Result<String> {
    let mut key_bytes: [u8; 32] = hex::decode(root_key)?
        .try_into()
        .map_err(|_| anyhow!("Invalid private key length"))?;
    key_bytes[0] = key_bytes[0].wrapping_add(worker_idx);
    Ok(hex::encode(key_bytes))
}

fn privileged_deployer_address() -> anyhow::Result<Address> {
    let signer: PrivateKeySigner = PRIVILEGED_DEPLOYER_KEY.parse()?;
    Ok(signer.address())
}

pub fn privileged_deployer_key() -> &'static str {
    PRIVILEGED_DEPLOYER_KEY
}

pub fn unprivileged_deployer_key() -> anyhow::Result<String> {
    derive_worker_key(PRIVILEGED_DEPLOYER_KEY, UNPRIVILEGED_DEPLOYER_OFFSET)
}

fn worker_key_for_index(offset: u8, idx: usize) -> anyhow::Result<String> {
    let idx_u8 = u8::try_from(idx).map_err(|_| anyhow!("worker index {idx} exceeds u8 range"))?;
    derive_worker_key(PRIVILEGED_DEPLOYER_KEY, offset.wrapping_add(idx_u8))
}

pub fn pinned_worker_key(idx: usize) -> anyhow::Result<String> {
    worker_key_for_index(PINNED_WORKER_KEY_OFFSET, idx)
}

pub fn unpinned_worker_key(idx: usize) -> anyhow::Result<String> {
    worker_key_for_index(UNPINNED_WORKER_KEY_OFFSET, idx)
}

fn tx_request(
    from: Address,
    nonce: u64,
    to: Option<Address>,
    data: Bytes,
    gas_limit: u64,
) -> TransactionRequest {
    let mut tx = TransactionRequest::default()
        .from(from)
        .nonce(nonce)
        .max_priority_fee_per_gas(MAX_PRIORITY_FEE_PER_GAS)
        .max_fee_per_gas(MAX_FEE_PER_GAS)
        .gas_limit(gas_limit)
        .input(data.into());

    if let Some(addr) = to {
        tx = tx.to(addr);
    }

    tx
}

fn call_request(from: Address, to: Address, data: Bytes) -> TransactionRequest {
    TransactionRequest::default()
        .from(from)
        .to(to)
        .input(data.into())
}

fn is_stop_height_error_message(message: &str) -> bool {
    message.contains(STOP_HEIGHT_ERROR_MARKER) || message.contains("stop height")
}

fn encode_update_call(old_value: U256, new_value: U256) -> Bytes {
    let call = StateConsistencyTester::updateCall {
        oldValue: old_value,
        newValue: new_value,
    };
    Bytes::from(call.abi_encode())
}

fn encode_value_call() -> Bytes {
    let call = StateConsistencyTester::valueCall {};
    Bytes::from(call.abi_encode())
}

fn decode_value_output(raw: Bytes) -> anyhow::Result<U256> {
    if raw.len() < 32 {
        return Err(anyhow!("Value call returned {} bytes", raw.len()));
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&raw[raw.len() - 32..]);
    Ok(U256::from_be_bytes(buf))
}

fn state_consistency_bytecode() -> Bytes {
    Bytes::from(StateConsistencyTester::BYTECODE.to_vec())
}

fn write_state_consistency_metadata(
    directories: &Directories,
    pinned_addresses: &[Address],
    unpinned_addresses: &[Address],
) -> anyhow::Result<()> {
    let metadata = StateConsistencyMetadata {
        pinned_addresses: pinned_addresses
            .iter()
            .map(|address| format!("{:#x}", address))
            .collect(),
        unpinned_addresses: unpinned_addresses
            .iter()
            .map(|address| format!("{:#x}", address))
            .collect(),
    };
    let metadata_path = state_consistency_metadata_path(directories);
    fs::create_dir_all(
        metadata_path
            .parent()
            .ok_or_else(|| anyhow!("Invalid metadata path"))?,
    )?;
    fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?)?;
    Ok(())
}

pub struct StateConsistencyContracts {
    pub pinned: Vec<Address>,
    pub unpinned: Vec<Address>,
}

pub fn load_state_consistency_contracts(
    directories: &Directories,
) -> anyhow::Result<StateConsistencyContracts> {
    let metadata_path = state_consistency_metadata_path(directories);
    let raw = fs::read_to_string(&metadata_path)
        .with_context(|| format!("Missing {}", metadata_path.display()))?;
    let metadata: StateConsistencyMetadata = serde_json::from_str(&raw)?;
    let pinned = metadata
        .pinned_addresses
        .iter()
        .map(|address| {
            address
                .parse::<Address>()
                .context("Failed to parse pinned EVM contract address")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let unpinned = metadata
        .unpinned_addresses
        .iter()
        .map(|address| {
            address
                .parse::<Address>()
                .context("Failed to parse unpinned EVM contract address")
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(StateConsistencyContracts { pinned, unpinned })
}

pub fn ensure_evm_pinned_cache_config(directories: &Directories) -> anyhow::Result<()> {
    let config_path = evm_pinned_cache_path(directories);
    let mut config = if config_path.exists() {
        let raw = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;
        serde_json::from_str::<EvmPinnedCacheConfig>(&raw)?
    } else {
        EvmPinnedCacheConfig::default()
    };

    let privileged_address = privileged_deployer_address()?;
    if !config
        .privileged_deployer_addresses
        .iter()
        .any(|addr| addr == &privileged_address)
    {
        config
            .privileged_deployer_addresses
            .push(privileged_address);
    }

    fs::write(config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

async fn deploy_state_consistency_contract(
    bytecode: Bytes,
    rpc: &RpcClient,
    nonce: u64,
) -> anyhow::Result<Address> {
    let tx = tx_request(rpc.address(), nonce, None, bytecode, DEPLOY_GAS_LIMIT);
    let tx_hash = rpc
        .eth_send_transaction(tx)
        .await
        .map_err(|err| anyhow!(err.to_string()))?;

    let timeout = Duration::from_secs(60);
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(anyhow!(
                "Timed out waiting for EVM contract deployment receipt"
            ));
        }
        if let Some(receipt) = rpc.receipt(tx_hash).await {
            let status = receipt.status();
            if !status {
                return Err(anyhow!(
                    "EVM contract deployment failed (status {status}) for tx {:#x}",
                    tx_hash
                ));
            }
            if let Some(addr) = receipt.contract_address {
                return Ok(addr);
            }
            return Err(anyhow!(
                "EVM contract deployment succeeded but no contract_address was returned for tx {:#x}",
                tx_hash
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn setup_state_consistency_contracts(
    directories: &Directories,
) -> anyhow::Result<StateConsistencyContracts> {
    let bytecode = state_consistency_bytecode();
    let rpc_addr = evm_rpc_addr();
    let privileged_rpc = RpcClient::new(PRIVILEGED_DEPLOYER_KEY, rpc_addr).await;
    let unprivileged_key = unprivileged_deployer_key()?;
    let unprivileged_rpc = RpcClient::new(&unprivileged_key, rpc_addr).await;

    let mut pinned_addresses = Vec::with_capacity(NUM_PINNED_CONTRACTS);
    let mut privileged_nonce = privileged_rpc
        .eth_get_transaction_count(privileged_rpc.address())
        .await;
    for _ in 0..NUM_PINNED_CONTRACTS {
        let address =
            deploy_state_consistency_contract(bytecode.clone(), &privileged_rpc, privileged_nonce)
                .await?;
        privileged_nonce = privileged_nonce.saturating_add(1);
        pinned_addresses.push(address);
    }

    let mut unpinned_addresses = Vec::with_capacity(NUM_UNPINNED_CONTRACTS);
    let mut unprivileged_nonce = unprivileged_rpc
        .eth_get_transaction_count(unprivileged_rpc.address())
        .await;
    for _ in 0..NUM_UNPINNED_CONTRACTS {
        let address = deploy_state_consistency_contract(
            bytecode.clone(),
            &unprivileged_rpc,
            unprivileged_nonce,
        )
        .await?;
        unprivileged_nonce = unprivileged_nonce.saturating_add(1);
        unpinned_addresses.push(address);
    }

    write_state_consistency_metadata(directories, &pinned_addresses, &unpinned_addresses)?;

    Ok(StateConsistencyContracts {
        pinned: pinned_addresses,
        unpinned: unpinned_addresses,
    })
}

pub async fn evm_state_consistency_worker(
    contract_address: Address,
    signer_key: String,
    label: &'static str,
) -> anyhow::Result<()> {
    let rpc = RpcClient::new(&signer_key, evm_rpc_addr()).await;
    let from = rpc.address();
    let mut nonce = rpc.eth_get_transaction_count(from).await;

    let mut expected_value = decode_value_output(
        rpc.eth_call(call_request(from, contract_address, encode_value_call()))
            .await
            .map_err(|err| anyhow!(err.to_string()))?,
    )?;

    tracing::info!(
        expected_value = ?expected_value,
        contract_address = %contract_address,
        label,
        "EVM state consistency worker started"
    );

    loop {
        let (tx_count, sleep_ms) = {
            let mut rng = rand::thread_rng();
            let sleep_ms = rng.gen_range(25..100);
            let tx_count = rng.gen_range(3..12);
            (tx_count, sleep_ms)
        };

        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;

        for _ in 0..tx_count {
            let new_value = expected_value + U256::from(1);
            let data = encode_update_call(expected_value, new_value);
            let tx = tx_request(from, nonce, Some(contract_address), data, UPDATE_GAS_LIMIT);

            match rpc.eth_send_transaction(tx).await {
                Ok(_) => {
                    nonce = nonce.saturating_add(1);
                    expected_value = new_value;
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    if is_stop_height_error_message(&err_msg) {
                        tracing::info!("EVM worker detected sequencer stop height, shutting down");
                        return Ok(());
                    }
                    return Err(anyhow!(err_msg));
                }
            }
        }
    }
}
