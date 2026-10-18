use crate::hash::{Hash, hash, hash_n, hashv};

// Agave's sentinel for `hashes_per_tick: None`: every `tick()` closes a tick.
const LOW_POWER_MODE: u64 = u64::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PohEntry {
    pub num_hashes: u64,
    pub hash: Hash,
    pub mixin: Option<Hash>,
}

impl PohEntry {
    pub const fn is_tick(&self) -> bool {
        self.mixin.is_none()
    }
}

/// The state machine of Agave's `solana_entry::poh::Poh`.
#[derive(Debug, Clone)]
pub struct Poh {
    hash: Hash,
    num_hashes: u64,
    hashes_per_tick: u64,
    remaining_hashes_until_tick: u64,
    tick_number: u64,
}

impl Poh {
    /// `hashes_per_tick: None` is Agave's low-power mode, where every `tick()` emits a tick.
    ///
    /// # Panics
    ///
    /// If `hashes_per_tick` is `Some(0)` or `Some(1)`: a tick needs one hash of
    /// its own and at least one before it.
    pub fn new(hash: Hash, hashes_per_tick: Option<u64>) -> Self {
        Self::new_with_tick_number(hash, hashes_per_tick, 0)
    }

    pub fn new_with_tick_number(
        hash: Hash,
        hashes_per_tick: Option<u64>,
        tick_number: u64,
    ) -> Self {
        let hashes_per_tick = hashes_per_tick.unwrap_or(LOW_POWER_MODE);
        assert!(
            hashes_per_tick > 1,
            "hashes_per_tick must be greater than 1"
        );
        Self {
            hash,
            num_hashes: 0,
            hashes_per_tick,
            remaining_hashes_until_tick: hashes_per_tick,
            tick_number,
        }
    }

    pub fn reset(&mut self, hash: Hash, hashes_per_tick: Option<u64>) {
        *self = Self::new(hash, hashes_per_tick);
    }

    /// Hashes up to `max_num_hashes` times, stopping one hash short of the next
    /// tick. Returns `true` when that point is reached and the caller must `tick()`.
    pub fn hash(&mut self, max_num_hashes: u64) -> bool {
        let num_hashes = max_num_hashes.min(self.remaining_hashes_until_tick - 1);
        self.hash = hash_n(&self.hash, num_hashes);
        self.num_hashes += num_hashes;
        self.remaining_hashes_until_tick -= num_hashes;
        self.remaining_hashes_until_tick == 1
    }

    /// Mixes `mixin` into the next hash and returns the entry for it. Returns
    /// `None`, changing nothing, when the next hash belongs to a tick.
    pub fn record(&mut self, mixin: Hash) -> Option<PohEntry> {
        if self.remaining_hashes_until_tick == 1 {
            return None;
        }
        self.hash = hashv(&[self.hash.as_ref(), mixin.as_ref()]);
        let num_hashes = self.num_hashes + 1;
        self.num_hashes = 0;
        self.remaining_hashes_until_tick -= 1;
        Some(PohEntry {
            num_hashes,
            hash: self.hash,
            mixin: Some(mixin),
        })
    }

    pub fn tick(&mut self) -> Option<PohEntry> {
        self.hash = hash(self.hash.as_ref());
        self.num_hashes += 1;
        self.remaining_hashes_until_tick -= 1;
        if self.hashes_per_tick != LOW_POWER_MODE && self.remaining_hashes_until_tick != 0 {
            return None;
        }
        let num_hashes = self.num_hashes;
        self.remaining_hashes_until_tick = self.hashes_per_tick;
        self.num_hashes = 0;
        self.tick_number += 1;
        Some(PohEntry {
            num_hashes,
            hash: self.hash,
            mixin: None,
        })
    }

    pub const fn current_hash(&self) -> Hash {
        self.hash
    }

    pub const fn num_hashes(&self) -> u64 {
        self.num_hashes
    }

    pub const fn hashes_per_tick(&self) -> Option<u64> {
        if self.hashes_per_tick == LOW_POWER_MODE {
            None
        } else {
            Some(self.hashes_per_tick)
        }
    }

    pub const fn remaining_hashes_until_tick(&self) -> u64 {
        self.remaining_hashes_until_tick
    }

    pub const fn tick_number(&self) -> u64 {
        self.tick_number
    }

    /// Hashes left until the end of the current slot, counting the current tick.
    /// Saturates where Agave's wrapping arithmetic would overflow (low-power mode).
    ///
    /// # Panics
    ///
    /// If `ticks_per_slot` is 0.
    pub fn remaining_hashes_in_slot(&self, ticks_per_slot: u64) -> u64 {
        ticks_per_slot
            .saturating_sub(self.tick_number % ticks_per_slot + 1)
            .saturating_mul(self.hashes_per_tick)
            .saturating_add(self.remaining_hashes_until_tick)
    }
}

#[cfg(test)]
mod tests {
    // Ported from Agave's entry/src/poh.rs tests.
    use super::*;
    use crate::verify::verify;

    #[test]
    fn poh_verify() {
        let zero = Hash::default();
        let one = hash(zero.as_ref());
        let two = hash(one.as_ref());
        let one_with_zero = hashv(&[zero.as_ref(), zero.as_ref()]);
        let tick = |num_hashes, hash| PohEntry {
            num_hashes,
            hash,
            mixin: None,
        };

        let mut poh = Poh::new(zero, None);
        let entries = [
            poh.tick().unwrap(),
            poh.record(zero).unwrap(),
            poh.record(zero).unwrap(),
            poh.tick().unwrap(),
        ];
        assert_eq!(verify(&zero, &entries), Ok(()));

        assert_eq!(verify(&zero, &[tick(1, one)]), Ok(()));
        assert_eq!(verify(&zero, &[tick(2, two)]), Ok(()));
        let mixed = PohEntry {
            num_hashes: 1,
            hash: one_with_zero,
            mixin: Some(zero),
        };
        assert_eq!(verify(&zero, &[mixed]), Ok(()));
        assert!(verify(&zero, &[tick(1, zero)]).is_err());
        assert_eq!(
            verify(&zero, &[mixed, tick(1, hash(one_with_zero.as_ref()))]),
            Ok(())
        );
    }

    #[test]
    fn record_counts_hashes_since_last_entry() {
        let mut poh = Poh::new(Hash::default(), Some(10));
        assert!(!poh.hash(3));
        let entry = poh.record(Hash::default()).unwrap();
        assert_eq!(entry.num_hashes, 4);
        assert_eq!(poh.num_hashes(), 0);
        assert_eq!(poh.remaining_hashes_until_tick(), 6);
    }

    #[test]
    #[should_panic(expected = "hashes_per_tick must be greater than 1")]
    fn rejects_zero_hashes_per_tick() {
        Poh::new(Hash::default(), Some(0));
    }

    #[test]
    #[should_panic(expected = "hashes_per_tick must be greater than 1")]
    fn rejects_one_hash_per_tick() {
        Poh::new(Hash::default(), Some(1));
    }

    #[test]
    fn poh_tick() {
        let mut poh = Poh::new(Hash::default(), Some(2));
        assert_eq!(poh.remaining_hashes_until_tick(), 2);
        assert!(poh.tick().is_none());
        assert_eq!(poh.remaining_hashes_until_tick(), 1);
        assert_eq!(poh.tick().map(|e| e.num_hashes), Some(2));
        assert_eq!(poh.remaining_hashes_until_tick(), 2);
        assert_eq!(poh.tick_number(), 1);
    }

    #[test]
    fn poh_tick_large_batch() {
        let mut poh = Poh::new(Hash::default(), Some(2));
        assert!(poh.hash(1_000_000));
        assert_eq!(poh.remaining_hashes_until_tick(), 1);
        assert!(poh.hash(1_000_000));
        assert_eq!(poh.remaining_hashes_until_tick(), 1);
        assert_eq!(poh.remaining_hashes_in_slot(2), 3);
        poh.tick();
        assert_eq!(poh.remaining_hashes_until_tick(), 2);
        assert_eq!(poh.remaining_hashes_in_slot(2), 2);
    }

    #[test]
    fn poh_tick_too_soon() {
        let mut poh = Poh::new(Hash::default(), Some(2));
        assert_eq!(poh.remaining_hashes_in_slot(2), 4);
        assert!(poh.tick().is_none());
    }

    #[test]
    fn record_not_permitted_at_final_hash() {
        let mut poh = Poh::new(Hash::default(), Some(10));
        assert!(poh.hash(9));
        assert_eq!(poh.remaining_hashes_until_tick(), 1);
        assert_eq!(poh.remaining_hashes_in_slot(2), 11);
        let before = poh.current_hash();
        assert!(poh.record(Hash::default()).is_none());
        assert_eq!(poh.current_hash(), before);
        assert_eq!(poh.tick().map(|e| e.num_hashes), Some(10));
        assert_eq!(poh.record(Hash::default()).map(|e| e.num_hashes), Some(1));
        assert_eq!(poh.remaining_hashes_until_tick(), 9);
        assert_eq!(poh.remaining_hashes_in_slot(2), 9);
    }

    #[test]
    fn remaining_hashes_in_slot_for_any_ticks_per_slot() {
        let mut poh = Poh::new(Hash::default(), Some(10));
        assert_eq!(poh.remaining_hashes_in_slot(3), 30);
        poh.hash(4);
        assert_eq!(poh.remaining_hashes_in_slot(3), 26);
        poh.hash(u64::MAX);
        poh.tick();
        assert_eq!(poh.remaining_hashes_in_slot(3), 20);
        for _ in 0..2 {
            poh.hash(u64::MAX);
            poh.tick();
        }
        assert_eq!(poh.tick_number(), 3);
        assert_eq!(poh.remaining_hashes_in_slot(3), 30);
    }

    #[test]
    fn remaining_hashes_in_slot_saturates_in_low_power_mode() {
        let mut poh = Poh::new(Hash::default(), None);
        assert_eq!(poh.remaining_hashes_in_slot(64), u64::MAX);
        poh.hash(10);
        assert_eq!(poh.remaining_hashes_in_slot(64), u64::MAX);
        assert_eq!(poh.remaining_hashes_in_slot(1), u64::MAX - 10);
    }

    #[test]
    fn low_power_mode_ticks_on_every_call() {
        let mut poh = Poh::new(Hash::default(), None);
        assert_eq!(poh.hashes_per_tick(), None);
        assert!(!poh.hash(5));
        assert_eq!(poh.tick().map(|e| e.num_hashes), Some(6));
        assert_eq!(poh.tick().map(|e| e.num_hashes), Some(1));
    }

    #[test]
    fn reset_starts_a_new_chain() {
        let mut poh = Poh::new_with_tick_number(Hash::default(), Some(4), 7);
        poh.hash(2);
        let start = hash(b"next");
        poh.reset(start, Some(8));
        assert_eq!(poh.current_hash(), start);
        assert_eq!(poh.num_hashes(), 0);
        assert_eq!(poh.tick_number(), 0);
        assert_eq!(poh.hashes_per_tick(), Some(8));
        assert_eq!(poh.remaining_hashes_until_tick(), 8);
    }
}
