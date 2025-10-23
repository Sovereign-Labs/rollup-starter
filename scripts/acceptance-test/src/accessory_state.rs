use crate::{module_state::*, Directories, Runtime, Spec};
use anyhow::Result;
use rand::Rng;
use sov_api_spec::types::Slot;
use sov_modules_api::capabilities::config_chain_id;
use sov_modules_api::transaction::TxDetails;
use sov_modules_api::{DispatchCall, PrivateKey, Runtime as RuntimeTrait};
use sov_rollup_interface::node::ledger_api::IncludeChildren;
use sov_test_utils::{TransactionType, TEST_DEFAULT_MAX_FEE, TEST_DEFAULT_MAX_PRIORITY_FEE};
use std::collections::HashMap;
use std::time::Duration;
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
        tracing::warn!("ACCESSORY STATE WORKER: waiting for slot (start of loop)...");
        // Step 1: Get exactly one slot (completed)
        let slot = match slot_stream.next().await {
            Some(Ok(slot)) => slot,
            Some(Err(e)) => {
                tracing::error!("Error receiving slot: {}", e);
                continue;
            }
            None => break,
        };
        tracing::warn!(
            "ACCESSORY STATE WORKER: got slot {}. Proceeding with loop body.",
            slot.number
        );

        // If pending tx did NOT get included, save the slot with the old value (and skip sending a
        // new tx)
        // If it DID get included, save the slot with the updated value and send new tx
        if let Some(pending) = &pending_tx {
            if tx_in_slot(pending.tx_number, &slot) {
                // Tx is in this slot - update known current value
                current_value = pending.new_value;
                tracing::info!(
                    "ACCESSORY STATE WORKER: Accessory tx {} found in slot {} (expected {}), expecting state to have new value {current_value}",
                    pending.tx_number,
                    slot.number,
                    pending.expected_slot
                );
                pending_tx = None;
            } else {
                // Tx not in this slot - slot has old value
                tracing::info!(
                    "ACCESSORY STATE WORKER: Accessory tx {} not in slot {} (expected {}), using old value {current_value}",
                    pending.tx_number,
                    slot.number,
                    pending.expected_slot
                );
            }
        }

        if save_enabled {
            save_module_state(slot.number, current_value, &directories.snapshots_dir)?;
        }

        // TODO: clean up race condition - integrate with state_consistency worker?
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Always verify with historical API
        let historical = query_accessory_value_at_slot(&client, slot.number).await?;
        anyhow::ensure!(
            historical == current_value,
            "Historical query mismatch at slot {}: expected {}, got {}",
            slot.number,
            current_value,
            historical
        );

        // Step 4: If we're no longer waiting for inclusion, send a new tx
        if pending_tx.is_none() {
            let new_value = rand::thread_rng().gen::<u64>();
            tracing::debug!(
                "ACCESSORY: sending update tx after slot {}, value {}",
                slot.number,
                new_value
            );
            let accessory_tx = create_update_accessory_state_tx(new_value)?;

            match client.send_tx_to_sequencer(&accessory_tx).await {
                Ok(receipt) => {
                    // state must match immediately
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

                    tracing::info!(
                        "ACCESSORY STATE WORKER: Submitted accessory tx {}, value: {}, expected slot: {}",
                        tx_number,
                        new_value,
                        expected_slot
                    );
                }
                Err(e)
                    if e.to_string()
                        .contains("The preferred sequencer has reached the stop height") =>
                {
                    tracing::info!("Accessory worker detected sequencer stop height");
                    break;
                }
                Err(e) => {
                    anyhow::bail!("Failed to submit accessory state tx: {}", e);
                }
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
