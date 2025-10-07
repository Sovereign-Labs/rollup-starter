use sov_modules_api::capabilities::config_chain_id;
use sov_modules_api::{DispatchCall, PrivateKey, Runtime as RuntimeTrait};
use sov_modules_api::transaction::{Transaction, TxDetails};
use sov_test_utils::{TransactionType, TEST_DEFAULT_MAX_FEE, TEST_DEFAULT_MAX_PRIORITY_FEE};
use std::collections::HashMap;
use tokio_stream::StreamExt;
use tokio::sync::watch;
use super::{Runtime, Spec, API_URL};

#[derive(serde::Deserialize, Debug)]
struct StateRootResponse {
    root_hashes: Vec<u8>,
}

#[derive(serde::Deserialize, Debug)]
struct ValueResponse<T> {
    value: T,
}

pub async fn state_validation_worker(
    client: sov_api_spec::Client,
    rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    use futures::FutureExt;
    use sov_rollup_interface::node::ledger_api::IncludeChildren;

    // Subscribe to slots
    let mut slot_stream = client
        .subscribe_slots_with_children(IncludeChildren::new(false))
        .await?;

    tracing::info!("State validation worker started");

    while !*rx.borrow() {
        // Wait for next slot notification (blocking)
        let mut latest_slot = match slot_stream.next().await {
            Some(Ok(slot)) => slot,
            Some(Err(e)) => {
                tracing::error!("Error receiving slot: {}", e);
                continue;
            }
            None => break,
        };
        tracing::info!("State validation: got new slot notification, number = {}", latest_slot.number);

        // Check if there are any additional slots already queued (non-blocking)
        // If so, we're falling behind and should fail
        let mut drained_count = 0;
        while let Some(Some(Ok(slot))) = slot_stream.next().now_or_never() {
            latest_slot = slot;
            drained_count += 1;
        }

        if drained_count > 0 {
            tracing::warn!(
                "State validation worker drained {} slots - slots are being produced faster than we can process them! Those slots will have skipped kernel state validation.",
                drained_count
            );
        }

        // Query the state values from the module
        let visible_slot_url = format!(
            "{}/modules/state-consistency/state/latest-visible-slot-number/",
            API_URL
        );
        let state_root_url = format!(
            "{}/modules/state-consistency/state/latest-state-root/",
            API_URL
        );
        let rollup_height_url = format!(
            "{}/modules/state-consistency/state/latest-rollup-height/",
            API_URL
        );

        // First, poll only visible_slot until the API state catches up (to handle race condition
        // between slot notification and checkpoint update)
        let visible_slot = {
            let max_attempts = 300; // 300 * 10ms = 3s = one full slot at the default config
            let mut attempt = 0;

            loop {
                let visible_slot: ValueResponse<u64> = client
                    .client()
                    .get(&visible_slot_url)
                    .send()
                    .await?
                    .json()
                    .await?;

                // Check if the API state has caught up to the slot notification
                if visible_slot.value == latest_slot.number {
                    tracing::debug!("State consistency: waited {} ms for API state to be updated...", attempt * 10);
                    break visible_slot;
                }

                attempt += 1;
                if attempt >= max_attempts {
                    tracing::error!(
                        "State validation worker timed out waiting for API state to update. Slot notification: {}, API visible_slot: {}. This is either an error in the sequencer, or the acceptance test has a bug.",
                        latest_slot.number,
                        visible_slot.value
                    );
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        };

        // Once the visible_slot has caught up, we can query the other two values
        let (state_root_result, rollup_height_result) = tokio::join!(
            async {
                client
                    .client()
                    .get(&state_root_url)
                    .send()
                    .await?
                    .json::<ValueResponse<StateRootResponse>>()
                    .await
            },
            async {
                client
                    .client()
                    .get(&rollup_height_url)
                    .send()
                    .await?
                    .json::<ValueResponse<u64>>()
                    .await
            }
        );
        let state_root = state_root_result?;
        let rollup_height = rollup_height_result?;

        tracing::info!(
            "State validation: slot_notification={}, module_visible_slot={}, module_rollup_height={}, state_root={}",
            latest_slot.number,
            visible_slot.value,
            rollup_height.value,
            hex::encode(&state_root.value.root_hashes)
        );

        // Now we can submit the assertion transaction
        let assert_tx = create_assert_block_state_tx(
            visible_slot.value,
            rollup_height.value,
            state_root.value.root_hashes,
        )?;

        if let Err(e) = client.send_tx_to_sequencer(&assert_tx).await {
            if e.to_string().contains("The preferred sequencer has reached the stop height") {
                tracing::info!("State validation worker detected sequencer stop height, shutting down");
                return Ok(())
            }

            // Check if this is a "too late" error (actual rollup height > expected)
            if is_too_late_error(&e, rollup_height.value) {
                tracing::warn!(
                    "State assertion tx for slot {} height {} was rejected because the rollup already advanced past it. This can happen, but the kernel assertions have been skipped for this slot as a result.",
                    visible_slot.value,
                    rollup_height.value,
                );
                continue;
            }

            anyhow::bail!(
                "Failed to submit state assertion tx for slot {} height {}: {}",
                visible_slot.value,
                rollup_height.value,
                e
            );
        }
    }

    tracing::info!("State validation worker shutting down");
    Ok(())
}

fn create_assert_block_state_tx(
    expected_visible_slot_number: u64,
    expected_rollup_height: u64,
    expected_state_root: Vec<u8>,
) -> anyhow::Result<Transaction<Runtime, Spec>> {
    // Generate a new key for this transaction
    let key = <<Spec as sov_modules_api::Spec>::CryptoSpec as sov_modules_api::CryptoSpec>::PrivateKey::generate();

    let message = <Runtime as DispatchCall>::Decodable::StateConsistency(
        sov_test_state_consistency::CallMessage::AssertBlockState {
            expected_visible_slot_number,
            expected_rollup_height,
            expected_state_root,
        },
    );

    // Sign but DON'T serialize - let send_tx_to_sequencer handle serialization
    Ok(TransactionType::<Runtime, Spec>::sign(
        message,
        key.clone(),
        &Runtime::CHAIN_HASH,
        TxDetails {
            max_priority_fee_bips: TEST_DEFAULT_MAX_PRIORITY_FEE,
            max_fee: TEST_DEFAULT_MAX_FEE,
            gas_limit: None,
            chain_id: config_chain_id(),
        },
        &mut HashMap::from([(key.pub_key(), 0)]),
    ))
}

/// Check if the error is a "too late" error, meaning the transaction was rejected
/// because the actual rollup height is greater than the expected height.
fn is_too_late_error(
    e: &sov_api_spec::Error<sov_api_spec::types::ApiError>,
    expected_height: u64,
) -> bool {
    // Extract the ApiError from the progenitor Error wrapper
    let api_error = match e {
        sov_api_spec::Error::ErrorResponse(response_value) => response_value,
        _ => return false,
    };

    // Get the error string from the details map
    let error_str = match api_error.details.get("error") {
        Some(serde_json::Value::String(s)) => s,
        _ => return false,
    };

    // Parse the error string to extract the actual rollup height
    // Pattern: "Block state assertion failed at rollup height <actual_height>."
    let actual_height = match extract_rollup_height_from_error(&error_str) {
        Some(h) => h,
        None => return false,
    };

    // If the actual height is greater than expected, this is a "too late" error
    actual_height > expected_height
}

/// Extract the rollup height from the error message.
/// Pattern: "Block state assertion failed at rollup height <height>."
fn extract_rollup_height_from_error(error_str: &str) -> Option<u64> {
    // Find "Block state assertion failed at rollup height "
    let prefix = "Block state assertion failed at rollup height ";
    let start_idx = error_str.find(prefix)?;
    let height_start = start_idx + prefix.len();

    // Find the next non-digit character (should be '.')
    let remaining = &error_str[height_start..];
    let height_str = remaining.split(|c: char| !c.is_ascii_digit()).next()?;

    height_str.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rollup_height_from_error() {
        let error_msg = "TransactionReceipt { tx_hash: 0x7e55c8eb41c10c756d1a226bf104297e1e328317102485b0bc6b1498bc4283cc, body_to_save: \"<removed>\", events: [], receipt: Reverted(RevertedTxContents { gas_used: GasUnit[21287, 21287], reason: ModuleError(Block state assertion failed at rollup height 14. List of mismatches: [\"Rollup height mismatch. Transaction expected 13, but actual state is 14 (sanity check: saved state from begin block hook is 14)\"]) }) }";

        let height = extract_rollup_height_from_error(error_msg);
        assert_eq!(height, Some(14));
    }

    #[test]
    fn test_extract_rollup_height_no_match() {
        let error_msg = "Some other error message";
        let height = extract_rollup_height_from_error(error_msg);
        assert_eq!(height, None);
    }
}
