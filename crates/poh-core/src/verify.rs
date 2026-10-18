use std::fmt;

use rayon::prelude::*;

use crate::{
    hash::{Hash, hash_n, hashv},
    poh::PohEntry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyError {
    /// Index of the first entry that failed.
    pub index: usize,
    pub kind: VerifyErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyErrorKind {
    /// The entry's hash is not what its `num_hashes` and mixin produce from the previous hash.
    HashMismatch { computed: Hash },
    /// A mixin entry with `num_hashes == 0`. `Poh::record` always spends one hash.
    ZeroHashMixin,
    /// A tick with `num_hashes == 0`.
    ZeroHashTick,
    /// A tick that does not close exactly `hashes_per_tick` hashes.
    TickHashCount { expected: u64, actual: u64 },
    /// A mixin entry that takes the last hash of a tick, which `Poh::record` refuses.
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
            Self::MissedTick => f.write_str("mixin entry takes the hash reserved for a tick"),
        }
    }
}

impl std::error::Error for VerifyError {}

/// The hash an entry with `num_hashes` and `mixin` must carry after `prev`,
/// as Agave's `next_hash` computes it.
pub fn next_hash(prev: &Hash, num_hashes: u64, mixin: Option<&Hash>) -> Hash {
    match mixin {
        None => hash_n(prev, num_hashes),
        Some(mixin) => {
            let before = hash_n(prev, num_hashes.saturating_sub(1));
            hashv(&[before.as_ref(), mixin.as_ref()])
        }
    }
}

/// Builds the entry `num_hashes` hashes after `prev`.
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

/// Checks that `entries` continue the hash chain from `start`, one entry after another.
pub fn verify(start: &Hash, entries: &[PohEntry]) -> Result<(), VerifyError> {
    let mut prev = start;
    for (index, entry) in entries.iter().enumerate() {
        check_entry(prev, entry).map_err(|kind| VerifyError { index, kind })?;
        prev = &entry.hash;
    }
    Ok(())
}

/// Same result as [`verify`], with entries checked in parallel on the current rayon pool.
///
/// Each entry only needs the previous entry's hash, so the work splits at entry
/// boundaries, as in Agave's `EntrySlice::verify`.
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

/// Checks that every tick closes exactly `hashes_per_tick` hashes, like Agave's
/// `verify_tick_hash_count`. The sequence may end partway through a tick.
pub fn verify_tick_hash_counts(
    entries: &[PohEntry],
    hashes_per_tick: u64,
) -> Result<(), VerifyError> {
    let mut count: u64 = 0;
    for (index, entry) in entries.iter().enumerate() {
        let fail = |kind| Err(VerifyError { index, kind });
        count = count.saturating_add(entry.num_hashes);
        if entry.is_tick() {
            if entry.num_hashes == 0 {
                return fail(VerifyErrorKind::ZeroHashTick);
            }
            if count != hashes_per_tick {
                return fail(VerifyErrorKind::TickHashCount {
                    expected: hashes_per_tick,
                    actual: count,
                });
            }
            count = 0;
        } else if count >= hashes_per_tick {
            return fail(VerifyErrorKind::MissedTick);
        }
    }
    Ok(())
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
    fn verify_tick_hash_count() {
        let hashes_per_tick = 10;
        let check = |entries: &[PohEntry], hashes_per_tick| {
            verify_tick_hash_counts(entries, hashes_per_tick).map_err(|e| (e.index, e.kind))
        };
        let full_tick = tick(hashes_per_tick, Hash::default());

        assert_eq!(check(&[], hashes_per_tick), Ok(()));
        assert_eq!(
            check(
                &[tick(hashes_per_tick - 1, Hash::default())],
                hashes_per_tick
            ),
            Err((
                0,
                VerifyErrorKind::TickHashCount {
                    expected: 10,
                    actual: 9
                }
            ))
        );
        assert_eq!(check(&[mixin_entry(0), full_tick], hashes_per_tick), Ok(()));
        assert!(check(&[mixin_entry(0), full_tick], hashes_per_tick - 1).is_err());
        assert_eq!(
            check(&[mixin_entry(hashes_per_tick - 1)], hashes_per_tick),
            Ok(())
        );
        assert_eq!(
            check(&[tick(0, Hash::default())], hashes_per_tick),
            Err((0, VerifyErrorKind::ZeroHashTick))
        );
        assert_eq!(
            check(
                &[mixin_entry(hashes_per_tick), tick(0, Hash::default())],
                hashes_per_tick
            ),
            Err((0, VerifyErrorKind::MissedTick))
        );
        assert_eq!(
            check(&[mixin_entry(hashes_per_tick)], hashes_per_tick),
            Err((0, VerifyErrorKind::MissedTick))
        );
        assert_eq!(
            check(&[mixin_entry(9), tick(2, Hash::default())], hashes_per_tick),
            Err((
                1,
                VerifyErrorKind::TickHashCount {
                    expected: 10,
                    actual: 11
                }
            ))
        );
        assert_eq!(
            check(&[mixin_entry(1), tick(9, Hash::default())], hashes_per_tick),
            Ok(())
        );
        let mut many = vec![mixin_entry(1); 9];
        many.push(tick(1, Hash::default()));
        assert_eq!(check(&many, hashes_per_tick), Ok(()));
        assert_eq!(
            check(
                &[full_tick, tick(u64::MAX, Hash::default())],
                hashes_per_tick
            ),
            Err((
                1,
                VerifyErrorKind::TickHashCount {
                    expected: 10,
                    actual: u64::MAX
                }
            ))
        );
        assert_eq!(
            check(&[mixin_entry(u64::MAX), full_tick], hashes_per_tick),
            Err((0, VerifyErrorKind::MissedTick))
        );
    }
}
