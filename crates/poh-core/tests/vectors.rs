#![allow(clippy::unwrap_used)]

use poh_core::{
    Hash, Poh, PohEntry, hash, hash_n, hash_signatures, verify, verify_parallel,
    verify_tick_hash_counts,
};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../vectors/poh.json");

fn vectors() -> Value {
    serde_json::from_str(VECTORS).unwrap()
}

fn parse_hash(value: &Value) -> Hash {
    value.as_str().unwrap().parse().unwrap()
}

fn parse_entry(value: &Value) -> PohEntry {
    PohEntry {
        num_hashes: value["num_hashes"].as_u64().unwrap(),
        hash: parse_hash(&value["hash"]),
        mixin: (!value["mixin"].is_null()).then(|| parse_hash(&value["mixin"])),
    }
}

#[test]
fn hash_chains() {
    for chain in vectors()["chains"].as_array().unwrap() {
        let start = parse_hash(&chain["start"]);
        let n = chain["num_hashes"].as_u64().unwrap();
        let expected = parse_hash(&chain["hash"]);
        assert_eq!(hash_n(&start, n), expected, "hash_n({start}, {n})");
        if n <= 1_000 {
            let naive = (0..n).fold(start, |h, _| hash(h.as_ref()));
            assert_eq!(naive, expected);
        }
    }
}

#[test]
fn sequences_verify() {
    for sequence in vectors()["sequences"].as_array().unwrap() {
        let name = sequence["name"].as_str().unwrap();
        let start = parse_hash(&sequence["start"]);
        let entries: Vec<_> = sequence["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(parse_entry)
            .collect();
        assert_eq!(verify(&start, &entries), Ok(()), "{name}");
        assert_eq!(verify_parallel(&start, &entries), Ok(()), "{name}");
        if let Some(hashes_per_tick) = sequence["hashes_per_tick"].as_u64() {
            assert_eq!(
                verify_tick_hash_counts(&entries, hashes_per_tick),
                Ok(()),
                "{name}"
            );
        }
    }
}

#[test]
fn sequences_replay_through_poh() {
    for sequence in vectors()["sequences"].as_array().unwrap() {
        let name = sequence["name"].as_str().unwrap();
        let start = parse_hash(&sequence["start"]);
        let mut poh = Poh::new(start, sequence["hashes_per_tick"].as_u64());
        for (index, value) in sequence["entries"].as_array().unwrap().iter().enumerate() {
            let expected = parse_entry(value);
            let at_tick_boundary = poh.hash(expected.num_hashes - 1);
            let produced = match expected.mixin {
                Some(mixin) => {
                    assert!(
                        !at_tick_boundary,
                        "{name}[{index}]: record at a tick boundary"
                    );
                    poh.record(mixin)
                }
                None => poh.tick(),
            };
            assert_eq!(produced, Some(expected), "{name}[{index}]");
        }
        assert_eq!(poh.num_hashes(), 0, "{name}");
    }
}

#[test]
fn signature_merkle_roots() {
    for case in vectors()["hash_signatures"].as_array().unwrap() {
        let leaves: Vec<Vec<u8>> = case["leaves"]
            .as_array()
            .unwrap()
            .iter()
            .map(|leaf| {
                let hex = leaf.as_str().unwrap();
                (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect()
            })
            .collect();
        assert_eq!(hash_signatures(&leaves), parse_hash(&case["root"]));
    }
}
