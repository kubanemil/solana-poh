//! Solana-compatible Proof of History: a generator with the same state machine
//! as Agave's `Poh`, and sequential and parallel verifiers that report the first
//! bad entry.
//!
//! ```
//! use poh_core::{Hash, Poh, hash, verify_parallel};
//!
//! let start = Hash::default();
//! let mut poh = Poh::new(start, Some(8));
//! let mut entries = Vec::new();
//! poh.hash(3);
//! entries.extend(poh.record(hash(b"tx batch")));
//! assert!(poh.hash(u64::MAX));
//! entries.extend(poh.tick());
//!
//! assert_eq!(entries.iter().map(|e| e.num_hashes).collect::<Vec<_>>(), [4, 4]);
//! assert!(verify_parallel(&start, &entries).is_ok());
//! ```
#![forbid(unsafe_code)]

mod hash;
mod merkle;
mod poh;
mod verify;

pub use hash::{HASH_BYTES, Hash, ParseHashError, hash, hash_n, hashv};
pub use merkle::hash_signatures;
pub use poh::{Poh, PohEntry};
pub use verify::{
    VerifyError, VerifyErrorKind, next_entry, next_hash, verify, verify_parallel,
    verify_tick_hash_count,
};
