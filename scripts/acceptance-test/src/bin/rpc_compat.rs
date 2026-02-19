use acceptance_test::evm_contracts::StateConsistencyTester;
use alloy_primitives::{Address, Bytes, B256, U256};
use alloy_rpc_types::TransactionRequest;
use alloy_sol_types::SolCall;
use anyhow::{anyhow, bail, Context};
use clap::Parser;
use reqwest::Url;
use serde_json::{json, Value};
use sov_eth_client::RpcClient;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const DEFAULT_RPC_URL: &str = "http://127.0.0.1:12346/rpc";
const DEFAULT_PRIVATE_KEY: &str =
    "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const DEPLOY_GAS_LIMIT: u64 = 5_000_000;
const UPDATE_GAS_LIMIT: u64 = 250_000;
const MAX_FEE_PER_GAS: u128 = 100;
const MAX_PRIORITY_FEE_PER_GAS: u128 = 1;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = DEFAULT_RPC_URL)]
    /// Full Ethereum JSON-RPC endpoint (for example: http://127.0.0.1:12346/rpc).
    rpc_url: String,

    #[arg(long, default_value = DEFAULT_PRIVATE_KEY)]
    /// EVM private key used for signed transactions.
    private_key: String,

    #[arg(long, default_value_t = 45)]
    /// Maximum time to wait for each transaction receipt.
    receipt_wait_secs: u64,

    #[arg(long, default_value_t = 7)]
    /// New value used when calling StateConsistencyTester.update(old, new).
    new_value: u64,
}

struct JsonRpcClient {
    client: reqwest::Client,
    url: String,
    next_id: AtomicU64,
}

impl JsonRpcClient {
    fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
            next_id: AtomicU64::new(1),
        }
    }

    async fn call(&self, method: &str, params: Vec<Value>) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let response: Value = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("failed HTTP request for method {method}"))?
            .json()
            .await
            .with_context(|| format!("invalid JSON response for method {method}"))?;

        if let Some(error) = response.get("error") {
            bail!("{method} returned error: {error}");
        }

        response
            .get("result")
            .cloned()
            .context("missing result field in JSON-RPC response")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    run(args).await
}

async fn run(args: Args) -> anyhow::Result<()> {
    let socket_addr = socket_addr_from_rpc_url(&args.rpc_url)?;
    let signed_rpc = RpcClient::new(&args.private_key, socket_addr).await;
    let rpc = JsonRpcClient::new(args.rpc_url.clone());
    let receipt_timeout = Duration::from_secs(args.receipt_wait_secs);

    println!("RPC compatibility suite");
    println!("Endpoint: {}", args.rpc_url);

    let from = signed_rpc.address();
    let from_hex = format!("{:#x}", from);

    check_hex_quantity(&rpc.call("eth_chainId", vec![]).await?, "eth_chainId")?;
    check_hex_quantity(
        &rpc.call("eth_blockNumber", vec![]).await?,
        "eth_blockNumber",
    )?;
    check_hex_quantity(&rpc.call("eth_gasPrice", vec![]).await?, "eth_gasPrice")?;
    println!("PASS: core quantity endpoints return decode-safe hex quantities");

    let fee_history = rpc
        .call(
            "eth_feeHistory",
            vec![json!("0x2"), json!("latest"), json!(Vec::<String>::new())],
        )
        .await?;
    validate_fee_history_shape(&fee_history, 2)?;
    println!("PASS: eth_feeHistory shape and quantity arrays are coherent");

    let latest_before = get_tx_count(&rpc, &from_hex, "latest").await?;
    let pending_before = get_tx_count(&rpc, &from_hex, "pending").await?;
    if pending_before < latest_before {
        bail!(
            "nonce ordering invalid before test txs: pending ({pending_before}) < latest ({latest_before})"
        );
    }
    println!("PASS: nonce ordering pending >= latest before submissions");

    for tag in ["latest", "pending", "earliest", "safe", "finalized"] {
        let block = rpc
            .call("eth_getBlockByNumber", vec![json!(tag), json!(false)])
            .await?;
        if !block.is_null() {
            let block_number = get_required_string_field(&block, "number")?;
            check_hex_quantity_str(block_number, &format!("eth_getBlockByNumber({tag}).number"))?;
        }
        let _ = get_tx_count(&rpc, &from_hex, tag).await?;
    }
    println!("PASS: block-tag matrix callable for block + nonce endpoints");

    let deploy_nonce = signed_rpc.eth_get_transaction_count(from).await;
    let deploy_tx = tx_request(
        from,
        deploy_nonce,
        None,
        Bytes::from(StateConsistencyTester::BYTECODE.to_vec()),
        DEPLOY_GAS_LIMIT,
    );
    let deploy_hash = signed_rpc
        .eth_send_transaction(deploy_tx)
        .await
        .map_err(|e| anyhow!("eth_send_transaction deploy failed: {e}"))?;
    let deploy_hash_hex = format!("{:#x}", deploy_hash);

    let deploy_receipt =
        wait_for_receipt(&rpc, &deploy_hash_hex, receipt_timeout, "deploy receipt").await?;
    validate_receipt_shape(&deploy_receipt)?;
    let contract_address = parse_contract_address(&deploy_receipt)?;
    let contract_hex = format!("{:#x}", contract_address);
    println!("PASS: contract deployment tx mined with decode-safe receipt shape");

    let value_call = StateConsistencyTester::valueCall {};
    let value_before = eth_call_u256(
        &rpc,
        &from_hex,
        &contract_hex,
        &hex_data(&value_call.abi_encode()),
    )
    .await?;
    if value_before != U256::ZERO {
        bail!("unexpected initial contract value: expected 0, got {value_before}");
    }

    let update_call = StateConsistencyTester::updateCall {
        oldValue: value_before,
        newValue: U256::from(args.new_value),
    };
    let update_data = hex_data(&update_call.abi_encode());
    let estimate_gas = eth_estimate_gas(&rpc, &from_hex, &contract_hex, &update_data).await?;
    let update_call_result = rpc
        .call(
            "eth_call",
            vec![
                json!({
                    "from": from_hex,
                    "to": contract_hex,
                    "data": update_data,
                }),
                json!("latest"),
            ],
        )
        .await?;
    let call_data = update_call_result
        .as_str()
        .context("eth_call update result must be a hex string")?;
    check_hex_data_str(call_data, "eth_call(update)")?;

    let update_nonce = signed_rpc.eth_get_transaction_count(from).await;
    let update_tx = tx_request(
        from,
        update_nonce,
        Some(contract_address),
        Bytes::from(update_call.abi_encode()),
        UPDATE_GAS_LIMIT,
    );
    let update_hash = signed_rpc
        .eth_send_transaction(update_tx)
        .await
        .map_err(|e| anyhow!("eth_send_transaction update failed: {e}"))?;
    let update_hash_hex = format!("{:#x}", update_hash);

    let update_receipt =
        wait_for_receipt(&rpc, &update_hash_hex, receipt_timeout, "update receipt").await?;
    validate_receipt_shape(&update_receipt)?;
    assert_receipt_status_success(&update_receipt)?;

    let value_after = eth_call_u256(
        &rpc,
        &from_hex,
        &contract_hex,
        &hex_data(&value_call.abi_encode()),
    )
    .await?;
    if value_after != U256::from(args.new_value) {
        bail!(
            "eth_call/execute mismatch: expected updated value {} but got {}",
            args.new_value,
            value_after
        );
    }

    let gas_used = parse_hex_quantity_u256(get_required_string_field(&update_receipt, "gasUsed")?)?;
    if estimate_gas < gas_used {
        bail!("estimateGas/execute mismatch: estimate ({estimate_gas}) < gasUsed ({gas_used})");
    }
    println!("PASS: eth_call, eth_estimateGas, and executed tx outcome are coherent");

    let tx_obj = rpc
        .call("eth_getTransactionByHash", vec![json!(update_hash_hex)])
        .await?;
    if tx_obj.is_null() {
        bail!("eth_getTransactionByHash returned null for mined tx");
    }
    validate_tx_shape(&tx_obj)?;
    assert_tx_receipt_consistency(&tx_obj, &update_receipt, &update_hash)?;

    let block_number = get_required_string_field(&update_receipt, "blockNumber")?.to_owned();
    let block_hash = get_required_string_field(&update_receipt, "blockHash")?.to_owned();
    let block_hashes_only = rpc
        .call(
            "eth_getBlockByNumber",
            vec![json!(block_number), json!(false)],
        )
        .await?;
    assert_block_contains_tx_hash(&block_hashes_only, &update_hash)?;
    let block_full = rpc
        .call(
            "eth_getBlockByNumber",
            vec![json!(block_number), json!(true)],
        )
        .await?;
    assert_block_contains_full_tx(&block_full, &update_hash)?;
    let block_by_hash = rpc
        .call("eth_getBlockByHash", vec![json!(block_hash), json!(false)])
        .await?;
    assert_block_contains_tx_hash(&block_by_hash, &update_hash)?;
    println!("PASS: tx/receipt/block cross-endpoint consistency holds");

    let logs = rpc
        .call(
            "eth_getLogs",
            vec![json!({
                "fromBlock": block_number,
                "toBlock": block_number,
                "address": contract_hex,
            })],
        )
        .await?;
    assert_logs_include_tx(&logs, &update_hash)?;
    println!("PASS: eth_getLogs returns the emitted contract event");

    let latest_after = get_tx_count(&rpc, &from_hex, "latest").await?;
    let pending_after = get_tx_count(&rpc, &from_hex, "pending").await?;
    if latest_after < latest_before.saturating_add(2) {
        bail!(
            "nonce progression invalid: latest before={latest_before}, latest after={latest_after}"
        );
    }
    if pending_after < latest_after {
        bail!(
            "nonce ordering invalid after txs: pending ({pending_after}) < latest ({latest_after})"
        );
    }
    println!("PASS: nonce progression and pending/latest ordering are stable");

    println!("RPC compatibility suite completed successfully.");
    Ok(())
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

    if let Some(to) = to {
        tx = tx.to(to);
    }

    tx
}

fn socket_addr_from_rpc_url(rpc_url: &str) -> anyhow::Result<SocketAddr> {
    let parsed = Url::parse(rpc_url).with_context(|| format!("invalid rpc url: {rpc_url}"))?;
    let host = parsed
        .host_str()
        .context("rpc url must include a hostname")?;
    let port = parsed
        .port_or_known_default()
        .context("rpc url must include an explicit or default port")?;
    format!("{host}:{port}")
        .parse()
        .with_context(|| format!("failed to parse socket address from {rpc_url}"))
}

async fn wait_for_receipt(
    rpc: &JsonRpcClient,
    tx_hash: &str,
    timeout: Duration,
    label: &str,
) -> anyhow::Result<Value> {
    let start = Instant::now();
    loop {
        let receipt = rpc
            .call("eth_getTransactionReceipt", vec![json!(tx_hash)])
            .await?;
        if !receipt.is_null() {
            return Ok(receipt);
        }
        if start.elapsed() > timeout {
            bail!("timed out waiting for {label} after {}s", timeout.as_secs());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn get_tx_count(rpc: &JsonRpcClient, address: &str, tag: &str) -> anyhow::Result<u64> {
    let value = rpc
        .call(
            "eth_getTransactionCount",
            vec![json!(address), json!(tag.to_string())],
        )
        .await?;
    let value_str = value
        .as_str()
        .context("eth_getTransactionCount must return a string")?;
    check_hex_quantity_str(value_str, &format!("eth_getTransactionCount({tag})"))?;
    parse_hex_quantity_u64(value_str)
}

async fn eth_estimate_gas(
    rpc: &JsonRpcClient,
    from: &str,
    to: &str,
    data: &str,
) -> anyhow::Result<U256> {
    let value = rpc
        .call(
            "eth_estimateGas",
            vec![json!({
                "from": from,
                "to": to,
                "data": data,
            })],
        )
        .await?;
    let value_str = value
        .as_str()
        .context("eth_estimateGas must return a hex quantity string")?;
    check_hex_quantity_str(value_str, "eth_estimateGas")?;
    parse_hex_quantity_u256(value_str)
}

async fn eth_call_u256(
    rpc: &JsonRpcClient,
    from: &str,
    to: &str,
    data: &str,
) -> anyhow::Result<U256> {
    let result = rpc
        .call(
            "eth_call",
            vec![
                json!({
                    "from": from,
                    "to": to,
                    "data": data,
                }),
                json!("latest"),
            ],
        )
        .await?;
    let result_str = result
        .as_str()
        .context("eth_call must return a hex data string")?;
    check_hex_data_str(result_str, "eth_call")?;
    decode_u256_from_hex_data(result_str)
}

fn parse_contract_address(receipt: &Value) -> anyhow::Result<Address> {
    let contract = get_required_string_field(receipt, "contractAddress")?;
    contract
        .parse::<Address>()
        .with_context(|| format!("invalid contractAddress: {contract}"))
}

fn hex_data(data: &[u8]) -> String {
    format!("0x{}", hex::encode(data))
}

fn validate_fee_history_shape(fee_history: &Value, block_count: usize) -> anyhow::Result<()> {
    let oldest_block = get_required_string_field(fee_history, "oldestBlock")?;
    check_hex_quantity_str(oldest_block, "eth_feeHistory.oldestBlock")?;

    let base_fee_per_gas = fee_history
        .get("baseFeePerGas")
        .and_then(Value::as_array)
        .context("eth_feeHistory.baseFeePerGas must be an array")?;
    if base_fee_per_gas.len() != block_count + 1 {
        bail!(
            "eth_feeHistory.baseFeePerGas length expected {}, got {}",
            block_count + 1,
            base_fee_per_gas.len()
        );
    }
    for (idx, value) in base_fee_per_gas.iter().enumerate() {
        let s = value
            .as_str()
            .with_context(|| format!("eth_feeHistory.baseFeePerGas[{idx}] must be string"))?;
        check_hex_quantity_str(s, &format!("eth_feeHistory.baseFeePerGas[{idx}]"))?;
    }

    let gas_used_ratio = fee_history
        .get("gasUsedRatio")
        .and_then(Value::as_array)
        .context("eth_feeHistory.gasUsedRatio must be an array")?;
    if gas_used_ratio.len() != block_count {
        bail!(
            "eth_feeHistory.gasUsedRatio length expected {}, got {}",
            block_count,
            gas_used_ratio.len()
        );
    }
    for (idx, value) in gas_used_ratio.iter().enumerate() {
        if !value.is_f64() && !value.is_u64() && !value.is_i64() {
            bail!("eth_feeHistory.gasUsedRatio[{idx}] must be numeric");
        }
    }
    Ok(())
}

fn validate_receipt_shape(receipt: &Value) -> anyhow::Result<()> {
    for field in [
        "transactionHash",
        "blockHash",
        "blockNumber",
        "transactionIndex",
        "gasUsed",
        "cumulativeGasUsed",
        "status",
        "effectiveGasPrice",
    ] {
        ensure_required_field(receipt, field)?;
    }

    check_hex_hash_str(
        get_required_string_field(receipt, "transactionHash")?,
        "receipt.transactionHash",
    )?;
    check_hex_hash_str(
        get_required_string_field(receipt, "blockHash")?,
        "receipt.blockHash",
    )?;
    for field in [
        "blockNumber",
        "transactionIndex",
        "gasUsed",
        "cumulativeGasUsed",
        "status",
        "effectiveGasPrice",
    ] {
        check_hex_quantity_str(
            get_required_string_field(receipt, field)?,
            &format!("receipt.{field}"),
        )?;
    }

    receipt
        .get("logs")
        .and_then(Value::as_array)
        .context("receipt.logs must be an array")?;

    if let Some(contract) = receipt.get("contractAddress") {
        if !contract.is_null() {
            let contract_str = contract
                .as_str()
                .context("receipt.contractAddress must be string|null")?;
            check_hex_address_str(contract_str, "receipt.contractAddress")?;
        }
    }

    Ok(())
}

fn validate_tx_shape(tx: &Value) -> anyhow::Result<()> {
    for field in [
        "hash",
        "nonce",
        "blockHash",
        "blockNumber",
        "transactionIndex",
        "from",
        "to",
        "gas",
    ] {
        ensure_required_field(tx, field)?;
    }

    check_hex_hash_str(get_required_string_field(tx, "hash")?, "tx.hash")?;
    check_hex_quantity_str(get_required_string_field(tx, "nonce")?, "tx.nonce")?;
    check_hex_hash_str(get_required_string_field(tx, "blockHash")?, "tx.blockHash")?;
    check_hex_quantity_str(
        get_required_string_field(tx, "blockNumber")?,
        "tx.blockNumber",
    )?;
    check_hex_quantity_str(
        get_required_string_field(tx, "transactionIndex")?,
        "tx.transactionIndex",
    )?;
    check_hex_address_str(get_required_string_field(tx, "from")?, "tx.from")?;
    let to = tx
        .get("to")
        .and_then(Value::as_str)
        .context("tx.to must be a string")?;
    check_hex_address_str(to, "tx.to")?;
    check_hex_quantity_str(get_required_string_field(tx, "gas")?, "tx.gas")?;
    Ok(())
}

fn assert_tx_receipt_consistency(
    tx: &Value,
    receipt: &Value,
    expected_hash: &B256,
) -> anyhow::Result<()> {
    let expected_hash_str = format!("{:#x}", expected_hash);
    let tx_hash = get_required_string_field(tx, "hash")?;
    let receipt_hash = get_required_string_field(receipt, "transactionHash")?;

    if tx_hash != expected_hash_str {
        bail!("tx.hash mismatch: expected {expected_hash_str}, got {tx_hash}");
    }
    if receipt_hash != expected_hash_str {
        bail!("receipt.transactionHash mismatch: expected {expected_hash_str}, got {receipt_hash}");
    }

    for field in ["blockHash", "blockNumber", "transactionIndex"] {
        let tx_val = get_required_string_field(tx, field)?;
        let receipt_val = get_required_string_field(receipt, field)?;
        if tx_val != receipt_val {
            bail!("tx/receipt mismatch for {field}: tx={tx_val}, receipt={receipt_val}");
        }
    }
    Ok(())
}

fn assert_block_contains_tx_hash(block: &Value, tx_hash: &B256) -> anyhow::Result<()> {
    let block_hash = get_required_string_field(block, "hash")?;
    check_hex_hash_str(block_hash, "block.hash")?;

    let txs = block
        .get("transactions")
        .and_then(Value::as_array)
        .context("block.transactions must be an array")?;
    let tx_hash_str = format!("{:#x}", tx_hash);
    if !txs.iter().any(|v| v.as_str() == Some(tx_hash_str.as_str())) {
        bail!("block.transactions does not contain expected tx hash {tx_hash_str}");
    }
    Ok(())
}

fn assert_block_contains_full_tx(block: &Value, tx_hash: &B256) -> anyhow::Result<()> {
    let txs = block
        .get("transactions")
        .and_then(Value::as_array)
        .context("full block.transactions must be an array")?;
    let tx_hash_str = format!("{:#x}", tx_hash);
    let has_match = txs.iter().any(|tx| {
        tx.get("hash")
            .and_then(Value::as_str)
            .is_some_and(|hash| hash == tx_hash_str)
    });
    if !has_match {
        bail!("full block.transactions does not include tx object for {tx_hash_str}");
    }
    Ok(())
}

fn assert_logs_include_tx(logs: &Value, tx_hash: &B256) -> anyhow::Result<()> {
    let logs = logs
        .as_array()
        .context("eth_getLogs result must be an array")?;
    if logs.is_empty() {
        bail!("eth_getLogs returned no logs for emitted event");
    }

    let tx_hash_str = format!("{:#x}", tx_hash);
    let matches = logs.iter().any(|log| {
        log.get("transactionHash")
            .and_then(Value::as_str)
            .is_some_and(|hash| hash == tx_hash_str)
    });
    if !matches {
        bail!("eth_getLogs result did not include expected transaction hash {tx_hash_str}");
    }
    Ok(())
}

fn assert_receipt_status_success(receipt: &Value) -> anyhow::Result<()> {
    let status = get_required_string_field(receipt, "status")?;
    let status_u64 = parse_hex_quantity_u64(status)?;
    if status_u64 != 1 {
        bail!("receipt status is not success: {status}");
    }
    Ok(())
}

fn ensure_required_field<'a>(obj: &'a Value, field: &str) -> anyhow::Result<&'a Value> {
    obj.get(field)
        .with_context(|| format!("missing required field: {field}"))
}

fn get_required_string_field<'a>(obj: &'a Value, field: &str) -> anyhow::Result<&'a str> {
    ensure_required_field(obj, field)?
        .as_str()
        .with_context(|| format!("field {field} must be string"))
}

fn check_hex_quantity(value: &Value, label: &str) -> anyhow::Result<()> {
    let s = value
        .as_str()
        .with_context(|| format!("{label} must return string"))?;
    check_hex_quantity_str(s, label)
}

fn check_hex_quantity_str(value: &str, label: &str) -> anyhow::Result<()> {
    let digits = value
        .strip_prefix("0x")
        .with_context(|| format!("{label} must start with 0x"))?;
    if digits.is_empty() {
        bail!("{label} has empty hex payload");
    }
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("{label} contains non-hex characters: {value}");
    }
    if digits.len() > 1 && digits.starts_with('0') {
        bail!("{label} has a non-canonical leading zero quantity: {value}");
    }
    Ok(())
}

fn check_hex_data_str(value: &str, label: &str) -> anyhow::Result<()> {
    let digits = value
        .strip_prefix("0x")
        .with_context(|| format!("{label} must start with 0x"))?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("{label} contains non-hex characters: {value}");
    }
    if digits.len() % 2 != 0 {
        bail!("{label} hex payload length must be even: {value}");
    }
    Ok(())
}

fn check_hex_hash_str(value: &str, label: &str) -> anyhow::Result<()> {
    let digits = value
        .strip_prefix("0x")
        .with_context(|| format!("{label} must start with 0x"))?;
    if digits.len() != 64 {
        bail!(
            "{label} must be 32-byte hex value, got len={}",
            digits.len()
        );
    }
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("{label} contains non-hex characters");
    }
    Ok(())
}

fn check_hex_address_str(value: &str, label: &str) -> anyhow::Result<()> {
    let digits = value
        .strip_prefix("0x")
        .with_context(|| format!("{label} must start with 0x"))?;
    if digits.len() != 40 {
        bail!(
            "{label} must be 20-byte hex value, got len={}",
            digits.len()
        );
    }
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("{label} contains non-hex characters");
    }
    Ok(())
}

fn parse_hex_quantity_u64(value: &str) -> anyhow::Result<u64> {
    let digits = value
        .strip_prefix("0x")
        .with_context(|| format!("expected hex quantity string, got {value}"))?;
    u64::from_str_radix(digits, 16).with_context(|| format!("failed to parse quantity {value}"))
}

fn parse_hex_quantity_u256(value: &str) -> anyhow::Result<U256> {
    let digits = value
        .strip_prefix("0x")
        .with_context(|| format!("expected hex quantity string, got {value}"))?;
    U256::from_str_radix(digits, 16).with_context(|| format!("failed to parse quantity {value}"))
}

fn decode_u256_from_hex_data(data: &str) -> anyhow::Result<U256> {
    let digits = data
        .strip_prefix("0x")
        .with_context(|| format!("expected hex data string, got {data}"))?;
    if digits.len() < 64 {
        bail!("hex data too short for uint256 decode: {data}");
    }
    let tail = &digits[digits.len() - 64..];
    U256::from_str_radix(tail, 16).with_context(|| format!("failed to decode uint256 from {data}"))
}
