use crate::{Directories, API_URL};
use alloy::signers::local::PrivateKeySigner;
use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
use alloy_eips::Encodable2718;
use alloy_primitives::{Address, Bytes, TxHash, TxKind, U256};
use alloy_signer::SignerSync;
use anyhow::{anyhow, Context};
use rand::Rng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tokio::sync::watch;

const STATE_CONSISTENCY_CONTRACT: &str = "StateConsistencyTester";
const STATE_CONSISTENCY_CONTRACT_PATH: &str = "evm-contracts/StateConsistencyTester.sol";
const STATE_CONSISTENCY_BIN: &str = "StateConsistencyTester.bin";
const STATE_CONSISTENCY_ABI: &str = "StateConsistencyTester.abi.json";
const STATE_CONSISTENCY_METADATA: &str = "state_consistency_contract.json";

const UPDATE_SELECTOR: [u8; 4] = [0x2f, 0xb5, 0x65, 0xe8];
const VALUE_SELECTOR: [u8; 4] = [0x3f, 0xa4, 0xf2, 0x45];

const DEPLOY_GAS_LIMIT: u64 = 5_000_000;
const UPDATE_GAS_LIMIT: u64 = 200_000;
const MAX_FEE_PER_GAS: u128 = 100;
const MAX_PRIORITY_FEE_PER_GAS: u128 = 1;

const STOP_HEIGHT_ERROR_MARKER: &str = "The preferred sequencer has reached the stop height";

const DEFAULT_PRIVATE_KEY: &str =
    "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

#[derive(Debug, Serialize, Deserialize)]
struct StateConsistencyMetadata {
    address: String,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[allow(dead_code)]
    data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct TransactionReceipt {
    #[serde(rename = "contractAddress")]
    contract_address: Option<String>,
}

#[derive(Clone)]
struct EvmRpcClient {
    http: reqwest::Client,
    url: String,
}

impl EvmRpcClient {
    fn new(url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            url,
        }
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<T> {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let response = self.http.post(&self.url).json(&payload).send().await?;
        let status = response.status();
        let body: JsonRpcResponse<T> = response.json().await?;

        if !status.is_success() {
            return Err(anyhow!("EVM RPC HTTP error {} for {}", status, method));
        }
        if let Some(error) = body.error {
            let data_suffix = format_rpc_error_data(&error.data);
            return Err(anyhow!(
                "EVM RPC error {} (code {}): {}{}",
                method,
                error.code,
                error.message,
                data_suffix
            ));
        }

        body.result
            .ok_or_else(|| anyhow!("EVM RPC {} missing result", method))
    }

    async fn eth_chain_id(&self) -> anyhow::Result<u64> {
        let raw: String = self.call("eth_chainId", serde_json::json!([])).await?;
        parse_hex_u64(&raw)
    }

    async fn eth_get_transaction_count(&self, address: Address, tag: &str) -> anyhow::Result<u64> {
        let addr = format!("{:#x}", address);
        let raw: String = self
            .call("eth_getTransactionCount", serde_json::json!([addr, tag]))
            .await?;
        parse_hex_u64(&raw)
    }

    async fn eth_send_raw_transaction(&self, raw: Bytes) -> anyhow::Result<TxHash> {
        let raw_hex = format!("0x{}", hex::encode(raw.as_ref()));
        let hash: String = self
            .call("eth_sendRawTransaction", serde_json::json!([raw_hex]))
            .await?;
        hash.parse::<TxHash>()
            .context("Failed to parse eth_sendRawTransaction hash")
    }

    async fn eth_get_transaction_receipt(
        &self,
        hash: TxHash,
    ) -> anyhow::Result<Option<TransactionReceipt>> {
        let hash_hex = format!("{:#x}", hash);
        self.call(
            "eth_getTransactionReceipt",
            serde_json::json!([hash_hex]),
        )
        .await
    }

    async fn eth_call(&self, to: Address, data: Bytes) -> anyhow::Result<Bytes> {
        let raw: String = self
            .call(
                "eth_call",
                serde_json::json!([
                    {
                        "to": format!("{:#x}", to),
                        "data": format!("0x{}", hex::encode(data.as_ref())),
                    },
                    "latest"
                ]),
            )
            .await?;
        parse_hex_bytes(&raw)
    }
}

fn evm_rpc_url() -> String {
    format!("{}/rpc", API_URL)
}

fn evm_artifacts_dir(directories: &Directories) -> PathBuf {
    directories.output_dir.join("evm")
}

fn state_consistency_bin_path(directories: &Directories) -> PathBuf {
    evm_artifacts_dir(directories).join(STATE_CONSISTENCY_BIN)
}

fn state_consistency_abi_path(directories: &Directories) -> PathBuf {
    evm_artifacts_dir(directories).join(STATE_CONSISTENCY_ABI)
}

fn state_consistency_metadata_path(directories: &Directories) -> PathBuf {
    evm_artifacts_dir(directories).join(STATE_CONSISTENCY_METADATA)
}

fn parse_hex_u64(value: &str) -> anyhow::Result<u64> {
    let trimmed = value.trim_start_matches("0x");
    if trimmed.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(trimmed, 16)
        .map_err(|e| anyhow!("Failed to parse hex u64 {}: {}", value, e))
}

fn parse_hex_bytes(value: &str) -> anyhow::Result<Bytes> {
    let trimmed = value.trim_start_matches("0x");
    let bytes = hex::decode(trimmed)?;
    Ok(Bytes::from(bytes))
}

fn extract_rpc_error_message(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(message) => Some(message.clone()),
        serde_json::Value::Object(map) => {
            for key in ["message", "error", "data", "details"] {
                if let Some(entry) = map.get(key) {
                    if let Some(message) = extract_rpc_error_message(entry) {
                        return Some(message);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn format_rpc_error_data(data: &Option<serde_json::Value>) -> String {
    let Some(value) = data else {
        return String::new();
    };

    if let Some(message) = extract_rpc_error_message(value) {
        format!("; data: {message}")
    } else {
        format!("; data: {value}")
    }
}

fn is_stop_height_error_message(message: &str) -> bool {
    message.contains(STOP_HEIGHT_ERROR_MARKER) || message.contains("stop height")
}

fn encode_update_call(old_value: U256, new_value: U256) -> Bytes {
    let mut data = Vec::with_capacity(4 + 32 + 32);
    data.extend_from_slice(&UPDATE_SELECTOR);
    data.extend_from_slice(&old_value.to_be_bytes::<32>());
    data.extend_from_slice(&new_value.to_be_bytes::<32>());
    Bytes::from(data)
}

fn encode_value_call() -> Bytes {
    Bytes::from(VALUE_SELECTOR.to_vec())
}

fn decode_value_output(raw: Bytes) -> anyhow::Result<U256> {
    if raw.len() < 32 {
        return Err(anyhow!("Value call returned {} bytes", raw.len()));
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&raw[raw.len() - 32..]);
    Ok(U256::from_be_bytes(buf))
}

fn sign_eip1559_tx(
    signer: &PrivateKeySigner,
    chain_id: u64,
    nonce: u64,
    to: TxKind,
    data: Bytes,
    gas_limit: u64,
) -> anyhow::Result<Bytes> {
    let tx = TxEip1559 {
        chain_id,
        nonce,
        gas_limit,
        max_fee_per_gas: MAX_FEE_PER_GAS,
        max_priority_fee_per_gas: MAX_PRIORITY_FEE_PER_GAS,
        to,
        value: U256::ZERO,
        input: data.into(),
        access_list: Default::default(),
    };
    let sig = signer
        .sign_hash_sync(&tx.signature_hash())
        .context("Failed to sign EVM transaction")?;
    let signed = tx.into_signed(sig);
    let envelope = TxEnvelope::Eip1559(signed);
    Ok(envelope.encoded_2718().into())
}

fn load_state_consistency_bytecode(directories: &Directories) -> anyhow::Result<Bytes> {
    let bin_path = state_consistency_bin_path(directories);
    let hex_str = fs::read_to_string(&bin_path)
        .with_context(|| format!("Failed to read {}", bin_path.display()))?;
    let trimmed = hex_str.trim().trim_start_matches("0x");
    let bytes = hex::decode(trimmed)
        .with_context(|| format!("Failed to decode bytecode from {}", bin_path.display()))?;
    Ok(Bytes::from(bytes))
}

fn write_state_consistency_metadata(
    directories: &Directories,
    address: Address,
) -> anyhow::Result<()> {
    let metadata = StateConsistencyMetadata {
        address: format!("{:#x}", address),
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

pub fn load_state_consistency_contract_address(
    directories: &Directories,
) -> anyhow::Result<Address> {
    let metadata_path = state_consistency_metadata_path(directories);
    let raw = fs::read_to_string(&metadata_path)
        .with_context(|| format!("Missing {}", metadata_path.display()))?;
    let metadata: StateConsistencyMetadata = serde_json::from_str(&raw)?;
    metadata
        .address
        .parse::<Address>()
        .context("Failed to parse EVM contract address")
}

pub fn compile_state_consistency_contract(directories: &Directories) -> anyhow::Result<()> {
    let contract_path = Path::new(STATE_CONSISTENCY_CONTRACT_PATH);
    let output = Command::new("solc")
        .args(["--combined-json", "abi,bin", contract_path.to_str().unwrap()])
        .current_dir(&directories.acceptance_test_dir)
        .output()
        .context("Failed to run solc for EVM contract compilation")?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "solc failed (exit {:?}). stdout: {} stderr: {}",
            output.status.code(),
            stdout,
            stderr
        ));
    }

    let combined: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("Failed to parse solc combined-json output")?;
    let contracts = combined
        .get("contracts")
        .and_then(|contracts| contracts.as_object())
        .ok_or_else(|| anyhow!("solc output missing contracts map"))?;
    let contract_key = format!(
        "{}:{}",
        contract_path.display(),
        STATE_CONSISTENCY_CONTRACT
    );
    let contract = contracts.get(&contract_key).or_else(|| {
        let suffix = format!(":{}", STATE_CONSISTENCY_CONTRACT);
        contracts
            .iter()
            .find(|(key, _)| key.ends_with(&suffix))
            .map(|(_, value)| value)
    });
    let contract = contract.ok_or_else(|| {
        anyhow!(
            "solc output missing {}. Available keys: {}",
            contract_key,
            contracts.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })?;
    let bin = contract
        .get("bin")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("solc output missing bin for {}", contract_key))?;
    let abi_value = contract
        .get("abi")
        .ok_or_else(|| anyhow!("solc output missing abi for {}", contract_key))?;
    let abi = if let Some(value) = abi_value.as_str() {
        value.to_string()
    } else {
        serde_json::to_string(abi_value)?
    };

    let artifacts_dir = evm_artifacts_dir(directories);
    fs::create_dir_all(&artifacts_dir)?;
    fs::write(state_consistency_bin_path(directories), bin)?;
    fs::write(state_consistency_abi_path(directories), abi)?;
    Ok(())
}

async fn deploy_state_consistency_contract(
    bytecode: Bytes,
    rpc: &EvmRpcClient,
    signer: &PrivateKeySigner,
    chain_id: u64,
) -> anyhow::Result<Address> {
    let nonce = rpc
        .eth_get_transaction_count(signer.address(), "pending")
        .await?;
    let raw = sign_eip1559_tx(
        signer,
        chain_id,
        nonce,
        TxKind::Create,
        bytecode,
        DEPLOY_GAS_LIMIT,
    )?;
    let tx_hash = rpc.eth_send_raw_transaction(raw).await?;

    let timeout = Duration::from_secs(60);
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(anyhow!(
                "Timed out waiting for EVM contract deployment receipt"
            ));
        }
        if let Some(receipt) = rpc.eth_get_transaction_receipt(tx_hash).await? {
            if let Some(addr) = receipt.contract_address {
                return addr
                    .parse::<Address>()
                    .context("Failed to parse deployed contract address");
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn setup_state_consistency_contract(
    directories: &Directories,
) -> anyhow::Result<Address> {
    compile_state_consistency_contract(directories)?;
    let bytecode = load_state_consistency_bytecode(directories)?;
    let rpc = EvmRpcClient::new(evm_rpc_url());
    let signer: PrivateKeySigner = DEFAULT_PRIVATE_KEY.parse()?;
    let chain_id = rpc.eth_chain_id().await?;
    let address = deploy_state_consistency_contract(bytecode, &rpc, &signer, chain_id).await?;
    write_state_consistency_metadata(directories, address)?;
    Ok(address)
}

pub async fn evm_state_consistency_worker(
    contract_address: Address,
    _rollup_stop_height: u64,
    rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let rpc = EvmRpcClient::new(evm_rpc_url());
    let signer: PrivateKeySigner = DEFAULT_PRIVATE_KEY.parse()?;
    let chain_id = rpc.eth_chain_id().await?;
    let mut nonce = rpc
        .eth_get_transaction_count(signer.address(), "pending")
        .await?;

    let mut expected_value = decode_value_output(
        rpc.eth_call(contract_address, encode_value_call()).await?,
    )?;

    tracing::info!(
        expected_value = ?expected_value,
        contract_address = %contract_address,
        "EVM state consistency worker started"
    );

    while !*rx.borrow() {
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
            let raw = sign_eip1559_tx(
                &signer,
                chain_id,
                nonce,
                TxKind::Call(contract_address),
                data,
                UPDATE_GAS_LIMIT,
            )?;

            match rpc.eth_send_raw_transaction(raw).await {
                Ok(_) => {
                    nonce = nonce.saturating_add(1);
                    expected_value = new_value;
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    if is_stop_height_error_message(&err_msg) {
                        tracing::info!(
                            "EVM worker detected sequencer stop height, shutting down"
                        );
                        return Ok(());
                    }
                    return Err(err);
                }
            }
        }
    }

    tracing::info!("EVM state consistency worker shutting down");
    Ok(())
}
