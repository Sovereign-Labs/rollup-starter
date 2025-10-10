use crate::{module_state::*, Directories, Runtime, Spec};
use anyhow::Result;
use rand::Rng;
use sov_api_spec::types::Slot;
use sov_modules_api::capabilities::config_chain_id;
use sov_modules_api::transaction::TxDetails;
use sov_modules_api::{DispatchCall, PrivateKey, Runtime as RuntimeTrait};
use sov_test_utils::{TransactionType, TEST_DEFAULT_MAX_FEE, TEST_DEFAULT_MAX_PRIORITY_FEE};
use std::collections::HashMap;
use tokio::sync::watch;
use tokio_stream::StreamExt;

struct PendingTx {
    tx_number: u64,
    new_value: u64,
    expected_slot: u64,
}

pub async fn accessory_state_worker(
    client: sov_api_spec::Client,
    shutdown_rx: watch::Receiver<bool>,
    directories: Directories,
    save_enabled: bool,
) -> Result<()> {
    use futures::FutureExt;
    use sov_rollup_interface::node::ledger_api::IncludeChildren;

    let mut slot_stream = client
        .subscribe_slots_with_children(IncludeChildren::new(true)) // Need batches for tx_range
        .await?;

    let mut pending_tx: Option<PendingTx> = None;
    let mut current_value = query_accessory_value_immediate(&client).await?;

    tracing::info!(
        "Accessory state worker started, initial value: {}",
        current_value
    );

    while !*shutdown_rx.borrow() {
        // Step 1: Get exactly one slot (completed)
        let slot = match slot_stream.next().await {
            Some(Ok(slot)) => slot,
            Some(Err(e)) => {
                tracing::error!("Error receiving slot: {}", e);
                continue;
            }
            None => break,
        };

        // Step 2: Check for buffered slots - bail if any
        if slot_stream.next().now_or_never().is_some() {
            anyhow::bail!(
                "Websocket lag detected: multiple slots buffered at slot {}. \
                This simplified implementation cannot handle websocket lag.",
                slot.number
            );
        }

        // Step 3: Save the completed slot with correct value
        let slot_value = if let Some(pending) = &pending_tx {
            if tx_in_slot(pending.tx_number, &slot) {
                // Tx is in this slot - slot has NEW value
                tracing::debug!(
                    "Accessory tx {} found in slot {}",
                    pending.tx_number,
                    slot.number
                );
                pending.new_value
            } else {
                // Tx not in this slot - slot has OLD value
                tracing::debug!(
                    "Accessory tx {} not in slot {} (expected {}), using old value",
                    pending.tx_number,
                    slot.number,
                    pending.expected_slot
                );
                current_value
            }
        } else {
            // No pending tx (first iteration)
            current_value
        };

        // Save and verify
        if save_enabled {
            save_module_state(slot.number, slot_value, &directories.snapshots_dir)?;
        }

        // Always verify with historical API
        let historical = query_accessory_value_at_slot(&client, slot.number).await?;
        anyhow::ensure!(
            historical == slot_value,
            "Historical query mismatch at slot {}: expected {}, got {}",
            slot.number,
            slot_value,
            historical
        );

        // Update current_value if it changed
        current_value = slot_value;

        // Step 4: Always submit new transaction (may overwrite pending_tx)
        let new_value = rand::thread_rng().gen::<u64>();
        tracing::debug!(
            "ACCESSORY: sending update tx after slot {}, value {}",
            slot.number,
            new_value
        );
        let accessory_tx = create_update_accessory_state_tx(new_value)?;

        match client.send_tx_to_sequencer(&accessory_tx).await {
            Ok(receipt) => {
                // MUST match immediately
                let immediate = query_accessory_value_immediate(&client).await?;
                anyhow::ensure!(
                    immediate == new_value,
                    "Immediate assertion failed: expected {}, got {}",
                    new_value,
                    immediate
                );

                let expected_slot = slot.number + 1;
                let tx_number = receipt.tx_number.unwrap_or(0);
                pending_tx = Some(PendingTx {
                    tx_number,
                    new_value,
                    expected_slot,
                });

                tracing::debug!(
                    "Submitted accessory tx {}, value: {}, expected slot: {}",
                    tx_number,
                    new_value,
                    expected_slot
                );
            }
            Err(e) if e.to_string().contains("The preferred sequencer has reached the stop height") => {
                tracing::info!("Accessory worker detected sequencer stop height");
                break;
            }
            Err(e) => {
                anyhow::bail!("Failed to submit accessory state tx: {}", e);
            }
        }
    }

    // Shutdown: process remaining slots (no new submissions)
    if let Some(pending) = pending_tx {
        tracing::info!(
            "Accessory worker shutdown: waiting for pending tx {}",
            pending.tx_number
        );

        // Process up to 2 remaining slots
        for _ in 0..2 {
            if let Some(Ok(slot)) = slot_stream.next().await {
                // Same logic as main loop: save with correct value
                let slot_value = if tx_in_slot(pending.tx_number, &slot) {
                    tracing::info!("Pending tx {} found in slot {} during shutdown", pending.tx_number, slot.number);
                    pending.new_value
                } else {
                    current_value
                };

                if save_enabled {
                    save_module_state(slot.number, slot_value, &directories.snapshots_dir)?;
                }

                // Verify
                let historical = query_accessory_value_at_slot(&client, slot.number).await?;
                anyhow::ensure!(
                    historical == slot_value,
                    "Historical query mismatch at slot {} during shutdown: expected {}, got {}",
                    slot.number,
                    slot_value,
                    historical
                );

                current_value = slot_value;
            }
        }
    }

    tracing::info!("Accessory state worker shutting down");
    Ok(())
}

fn tx_in_slot(tx_number: u64, slot: &Slot) -> bool {
    for batch in &slot.batches {
        if tx_number >= batch.tx_range.start && tx_number < batch.tx_range.end {
            return true;
        }
    }
    false
}

fn create_update_accessory_state_tx(
    value: u64,
) -> Result<sov_modules_api::transaction::Transaction<Runtime, Spec>> {
    let key = <<Spec as sov_modules_api::Spec>::CryptoSpec as sov_modules_api::CryptoSpec>::PrivateKey::generate();

    let message = <Runtime as DispatchCall>::Decodable::StateConsistency(
        sov_test_state_consistency::CallMessage::UpdateAccessoryState(value),
    );

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
