use crate::hash::{Hash, hashv};

// Agave prefixes leaves and inner nodes differently to block second-preimage attacks.
const LEAF_PREFIX: &[u8] = &[0];
const INTERMEDIATE_PREFIX: &[u8] = &[1];

/// Merkle root of a batch's signatures: the mixin Agave records for a batch of
/// transactions (`solana_entry::entry::hash_signatures`). An empty batch gives
/// the all-zero hash, and an odd node is paired with itself.
pub fn hash_signatures<T: AsRef<[u8]>>(signatures: &[T]) -> Hash {
    let mut level: Vec<Hash> = signatures
        .iter()
        .map(|signature| hashv(&[LEAF_PREFIX, signature.as_ref()]))
        .collect();
    while level.len() > 1 {
        level = level
            .chunks(2)
            .map(|pair| {
                let (left, right) = (&pair[0], &pair[pair.len() - 1]);
                hashv(&[INTERMEDIATE_PREFIX, left.as_ref(), right.as_ref()])
            })
            .collect();
    }
    level.first().copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    // Vectors from Agave's merkle-tree/src/merkle_tree.rs tests.
    use super::*;

    #[test]
    fn empty_batch_is_default_hash() {
        assert_eq!(hash_signatures::<&[u8]>(&[]), Hash::default());
    }

    #[test]
    fn single_leaf_is_its_leaf_hash() {
        assert_eq!(hash_signatures(&[b"test"]), hashv(&[LEAF_PREFIX, b"test"]));
    }

    #[test]
    fn matches_agave_root() {
        let words: [&[u8]; 11] = [
            b"my", b"very", b"eager", b"mother", b"just", b"served", b"us", b"nine", b"pizzas",
            b"make", b"prime",
        ];
        assert_eq!(
            hash_signatures(&words).to_string(),
            "b40c847546fdceea166f927fc46c5ca33c3638236a36275c1346d3dffb84e1bc"
        );
    }
}
