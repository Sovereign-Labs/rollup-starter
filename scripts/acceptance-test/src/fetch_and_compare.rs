use sov_api_spec::types::{self, GetBatchByIdChildren, GetSlotByIdChildren, LedgerBatch, Slot};

use futures::stream::Stream;
use serde_json::Value;
use sov_rollup_interface::node::ledger_api::IncludeChildren;
use std::path::PathBuf;
use tokio_stream::StreamExt;

use crate::Directories;

fn assert_slots_match_excluding_batches(slot1: &Slot, slot2: &Slot, description: &str) {
    assert_eq!(
        slot1.batch_range, slot2.batch_range,
        "{}: batch_range should match",
        description
    );
    assert_eq!(
        slot1.finality_status, slot2.finality_status,
        "{}: finality_status should match",
        description
    );
    assert_eq!(slot1.hash, slot2.hash, "{}: hash should match", description);
    assert_eq!(
        slot1.number, slot2.number,
        "{}: number should match",
        description
    );
    assert_eq!(
        slot1.state_root, slot2.state_root,
        "{}: state_root should match",
        description
    );
    assert_eq!(
        slot1.timestamp, slot2.timestamp,
        "{}: timestamp should match",
        description
    );
    assert_eq!(
        slot1.type_, slot2.type_,
        "{}: type should match",
        description
    );
}

fn slot_to_json(slot: &Slot, exclude_batches: bool) -> Result<Value, anyhow::Error> {
    let mut json = serde_json::to_value(slot)?;
    if let Value::Object(ref mut map) = json {
        if exclude_batches {
            map.remove("batches");
        }
    }
    Ok(json)
}

fn assert_slots_match_json_excluding_batches(
    slot1: &Slot,
    slot2: &Slot,
    description: &str,
) -> Result<(), anyhow::Error> {
    let json1 = slot_to_json(slot1, true)?;
    let json2 = slot_to_json(slot2, true)?;

    if json1 != json2 {
        println!("❌ {} JSON mismatch:", description);
        println!("Slot 1: {}", serde_json::to_string_pretty(&json1)?);
        println!("Slot 2: {}", serde_json::to_string_pretty(&json2)?);
        anyhow::bail!("{}: JSON comparison failed", description);
    }
    Ok(())
}

pub fn compare_against_snapshot(
    slot: &Slot,
    snapshot: serde_json::Value,
    description: &str,
    exclude_batches: bool,
) -> Result<(), ValidationError> {
    let slot_json = slot_to_json(slot, exclude_batches).expect("Failed to convert slot to JSON");

    if slot_json != snapshot {
        println!("❌ {} snapshot mismatch:", description);
        println!(
            "Actual: {}",
            serde_json::to_string_pretty(&slot_json).expect("Failed to convert slot to JSON")
        );
        println!(
            "Expected: {}",
            serde_json::to_string_pretty(&snapshot).expect("Failed to convert snapshot to JSON")
        );
        return Err(ValidationError::InvalidSnapshot);
    }
    Ok(())
}

pub fn save_slot_snapshot(slot: &Slot, output_dir: &PathBuf) -> Result<(), anyhow::Error> {
    let json = slot_to_json(slot, false)?;
    let snapshot_json = serde_json::to_string_pretty(&json)?;
    let filename = format!("slot_{:04}_with_children.json", slot.number);
    let filepath = output_dir.join(&filename);

    std::fs::write(&filepath, snapshot_json)?;

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Missing snapshot")]
    MissingSnapshot(std::io::Error),
    #[error("Invalid snapshot")]
    InvalidSnapshot,
}

pub fn load_snapshot_json(
    slot_number: u64,
    output_dir: &PathBuf,
) -> Result<serde_json::Value, std::io::Error> {
    let filename = format!("slot_{:04}_with_children.json", slot_number);
    let filepath = output_dir.join(&filename);
    let snapshot_json = std::fs::read_to_string(&filepath)?;
    Ok(serde_json::from_str(&snapshot_json).expect("Failed to parse snapshot JSON"))
}

pub fn validate_against_snapshot(
    slot: &Slot,
    output_dir: &PathBuf,
    description: &str,
) -> Result<(), ValidationError> {
    let json = load_snapshot_json(slot.number, output_dir)
        .map_err(|e| ValidationError::MissingSnapshot(e))?;

    compare_against_snapshot(slot, json, description, false)
}

pub enum GetItemBehavior {
    SaveSnapshot,
    DoNothing,
    CheckAgainstSnapshot,
}
pub struct SlotMonitor {
    slots: Box<dyn Stream<Item = Result<Slot, anyhow::Error>> + Unpin>,
    slots_with_children: Box<dyn Stream<Item = Result<Slot, anyhow::Error>> + Unpin>,
    finalized_slots: Box<dyn Stream<Item = Result<Slot, anyhow::Error>> + Unpin>,
    finalized_slots_with_children: Box<dyn Stream<Item = Result<Slot, anyhow::Error>> + Unpin>,
    pub prev_slot_with_children: Option<Slot>,
    snapshots_dir: PathBuf,
    expected_slot_number: Option<u64>,
}

impl SlotMonitor {
    pub async fn new(
        client: &sov_api_spec::Client,
        directories: &Directories,
    ) -> Result<Self, anyhow::Error> {
        let finalized_slots = client.subscribe_finalized_slots().await?;
        let finalized_slots_with_children = client
            .subscribe_finalized_slots_with_children(IncludeChildren::new(true))
            .await?;
        let slots = client.subscribe_slots().await?;
        let slots_with_children = client
            .subscribe_slots_with_children(IncludeChildren::new(true))
            .await?;

        Ok(Self {
            slots: Box::new(slots),
            slots_with_children: Box::new(slots_with_children),
            finalized_slots: Box::new(finalized_slots),
            finalized_slots_with_children: Box::new(finalized_slots_with_children),
            prev_slot_with_children: None,
            snapshots_dir: directories.snapshots_dir.clone(),
            expected_slot_number: None,
        })
    }

    pub async fn get_next_slot(
        &mut self,
        behavior: GetItemBehavior,
    ) -> Result<(Slot, Slot, Slot, Slot), anyhow::Error> {
        let next_slot = self.slots.next().await.unwrap().unwrap();
        let next_slot_with_children = self.slots_with_children.next().await.unwrap().unwrap();
        let finalized_next_slot = self.finalized_slots.next().await.unwrap().unwrap();
        let finalized_next_slot_with_children = self
            .finalized_slots_with_children
            .next()
            .await
            .unwrap()
            .unwrap();

        // Validate slot number sequence
        if let Some(expected) = self.expected_slot_number {
            if next_slot_with_children.number != expected {
                anyhow::bail!(
                    "Slot number out of sequence! Expected {}, got {}",
                    expected,
                    next_slot_with_children.number
                );
            }
        } else {
            // First slot - initialize the expected sequence
            self.expected_slot_number = Some(next_slot_with_children.number);
        }
        // Check that slots match (excluding batches field)
        assert_slots_match_excluding_batches(&next_slot, &next_slot_with_children, "Next slot");
        assert_slots_match_json_excluding_batches(
            &next_slot,
            &next_slot_with_children,
            "Next slot JSON",
        )?;

        // Check that finalized_slots_with_children matches finalized_slots (excluding batches field)
        assert_slots_match_excluding_batches(
            &finalized_next_slot,
            &finalized_next_slot_with_children,
            "Finalized slot",
        );
        assert_slots_match_json_excluding_batches(
            &finalized_next_slot,
            &finalized_next_slot_with_children,
            "Finalized slot JSON",
        )?;

        // Check if this slot has been finalized and has batches
        if finalized_next_slot.batch_range.end != finalized_next_slot.batch_range.start {
            if let Some(ref prev_slot_with_children) = self.prev_slot_with_children {
                assert_slots_match_excluding_batches(
                    &finalized_next_slot,
                    prev_slot_with_children,
                    "Next slot with children should match previous slot with children",
                );
                assert_eq!(
                    finalized_next_slot_with_children.batches, prev_slot_with_children.batches,
                    "Previous slot with children should match newly finalized slot with children"
                );
            }
        }

        // Save the next_slot_with_children snapshot
        match behavior {
            GetItemBehavior::SaveSnapshot => {
                save_slot_snapshot(&next_slot_with_children, &self.snapshots_dir)?;
            }
            GetItemBehavior::CheckAgainstSnapshot => {
                validate_against_snapshot(
                    &next_slot_with_children,
                    &self.snapshots_dir,
                    "Next slot with children",
                )?;
            }
            GetItemBehavior::DoNothing => {
                // Do nothing
            }
        }

        self.prev_slot_with_children = Some(next_slot_with_children.clone());

        // Update expected slot number for next iteration
        self.expected_slot_number = Some(next_slot_with_children.number + 1);

        Ok((
            next_slot,
            next_slot_with_children,
            finalized_next_slot,
            finalized_next_slot_with_children,
        ))
    }

    pub fn save_slot_as_snapshot(&self, slot: &Slot) -> Result<String, anyhow::Error> {
        let json = slot_to_json(slot, false)?;
        Ok(serde_json::to_string_pretty(&json)?)
    }
}

pub struct SlotFetcher {
    client: sov_api_spec::Client,
    output_dir: PathBuf,
    stream: Option<Box<dyn Stream<Item = Result<Slot, anyhow::Error>> + Unpin>>,
}

impl SlotFetcher {
    pub fn new(client: sov_api_spec::Client, directories: &Directories) -> Self {
        Self {
            client,
            output_dir: directories.snapshots_dir.clone(),
            stream: None,
        }
    }

    pub async fn subscribe_slots(&mut self, include_children: bool) -> Result<(), anyhow::Error> {
        let stream = self
            .client
            .subscribe_slots_with_children(IncludeChildren::new(include_children))
            .await?;
        self.stream = Some(Box::new(stream));
        Ok(())
    }

    pub async fn next_slot(&mut self) -> Result<Option<Slot>, anyhow::Error> {
        Ok(self.stream.as_mut().unwrap().next().await.transpose()?)
    }

    pub async fn fetch_batch_without_children(
        &self,
        batch_number: u64,
    ) -> Result<LedgerBatch, anyhow::Error> {
        Ok(self
            .client
            .get_batch_by_id(&types::IntOrHash::Integer(batch_number), None)
            .await?
            .into_inner())
    }

    pub async fn fetch_and_compare_batch(
        &self,
        batch_number: u64,
    ) -> Result<LedgerBatch, anyhow::Error> {
        let batch = self.fetch_batch_without_children(batch_number).await?;
        let batch_by_hash = self
            .client
            .get_batch_by_id(
                &types::IntOrHash::Hash(batch.hash.clone()),
                Some(GetBatchByIdChildren::_0),
            )
            .await?
            .into_inner();
        let mut batch_with_children = self
            .client
            .get_batch_by_id(
                &types::IntOrHash::Integer(batch_number),
                Some(GetBatchByIdChildren::_1),
            )
            .await?
            .into_inner();
        let batch_by_hash_with_children = self
            .client
            .get_batch_by_id(
                &types::IntOrHash::Hash(batch.hash.clone()),
                Some(GetBatchByIdChildren::_1),
            )
            .await?
            .into_inner();

        // Check that the batch fetched by number matches the batch fetched by hash
        assert_eq!(batch, batch_by_hash);
        // Check that the batch fetched by number with children matches the batch fetched by hash with children
        assert_eq!(batch_with_children, batch_by_hash_with_children);

        // Check that removing the children causes the types to match
        batch_with_children.txs.clear();
        assert_eq!(batch_with_children, batch);

        // Return the complete version with children
        Ok(batch_by_hash_with_children)
    }

    pub async fn fetch_and_compare_slot(
        &self,
        slot_number: u64,
        behavior: GetItemBehavior,
    ) -> Result<Slot, anyhow::Error> {
        // Fetch slot in all 4 possible ways
        let slot_with_children = self
            .client
            .get_slot_by_id(
                &types::IntOrHash::Integer(slot_number),
                Some(GetSlotByIdChildren::_1),
            )
            .await?;
        let slot_without_children = self
            .client
            .get_slot_by_id(
                &types::IntOrHash::Integer(slot_number),
                Some(GetSlotByIdChildren::_0),
            )
            .await?;
        let slot_by_hash = self
            .client
            .get_slot_by_id(
                &types::IntOrHash::Hash(slot_with_children.hash.clone()),
                None,
            )
            .await?;
        let slot_by_hash_with_children = self
            .client
            .get_slot_by_id(
                &types::IntOrHash::Hash(slot_with_children.hash.clone()),
                Some(GetSlotByIdChildren::_1),
            )
            .await?;

        for batch in slot_with_children.batches.iter() {
            let batch_by_hash = self.fetch_and_compare_batch(batch.number).await?;
            assert_eq!(batch, &batch_by_hash);
        }

        // Compare all variations for consistency
        self.compare_slot_variations(
            &slot_with_children,
            &slot_without_children,
            &slot_by_hash,
            &slot_by_hash_with_children,
            slot_number,
        )?;

        // Handle snapshot behavior
        match behavior {
            GetItemBehavior::SaveSnapshot => {
                save_slot_snapshot(&slot_with_children, &self.output_dir)?;
            }
            GetItemBehavior::CheckAgainstSnapshot => {
                validate_against_snapshot(
                    &slot_with_children,
                    &self.output_dir,
                    &format!("Fetched slot {}", slot_number),
                )?;
            }
            GetItemBehavior::DoNothing => {
                // Do nothing
            }
        }

        // Return the most complete version (with children)
        Ok(slot_with_children.into_inner())
    }

    fn compare_slot_variations(
        &self,
        slot_with_children: &Slot,
        slot_without_children: &Slot,
        slot_by_hash: &Slot,
        slot_by_hash_with_children: &Slot,
        slot_number: u64,
    ) -> Result<(), anyhow::Error> {
        let description_prefix = format!("Slot {}", slot_number);

        // Compare slots fetched by number vs by hash (excluding batches)
        assert_slots_match_excluding_batches(
            slot_with_children,
            slot_by_hash_with_children,
            &format!(
                "{}: by number vs by hash (with children)",
                description_prefix
            ),
        );
        assert_eq!(
            slot_by_hash_with_children.batches, slot_with_children.batches,
            "{}: batches should match",
            description_prefix
        );
        assert_slots_match_excluding_batches(
            slot_without_children,
            slot_by_hash,
            &format!(
                "{}: by number vs by hash (without children)",
                description_prefix
            ),
        );
        assert_slots_match_excluding_batches(
            slot_with_children,
            slot_without_children,
            &format!("{}: by hash vs by hash with children", description_prefix),
        );

        // Compare the slots as JSON as well to be extra safe
        assert_slots_match_json_excluding_batches(
            slot_with_children,
            slot_by_hash_with_children,
            &format!(
                "{}: JSON by number vs by hash (with children)",
                description_prefix
            ),
        )?;
        assert_slots_match_json_excluding_batches(
            slot_without_children,
            slot_by_hash,
            &format!(
                "{}: JSON by number vs by hash (without children)",
                description_prefix
            ),
        )?;
        assert_slots_match_json_excluding_batches(
            slot_with_children,
            slot_without_children,
            &format!(
                "{}: JSON with vs without children (by number)",
                description_prefix
            ),
        )?;

        Ok(())
    }
}
