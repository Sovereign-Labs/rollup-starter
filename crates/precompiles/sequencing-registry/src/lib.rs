//! Per-transaction sequencing data shared by multiple precompiles.
//!
//! The SDK gives each precompile one opaque byte blob for sequencing data and
//! one for the scratchpad. Both are modelled as a registry keyed by precompile
//! address so each precompile only reads and writes its own section.

use std::collections::BTreeMap;

use alloy_primitives::Address;
use borsh::{BorshDeserialize, BorshSerialize};
use bytes::Bytes;
use sov_modules_api::capabilities::SequencingDataTrait;
use sov_modules_api::HDTimestamp;

/// Registry of per-precompile sections. The BTreeMap keeps the borsh encoding
/// canonical so every node produces the same bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SequencingRegistry(pub BTreeMap<Address, Bytes>);

impl SequencingDataTrait for SequencingRegistry {
    fn get_maybe_timestamp(self) -> Option<HDTimestamp> {
        None
    }
}

impl SequencingRegistry {
    pub fn section<P: PrecompileSequencing>(&self) -> Result<Option<P::Data>, std::io::Error> {
        match self.0.get(&P::ADDRESS) {
            Some(bytes) => Ok(Some(P::Data::try_from_slice(bytes)?)),
            None => Ok(None),
        }
    }

    pub fn set_section<P: PrecompileSequencing>(&mut self, data: &P::Data) {
        let bytes = borsh::to_vec(data).expect("in-memory borsh serialization is infallible");
        self.0.insert(P::ADDRESS, Bytes::from(bytes));
    }
}

pub trait PrecompileSequencing {
    const ADDRESS: Address;
    type Data: BorshSerialize + BorshDeserialize;
    type Used: BorshSerialize + BorshDeserialize + Default;

    fn prune(data: Self::Data, used: Self::Used) -> Self::Data;

    fn is_empty(data: &Self::Data) -> bool;
}

#[cfg(feature = "native")]
pub fn record_used<S, P>(
    context: &sov_modules_api::Context<S>,
    update: impl FnOnce(&mut P::Used),
) -> Result<(), std::io::Error>
where
    S: sov_modules_api::Spec,
    P: PrecompileSequencing,
{
    context.sequencing_scratchpad().with_value(|slot| {
        let mut registry = match slot.as_deref() {
            Some(bytes) => SequencingRegistry::try_from_slice(bytes)?,
            None => SequencingRegistry::default(),
        };
        let mut used = match registry.0.get(&P::ADDRESS) {
            Some(bytes) => P::Used::try_from_slice(bytes)?,
            None => P::Used::default(),
        };
        update(&mut used);
        let used_bytes = borsh::to_vec(&used).expect("in-memory borsh serialization is infallible");
        registry.0.insert(P::ADDRESS, Bytes::from(used_bytes));
        let registry_bytes =
            borsh::to_vec(&registry).expect("in-memory borsh serialization is infallible");
        *slot = Some(Bytes::from(registry_bytes));
        Ok(())
    })
}

/// Prunes one section. Returns Ok(None) to drop the section.
pub type SectionPruner = fn(&[u8], Option<&[u8]>) -> Result<Option<Bytes>, std::io::Error>;

fn prune_section<P: PrecompileSequencing>(
    data: &[u8],
    used: Option<&[u8]>,
) -> Result<Option<Bytes>, std::io::Error> {
    let data = P::Data::try_from_slice(data)?;
    let used = match used {
        Some(bytes) => P::Used::try_from_slice(bytes)?,
        None => P::Used::default(),
    };
    let pruned = P::prune(data, used);
    if P::is_empty(&pruned) {
        Ok(None)
    } else {
        let bytes = borsh::to_vec(&pruned).expect("in-memory borsh serialization is infallible");
        Ok(Some(Bytes::from(bytes)))
    }
}

pub fn pruner<P: PrecompileSequencing>() -> (Address, SectionPruner) {
    (P::ADDRESS, prune_section::<P>)
}

/// Keeps only the entries each precompile recorded as used and drops the rest.
///
/// On any decoding uncertainty it keeps more data rather than less so that
/// replay from the DA layer can never miss an entry that execution read.
pub fn finalize(
    data: SequencingRegistry,
    scratchpad: Option<Bytes>,
    pruners: &[(Address, SectionPruner)],
) -> SequencingRegistry {
    let Some(scratchpad) = scratchpad else {
        return SequencingRegistry::default();
    };
    let used = match SequencingRegistry::try_from_slice(&scratchpad) {
        Ok(used) => used,
        Err(err) => {
            tracing::error!(%err, "sequencing scratchpad is malformed; publishing full sequencing data");
            return data;
        }
    };

    let mut out = BTreeMap::new();
    for (addr, data_section) in data.0 {
        let Some((_, prune)) = pruners.iter().find(|(a, _)| *a == addr) else {
            tracing::warn!(%addr, "no pruner registered for sequencing section; keeping it in full");
            out.insert(addr, data_section);
            continue;
        };
        let used_section = used.0.get(&addr).map(|bytes| bytes.as_ref());
        match prune(&data_section, used_section) {
            Ok(Some(pruned)) => {
                out.insert(addr, pruned);
            }
            Ok(None) => {}
            Err(err) => {
                tracing::error!(%addr, %err, "could not prune sequencing section; keeping it in full");
                out.insert(addr, data_section);
            }
        }
    }
    SequencingRegistry(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    struct MockData(BTreeMap<u8, Vec<u8>>);

    #[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
    struct MockUsed(Vec<u8>);

    struct MockA;
    impl PrecompileSequencing for MockA {
        const ADDRESS: Address = Address::new([0xAA; 20]);
        type Data = MockData;
        type Used = MockUsed;

        fn prune(mut data: MockData, used: MockUsed) -> MockData {
            let keep: std::collections::BTreeSet<u8> = used.0.into_iter().collect();
            data.0.retain(|k, _| keep.contains(k));
            data
        }

        fn is_empty(data: &MockData) -> bool {
            data.0.is_empty()
        }
    }

    const UNKNOWN_ADDRESS: Address = Address::new([0xBB; 20]);

    fn mock_data(keys: &[u8]) -> MockData {
        MockData(keys.iter().map(|k| (*k, vec![*k])).collect())
    }

    fn data_with_mock(keys: &[u8]) -> SequencingRegistry {
        let mut registry = SequencingRegistry::default();
        registry.set_section::<MockA>(&mock_data(keys));
        registry
    }

    fn scratchpad_with_used(used: &[u8]) -> Bytes {
        let mut registry = SequencingRegistry::default();
        registry.0.insert(
            MockA::ADDRESS,
            Bytes::from(borsh::to_vec(&MockUsed(used.to_vec())).unwrap()),
        );
        Bytes::from(borsh::to_vec(&registry).unwrap())
    }

    #[test]
    fn none_scratchpad_drops_all_sections() {
        let finalized = finalize(data_with_mock(&[1, 2]), None, &[pruner::<MockA>()]);
        assert_eq!(finalized, SequencingRegistry::default());
    }

    #[test]
    fn malformed_scratchpad_keeps_full_registry() {
        let data = data_with_mock(&[1, 2]);
        let finalized = finalize(
            data.clone(),
            Some(Bytes::from(vec![0xff, 0xff, 0xff, 0xff])),
            &[pruner::<MockA>()],
        );
        assert_eq!(finalized, data);
    }

    #[test]
    fn prunes_section_to_used_subset() {
        let finalized = finalize(
            data_with_mock(&[1, 2, 3]),
            Some(scratchpad_with_used(&[2])),
            &[pruner::<MockA>()],
        );
        let section = finalized.section::<MockA>().unwrap().unwrap();
        assert_eq!(section, mock_data(&[2]));
    }

    #[test]
    fn section_pruned_to_empty_is_dropped() {
        let finalized = finalize(
            data_with_mock(&[1, 2]),
            Some(scratchpad_with_used(&[])),
            &[pruner::<MockA>()],
        );
        assert_eq!(finalized, SequencingRegistry::default());
    }

    #[test]
    fn section_without_registered_pruner_is_kept_in_full() {
        let mut data = SequencingRegistry::default();
        data.0
            .insert(UNKNOWN_ADDRESS, Bytes::from(vec![1, 2, 3, 4]));
        let scratchpad = Bytes::from(borsh::to_vec(&SequencingRegistry::default()).unwrap());

        let finalized = finalize(data.clone(), Some(scratchpad), &[pruner::<MockA>()]);
        assert_eq!(finalized, data);
    }

    #[test]
    fn prunes_known_section_while_keeping_unknown_section() {
        let mut data = data_with_mock(&[1, 2]);
        data.0.insert(UNKNOWN_ADDRESS, Bytes::from(vec![9, 9, 9]));

        let finalized = finalize(data, Some(scratchpad_with_used(&[1])), &[pruner::<MockA>()]);

        assert_eq!(
            finalized.section::<MockA>().unwrap().unwrap(),
            mock_data(&[1])
        );
        assert_eq!(
            finalized.0.get(&UNKNOWN_ADDRESS).map(|b| b.as_ref()),
            Some([9, 9, 9].as_slice()),
            "unknown precompile's section must be preserved in full"
        );
    }
}
