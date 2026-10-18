"""Proof of History in plain Python, step for step the same as the Rust core.

It runs about 12x slower than `solana_poh.Poh`, but the whole algorithm fits on
one screen. The tests use it as a second implementation, and the benchmark as
the baseline.
"""

from __future__ import annotations

import operator
from collections.abc import Sequence
from dataclasses import dataclass
from hashlib import sha256

from solana_poh._errors import VerifyError

# Agave's low-power mode: with no hashes_per_tick, every tick() emits a tick.
LOW_POWER_MODE = 2**64 - 1


# The Rust bindings take u64 and 32-byte hashes; these raise the same errors they do.
def u64(value: int) -> int:
    value = operator.index(value)
    if not 0 <= value <= LOW_POWER_MODE:
        raise OverflowError(f"{value} does not fit in an unsigned 64-bit integer")
    return value


def hash32(value: bytes) -> bytes:
    if not isinstance(value, bytes):
        raise TypeError(f"expected bytes, got {type(value).__name__}")
    if len(value) != 32:
        raise ValueError(f"expected 32 bytes, got {len(value)}")
    return value


@dataclass(frozen=True)
class PohEntry:
    num_hashes: int
    hash: bytes
    mixin: bytes | None = None

    def __post_init__(self) -> None:
        u64(self.num_hashes)
        hash32(self.hash)
        if self.mixin is not None:
            hash32(self.mixin)

    @property
    def is_tick(self) -> bool:
        return self.mixin is None


class Poh:
    def __init__(self, start_hash: bytes, hashes_per_tick: int | None = None) -> None:
        if hashes_per_tick is not None and u64(hashes_per_tick) <= 1:
            raise ValueError("hashes_per_tick must be greater than 1")
        self.current_hash = hash32(start_hash)
        self.num_hashes = 0
        self._hashes_per_tick = hashes_per_tick or LOW_POWER_MODE
        self.remaining_hashes_until_tick = self._hashes_per_tick
        self.tick_number = 0

    @property
    def hashes_per_tick(self) -> int | None:
        return None if self._hashes_per_tick == LOW_POWER_MODE else self._hashes_per_tick

    def hash(self, max_num_hashes: int) -> bool:
        """Hash until max_num_hashes or one short of the tick. True means tick() next."""
        num_hashes = min(u64(max_num_hashes), self.remaining_hashes_until_tick - 1)
        self.current_hash = hash_n(self.current_hash, num_hashes)
        self.num_hashes += num_hashes
        self.remaining_hashes_until_tick -= num_hashes
        return self.remaining_hashes_until_tick == 1

    def record(self, mixin: bytes) -> PohEntry | None:
        """Mix `mixin` into the next hash. None if that hash belongs to a tick."""
        hash32(mixin)
        if self.remaining_hashes_until_tick == 1:
            return None
        self.current_hash = sha256(self.current_hash + mixin).digest()
        num_hashes = self.num_hashes + 1
        self.num_hashes = 0
        self.remaining_hashes_until_tick -= 1
        return PohEntry(num_hashes, self.current_hash, mixin)

    def tick(self) -> PohEntry | None:
        """Hash once. Returns the tick entry if that hash completed a tick."""
        self.current_hash = sha256(self.current_hash).digest()
        self.num_hashes += 1
        self.remaining_hashes_until_tick -= 1
        if self._hashes_per_tick != LOW_POWER_MODE and self.remaining_hashes_until_tick != 0:
            return None
        num_hashes = self.num_hashes
        self.remaining_hashes_until_tick = self._hashes_per_tick
        self.num_hashes = 0
        self.tick_number += 1
        return PohEntry(num_hashes, self.current_hash)

    def remaining_hashes_in_slot(self, ticks_per_slot: int) -> int:
        if u64(ticks_per_slot) == 0:
            raise ValueError("ticks_per_slot must be positive")
        ticks_after_this = ticks_per_slot - self.tick_number % ticks_per_slot - 1
        hashes = ticks_after_this * self._hashes_per_tick + self.remaining_hashes_until_tick
        return min(hashes, LOW_POWER_MODE)


def hash_n(start: bytes, n: int) -> bytes:
    for _ in range(n):
        start = sha256(start).digest()
    return start


def next_hash(prev: bytes, num_hashes: int, mixin: bytes | None) -> bytes:
    if mixin is None:
        return hash_n(prev, num_hashes)
    return sha256(hash_n(prev, max(num_hashes - 1, 0)) + mixin).digest()


def verify(start_hash: bytes, entries: Sequence[PohEntry]) -> None:
    """Raise VerifyError at the first entry that does not follow from the one before."""
    prev = hash32(start_hash)
    for index, entry in enumerate(entries):
        if entry.num_hashes == 0 and entry.mixin is not None:
            raise VerifyError(index, "mixin entry has num_hashes = 0")
        computed = next_hash(prev, entry.num_hashes, entry.mixin)
        if computed != entry.hash:
            reason = f"hash does not follow from the previous entry (computed {computed.hex()})"
            raise VerifyError(index, reason)
        prev = entry.hash


def hash_signatures(signatures: Sequence[bytes]) -> bytes:
    """Merkle root of a batch's signatures, the mixin Agave records for it."""
    level = [sha256(b"\x00" + signature).digest() for signature in signatures]
    while len(level) > 1:
        level = [
            sha256(b"\x01" + level[i] + level[min(i + 1, len(level) - 1)]).digest()
            for i in range(0, len(level), 2)
        ]
    return level[0] if level else bytes(32)
