use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleStateSnapshot {
    pub accessory_value: u64,
}

#[derive(serde::Deserialize)]
struct ValueResponse<T> {
    value: T,
}

/// Query accessory value at tip (no ?slot parameter)
pub async fn query_accessory_value_immediate(
    client: &sov_api_spec::Client,
) -> anyhow::Result<u64> {
    let url = format!(
        "{}/modules/state-consistency/state/accessory-value/",
        crate::API_URL
    );
    let response: ValueResponse<u64> = client.client().get(&url).send().await?.json().await?;
    Ok(response.value)
}

/// Query accessory value at specific slot
pub async fn query_accessory_value_at_slot(
    client: &sov_api_spec::Client,
    slot: u64,
) -> anyhow::Result<u64> {
    let url = format!(
        "{}/modules/state-consistency/state/accessory-value/?slot={}",
        crate::API_URL,
        slot
    );
    let response: ValueResponse<u64> = client.client().get(&url).send().await?.json().await?;
    Ok(response.value)
}

pub fn save_module_state(
    slot_number: u64,
    accessory_value: u64,
    snapshots_dir: &PathBuf,
) -> anyhow::Result<()> {
    let snapshot = ModuleStateSnapshot { accessory_value };
    let filename = format!("slot_{:04}_state.json", slot_number);
    let filepath = snapshots_dir.join(filename);
    let json = serde_json::to_string_pretty(&snapshot)?;
    std::fs::write(filepath, json)?;
    Ok(())
}

pub fn load_state_snapshot(
    slot_number: u64,
    snapshots_dir: &PathBuf,
) -> anyhow::Result<ModuleStateSnapshot> {
    let filename = format!("slot_{:04}_state.json", slot_number);
    let filepath = snapshots_dir.join(filename);
    let contents = std::fs::read_to_string(filepath)?;
    Ok(serde_json::from_str(&contents)?)
}

pub async fn verify_module_state(
    slot_number: u64,
    client: &sov_api_spec::Client,
    snapshot: &ModuleStateSnapshot,
) -> anyhow::Result<()> {
    let actual = query_accessory_value_at_slot(client, slot_number).await?;
    anyhow::ensure!(
        actual == snapshot.accessory_value,
        "Accessory value mismatch at slot {}: expected {}, got {}",
        slot_number,
        snapshot.accessory_value,
        actual
    );
    Ok(())
}
