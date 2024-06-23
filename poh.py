import time
from hashlib import sha256

LOW_POWER_MODE = 2**64 - 1


class PohEntry:
    def __init__(self, num_hashes: int, hash_value: bytes):
        self.num_hashes = num_hashes
        self.hash = hash_value

    def __repr__(self):
        return f"PohEntry(num_hashes={self.num_hashes}, hash={self.hash.hex()})"


class Poh:
    def __init__(self, hash_: bytes, hashes_per_tick: int = None, tick_number: int = 0):
        self.hash_: bytes = hash_
        self.num_hashes: int = 0
        self.hashes_per_tick: int = hashes_per_tick
        self.remaining_hashes: int = hashes_per_tick
        self.tick_number: int = tick_number
        self.slot_start_time: float = time.time()

    def reset(self, hash_: bytes, hashes_per_tick: int = None):
        self.__init__(hash_, hashes_per_tick, 0)

    def target_poh_time(self, target_ns_per_tick):  # check if valid
        assert self.hashes_per_tick > 0
        offset_tick_ns = target_ns_per_tick * self.tick_number
        offset_ns = target_ns_per_tick * self.num_hashes / self.hashes_per_tick
        return self.slot_start_time + (offset_ns + offset_tick_ns) / 1e9

    def hash(self, max_num_hashes: int) -> bool:
        num_hashes = min(max_num_hashes, self.remaining_hashes - 1)

        for _ in range(num_hashes):
            self.hash_ = sha256(self.hash_).digest()

        self.num_hashes += num_hashes
        self.remaining_hashes -= num_hashes

        assert self.remaining_hashes > 0
        return self.remaining_hashes == 1

    def record(self, mixin: bytes):
        if self.remaining_hashes == 1:
            return None

        self.hash_ = sha256(self.hash_ + mixin).digest()
        num_hashes = self.num_hashes + 1
        self.num_hashes = 0
        self.remaining_hashes -= 1

        return PohEntry(num_hashes, self.hash_)

    def tick(self):
        self.hash_ = sha256(self.hash_).digest()
        self.num_hashes += 1
        self.remaining_hashes -= 1

        if self.remaining_hashes != 0:
            return None

        num_hashes = self.num_hashes
        self.remaining_hashes = self.hashes_per_tick
        self.num_hashes = 0
        self.tick_number += 1

        return PohEntry(num_hashes, self.hash_)
