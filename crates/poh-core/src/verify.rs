use std::fmt;

use rayon::prelude::*;

use crate::{
    hash::{Hash, hash_n, hashv},
    poh::PohEntry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyError {
    pub index: usize,
    pub kind: VerifyErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyErrorKind {
    HashMismatch {
        computed: Hash,
    },
    /// Agave hashes this like `num_hashes == 1`; we reject it because `Poh::record`
    /// never produces it and accepting it lets `num_hashes` change without the hash changing.
    ZeroHashMixin,
    ZeroHashTick,
    TickHashCount {
        expected: u64,
        actual: u64,
    },
    /// The slice ends with `hashes_per_tick` or more hashes since the last tick.
    MissedTick,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "entry {}: {}", self.index, self.kind)
    }
}

impl fmt::Display for VerifyErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HashMismatch { computed } => {
                write!(
                    f,
                    "hash does not follow from the previous entry (computed {computed})"
                )
            }
            Self::ZeroHashMixin => f.write_str("mixin entry has num_hashes = 0"),
            Self::ZeroHashTick => f.write_str("tick has num_hashes = 0"),
            Self::TickHashCount { expected, actual } => {
                write!(f, "tick closes {actual} hashes, expected {expected}")
            }
            Self::MissedTick => f.write_str("a tick's worth of hashes without a tick"),
        }
    }
}

impl std::error::Error for VerifyError {}

/// Agave's `next_hash`, with a mixin in place of the transactions.
pub fn next_hash(prev: &Hash, num_hashes: u64, mixin: Option<&Hash>) -> Hash {
    match mixin {
        None => hash_n(prev, num_hashes),
        Some(mixin) => {
            let before = hash_n(prev, num_hashes.saturating_sub(1));
            hashv(&[before.as_ref(), mixin.as_ref()])
        }
    }
}

pub fn next_entry(prev: &Hash, num_hashes: u64, mixin: Option<Hash>) -> PohEntry {
    PohEntry {
        num_hashes,
        hash: next_hash(prev, num_hashes, mixin.as_ref()),
        mixin,
    }
}

fn check_entry(prev: &Hash, entry: &PohEntry) -> Result<(), VerifyErrorKind> {
    if entry.num_hashes == 0 && entry.mixin.is_some() {
        return Err(VerifyErrorKind::ZeroHashMixin);
    }
    let computed = next_hash(prev, entry.num_hashes, entry.mixin.as_ref());
    if computed == entry.hash {
        Ok(())
    } else {
        Err(VerifyErrorKind::HashMismatch { computed })
    }
}

pub fn verify(start: &Hash, entries: &[PohEntry]) -> Result<(), VerifyError> {
    let mut prev = start;
    for (index, entry) in entries.iter().enumerate() {
        check_entry(prev, entry).map_err(|kind| VerifyError { index, kind })?;
        prev = &entry.hash;
    }
    Ok(())
}

/// Same result as [`verify`], on the current rayon pool. Each entry only needs the
/// previous entry's hash, so work splits at entry boundaries, as in Agave's `EntrySlice::verify`.
pub fn verify_parallel(start: &Hash, entries: &[PohEntry]) -> Result<(), VerifyError> {
    let failure = (0..entries.len()).into_par_iter().find_map_first(|index| {
        let prev = match index {
            0 => start,
            _ => &entries[index - 1].hash,
        };
        check_entry(prev, &entries[index])
            .err()
            .map(|kind| VerifyError { index, kind })
    });
    failure.map_or(Ok(()), Err)
}

/// Agave's `verify_tick_hash_count`, accepting and rejecting the same slices.
/// `tick_hash_count` carries hashes since the last tick across slices, and
/// `hashes_per_tick == 0` disables the check. A `MissedTick` error points at the
/// entry that reached a tick's worth of hashes, or `entries.len()` if the count
/// was already there.
pub fn verify_tick_hash_count(
    entries: &[PohEntry],
    tick_hash_count: &mut u64,
    hashes_per_tick: u64,
) -> Result<(), VerifyError> {
    if hashes_per_tick == 0 {
        return Ok(());
    }
    let mut reached = None;
    for (index, entry) in entries.iter().enumerate() {
        *tick_hash_count = tick_hash_count.saturating_add(entry.num_hashes);
        if entry.is_tick() {
            let kind = if entry.num_hashes == 0 {
                VerifyErrorKind::ZeroHashTick
            } else if *tick_hash_count != hashes_per_tick {
                VerifyErrorKind::TickHashCount {
                    expected: hashes_per_tick,
                    actual: *tick_hash_count,
                }
            } else {
                *tick_hash_count = 0;
                reached = None;
                continue;
            };
            return Err(VerifyError { index, kind });
        }
        if reached.is_none() && *tick_hash_count >= hashes_per_tick {
            reached = Some(index);
        }
    }
    if *tick_hash_count < hashes_per_tick {
        Ok(())
    } else {
        Err(VerifyError {
            index: reached.unwrap_or(entries.len()),
            kind: VerifyErrorKind::MissedTick,
        })
    }
}

#[cfg(test)]
mod tests {
    // Cases ported from Agave's entry/src/entry.rs tests.
    use super::*;
    use crate::hash::hash;

    fn tick(num_hashes: u64, hash: Hash) -> PohEntry {
        PohEntry {
            num_hashes,
            hash,
            mixin: None,
        }
    }

    fn mixin_entry(num_hashes: u64) -> PohEntry {
        PohEntry {
            num_hashes,
            hash: Hash::default(),
            mixin: Some(Hash::default()),
        }
    }

    fn both(start: &Hash, entries: &[PohEntry]) -> Result<(), VerifyError> {
        let sequential = verify(start, entries);
        assert_eq!(verify_parallel(start, entries), sequential);
        sequential
    }

    #[test]
    fn entry_verify() {
        let zero = Hash::default();
        let one = hash(zero.as_ref());
        assert!(both(&zero, &[tick(0, zero)]).is_ok());
        assert!(both(&one, &[tick(0, zero)]).is_err());
        assert!(both(&zero, &[next_entry(&zero, 1, None)]).is_ok());
        assert!(both(&one, &[next_entry(&zero, 1, None)]).is_err());
    }

    #[test]
    fn next_entry_hashes() {
        let zero = Hash::default();
        let entry = next_entry(&zero, 1, None);
        assert_eq!(entry.num_hashes, 1);
        assert_ne!(entry.hash, zero);
        assert_eq!(next_entry(&zero, 0, None).hash, zero);
        let mixin = hash(b"tx");
        assert_eq!(
            next_entry(&zero, 1, Some(mixin)).hash,
            hashv(&[zero.as_ref(), mixin.as_ref()])
        );
    }

    #[test]
    fn verify_slice() {
        let zero = Hash::default();
        let one = hash(zero.as_ref());
        assert!(both(&zero, &[]).is_ok());
        assert!(both(&zero, &[tick(0, zero)]).is_ok());
        assert!(both(&one, &[tick(0, zero)]).is_err());
        assert!(both(&zero, &[next_entry(&zero, 0, None); 2]).is_ok());

        let mut bad = [next_entry(&zero, 0, None); 2];
        bad[1].hash = one;
        assert_eq!(both(&zero, &bad).map_err(|e| e.index), Err(1));
    }

    #[test]
    fn verify_slice_with_hashes() {
        let zero = Hash::default();
        let one = hash(zero.as_ref());
        let two = hash(one.as_ref());
        assert!(both(&one, &[]).is_ok());
        assert!(both(&one, &[tick(1, two)]).is_ok());
        assert!(both(&two, &[tick(1, two)]).is_err());

        let first = next_entry(&one, 1, None);
        let ticks = [first, next_entry(&first.hash, 1, None)];
        assert!(both(&one, &ticks).is_ok());

        let mut bad = ticks;
        bad[1].hash = one;
        assert_eq!(both(&one, &bad).map_err(|e| e.index), Err(1));
    }

    #[test]
    fn verify_slice_with_mixins() {
        let zero = Hash::default();
        let one = hash(zero.as_ref());
        let two = hash(one.as_ref());
        let (tx0, tx1) = (hash(b"tx0"), hash(b"tx1"));
        assert!(both(&one, &[next_entry(&one, 1, Some(tx0))]).is_ok());
        assert!(both(&two, &[next_entry(&one, 1, Some(tx0))]).is_err());

        let first = next_entry(&one, 1, Some(tx0));
        let entries = [first, next_entry(&first.hash, 1, Some(tx1))];
        assert!(both(&one, &entries).is_ok());

        let mut bad = entries;
        bad[1].hash = one;
        assert_eq!(both(&one, &bad).map_err(|e| e.index), Err(1));
    }

    #[test]
    fn zero_hash_mixin_is_rejected() {
        let zero = Hash::default();
        let mut entry = next_entry(&zero, 1, Some(zero));
        entry.num_hashes = 0;
        assert_eq!(
            both(&zero, &[entry]),
            Err(VerifyError {
                index: 0,
                kind: VerifyErrorKind::ZeroHashMixin
            })
        );
    }

    #[test]
    fn reports_first_failure_and_computed_hash() {
        let zero = Hash::default();
        let mut entries: Vec<_> = (0..64)
            .scan(zero, |prev, _| {
                let entry = next_entry(prev, 3, None);
                *prev = entry.hash;
                Some(entry)
            })
            .collect();
        let expected = entries[40].hash;
        entries[40].hash = zero;
        entries[50].hash = zero;
        assert_eq!(
            both(&zero, &entries),
            Err(VerifyError {
                index: 40,
                kind: VerifyErrorKind::HashMismatch { computed: expected }
            })
        );
    }

    #[test]
    fn verify_tick_hash_count_matches_agave() {
        // Line for line from Agave's test_verify_tick_hash_count.
        let hashes_per_tick = 10;
        let no_hash_tx_entry = mixin_entry(0);
        let single_hash_tx_entry = mixin_entry(1);
        let partial_tx_entry = mixin_entry(hashes_per_tick - 1);
        let full_tx_entry = mixin_entry(hashes_per_tick);
        let max_hash_tx_entry = mixin_entry(u64::MAX);
        let no_hash_tick_entry = tick(0, Hash::default());
        let single_hash_tick_entry = tick(1, Hash::default());
        let partial_tick_entry = tick(hashes_per_tick - 1, Hash::default());
        let full_tick_entry = tick(hashes_per_tick, Hash::default());
        let max_hash_tick_entry = tick(u64::MAX, Hash::default());
        let check = |entries: &[PohEntry], count: &mut u64, hashes_per_tick| {
            verify_tick_hash_count(entries, count, hashes_per_tick).is_ok()
        };

        let mut tick_hash_count = 0;
        let mut entries: Vec<PohEntry> = vec![];
        assert!(check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, 0);

        tick_hash_count = hashes_per_tick;
        assert!(!check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, hashes_per_tick);
        tick_hash_count = 0;

        entries = vec![max_hash_tx_entry];
        assert!(check(&entries, &mut tick_hash_count, 0));
        assert_eq!(tick_hash_count, 0);

        entries = vec![partial_tick_entry];
        assert!(!check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, hashes_per_tick - 1);
        tick_hash_count = 0;

        entries = vec![no_hash_tx_entry, full_tick_entry];
        assert!(check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, 0);

        assert!(!check(&entries, &mut tick_hash_count, hashes_per_tick - 1));
        assert_eq!(tick_hash_count, hashes_per_tick);
        tick_hash_count = 0;

        entries = vec![partial_tx_entry];
        assert!(check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, hashes_per_tick - 1);
        tick_hash_count = 0;

        entries = vec![no_hash_tick_entry];
        assert!(!check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, 0);

        entries = vec![full_tx_entry, no_hash_tick_entry];
        assert!(!check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, hashes_per_tick);
        tick_hash_count = 0;

        entries = vec![full_tx_entry, single_hash_tick_entry];
        assert!(!check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, hashes_per_tick + 1);
        tick_hash_count = 0;

        entries = vec![full_tx_entry];
        assert!(!check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, hashes_per_tick);
        tick_hash_count = 0;

        entries = vec![single_hash_tx_entry, partial_tick_entry];
        assert!(check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, 0);

        let tx_entries = vec![single_hash_tx_entry; hashes_per_tick as usize - 1];
        entries = [tx_entries, vec![single_hash_tick_entry]].concat();
        assert!(check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, 0);

        entries = vec![full_tick_entry, max_hash_tick_entry];
        assert!(!check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, u64::MAX);
        tick_hash_count = 0;

        entries = vec![max_hash_tx_entry, full_tick_entry];
        assert!(!check(&entries, &mut tick_hash_count, hashes_per_tick));
        assert_eq!(tick_hash_count, u64::MAX);
    }

    #[test]
    fn tick_hash_count_errors_point_at_the_entry() {
        let check = |entries: &[PohEntry]| {
            verify_tick_hash_count(entries, &mut 0, 10).map_err(|e| (e.index, e.kind))
        };
        let full_tick = tick(10, Hash::default());
        assert_eq!(
            check(&[full_tick, mixin_entry(9), tick(2, Hash::default())]),
            Err((
                2,
                VerifyErrorKind::TickHashCount {
                    expected: 10,
                    actual: 11
                }
            ))
        );
        assert_eq!(
            check(&[full_tick, tick(0, Hash::default())]),
            Err((1, VerifyErrorKind::ZeroHashTick))
        );
        assert_eq!(
            check(&[full_tick, mixin_entry(4), mixin_entry(6), mixin_entry(1)]),
            Err((2, VerifyErrorKind::MissedTick))
        );
        assert_eq!(
            verify_tick_hash_count(&[], &mut 10, 10).map_err(|e| (e.index, e.kind)),
            Err((0, VerifyErrorKind::MissedTick))
        );
    }

    #[test]
    fn zero_hashes_per_tick_disables_the_check() {
        let entries = [tick(0, Hash::default()), mixin_entry(u64::MAX)];
        let mut count = 7;
        assert_eq!(verify_tick_hash_count(&entries, &mut count, 0), Ok(()));
        assert_eq!(count, 7);
    }
}
