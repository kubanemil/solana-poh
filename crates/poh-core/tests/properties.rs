#![allow(clippy::unwrap_used)]

use poh_core::{
    Hash, Poh, PohEntry, VerifyErrorKind, hash, hash_n, verify, verify_parallel,
    verify_tick_hash_count,
};
use proptest::{prelude::*, sample::Index};

#[derive(Debug, Clone)]
enum Op {
    Hash(u64),
    Record([u8; 32]),
    Tick,
}

#[derive(Debug, Clone)]
enum Tamper {
    HashBit(Index, u8),
    NumHashes(u64),
    MixinBit(Index, u8),
    ToggleMixin,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0u64..40).prop_map(Op::Hash),
        any::<[u8; 32]>().prop_map(Op::Record),
        Just(Op::Tick),
    ]
}

fn tamper() -> impl Strategy<Value = Tamper> {
    prop_oneof![
        (any::<Index>(), 0u8..8).prop_map(|(byte, bit)| Tamper::HashBit(byte, bit)),
        (0u64..4).prop_map(Tamper::NumHashes),
        (any::<Index>(), 0u8..8).prop_map(|(byte, bit)| Tamper::MixinBit(byte, bit)),
        Just(Tamper::ToggleMixin),
    ]
}

fn flip(hash: Hash, byte: &Index, bit: u8) -> Hash {
    let mut bytes = hash.to_bytes();
    bytes[byte.index(bytes.len())] ^= 1 << bit;
    Hash::new(bytes)
}

struct Run {
    poh: Poh,
    entries: Vec<PohEntry>,
}

/// Drives `Poh` the way Agave's `PohRecorder` does, checking the hash
/// accounting after every step.
fn run(start: Hash, hashes_per_tick: u64, ops: &[Op]) -> Run {
    let mut poh = Poh::new(start, Some(hashes_per_tick));
    let mut entries = Vec::new();
    for op in ops {
        match op {
            Op::Hash(n) => {
                let at_boundary = poh.hash(*n);
                assert_eq!(at_boundary, poh.remaining_hashes_until_tick() == 1);
            }
            Op::Record(mixin) => loop {
                let refuse = poh.remaining_hashes_until_tick() == 1;
                match poh.record(Hash::new(*mixin)) {
                    Some(entry) => {
                        assert!(!refuse);
                        entries.push(entry);
                        break;
                    }
                    None => {
                        assert!(refuse);
                        entries.push(poh.tick().unwrap());
                    }
                }
            },
            Op::Tick => entries.extend(poh.tick()),
        }
        let since_tick: u64 = entries
            .iter()
            .rev()
            .take_while(|e| !e.is_tick())
            .map(|e| e.num_hashes)
            .sum();
        let remaining = poh.remaining_hashes_until_tick();
        assert!((1..=hashes_per_tick).contains(&remaining));
        assert_eq!(hashes_per_tick - remaining, since_tick + poh.num_hashes());
    }
    Run { poh, entries }
}

fn generated() -> impl Strategy<Value = (Hash, u64, Vec<Op>)> {
    (
        any::<[u8; 32]>().prop_map(Hash::new),
        2u64..64,
        prop::collection::vec(op(), 1..60),
    )
}

proptest! {
    #[test]
    fn generated_entries_verify((start, hashes_per_tick, ops) in generated(), split in any::<Index>()) {
        let Run { poh, entries } = run(start, hashes_per_tick, &ops);
        prop_assert_eq!(verify(&start, &entries), Ok(()));
        prop_assert_eq!(verify_parallel(&start, &entries), Ok(()));

        let (head, tail) = entries.split_at(split.index(entries.len() + 1));
        let mut tick_hash_count = 0;
        prop_assert_eq!(verify_tick_hash_count(head, &mut tick_hash_count, hashes_per_tick), Ok(()));
        prop_assert_eq!(verify_tick_hash_count(tail, &mut tick_hash_count, hashes_per_tick), Ok(()));
        let since_tick = hashes_per_tick - poh.remaining_hashes_until_tick() - poh.num_hashes();
        prop_assert_eq!(tick_hash_count, since_tick);

        let last = entries.last().map_or(start, |e| e.hash);
        prop_assert_eq!(hash_n(&last, poh.num_hashes()), poh.current_hash());
    }

    #[test]
    fn tampering_fails_at_the_tampered_entry(
        (start, hashes_per_tick, ops) in generated(),
        at in any::<Index>(),
        tamper in tamper(),
    ) {
        let Run { mut entries, .. } = run(start, hashes_per_tick, &ops);
        prop_assume!(!entries.is_empty());
        let index = at.index(entries.len());
        let original = entries[index];
        let entry = &mut entries[index];
        match &tamper {
            Tamper::HashBit(byte, bit) => entry.hash = flip(entry.hash, byte, *bit),
            Tamper::NumHashes(n) => {
                entry.num_hashes = if *n == entry.num_hashes { n + 1 } else { *n };
            }
            Tamper::MixinBit(byte, bit) => {
                entry.mixin = Some(flip(entry.mixin.unwrap_or_default(), byte, *bit));
            }
            Tamper::ToggleMixin => {
                entry.mixin = match entry.mixin {
                    Some(_) => None,
                    None => Some(Hash::default()),
                };
            }
        }
        let sequential = verify(&start, &entries).unwrap_err();
        prop_assert_eq!(sequential.index, index);
        prop_assert_eq!(verify_parallel(&start, &entries), Err(sequential));
        if let Tamper::HashBit(..) = tamper {
            let computed = VerifyErrorKind::HashMismatch { computed: original.hash };
            prop_assert_eq!(sequential.kind, computed);
        }
    }

    #[test]
    fn hash_n_is_repeated_hash(seed in any::<[u8; 32]>(), n in 0u64..300) {
        let start = Hash::new(seed);
        let naive = (0..n).fold(start, |h, _| hash(h.as_ref()));
        prop_assert_eq!(hash_n(&start, n), naive);
    }
}
