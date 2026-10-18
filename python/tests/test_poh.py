import json
import random
import threading
import time
from collections.abc import Callable
from hashlib import sha256
from pathlib import Path

import pytest

import solana_poh
from solana_poh import Poh, PohEntry, VerifyError, reference

VECTORS = json.loads((Path(__file__).parents[2] / "vectors" / "poh.json").read_text())
IMPLEMENTATIONS = [solana_poh, reference]


def as_tuple(entry) -> tuple:
    return (entry.num_hashes, entry.hash, entry.mixin)


def vector_entries(sequence: dict) -> list[tuple]:
    return [
        (e["num_hashes"], bytes.fromhex(e["hash"]), e["mixin"] and bytes.fromhex(e["mixin"]))
        for e in sequence["entries"]
    ]


def drive(impl, start: bytes, hashes_per_tick: int, seed: int, steps: int = 200):
    """Feed the same random hash/record/tick calls to a Poh, like PohRecorder does."""
    rng = random.Random(seed)
    poh = impl.Poh(start, hashes_per_tick)
    entries, states = [], []
    for _ in range(steps):
        action = rng.choice(["hash", "record", "tick"])
        if action == "hash":
            poh.hash(rng.randrange(hashes_per_tick * 2))
        elif action == "record":
            mixin = rng.randbytes(32)
            while (entry := poh.record(mixin)) is None:
                entries.append(poh.tick())
            entries.append(entry)
        elif tick := poh.tick():
            entries.append(tick)
        states.append((poh.current_hash, poh.num_hashes, poh.remaining_hashes_until_tick))
    return entries, states


@pytest.mark.parametrize("impl", IMPLEMENTATIONS, ids=["rust", "reference"])
@pytest.mark.parametrize("sequence", VECTORS["sequences"], ids=lambda s: s["name"])
def test_replays_vector_sequences(impl, sequence):
    start = bytes.fromhex(sequence["start"])
    poh = impl.Poh(start, sequence["hashes_per_tick"])
    produced = []
    for num_hashes, _, mixin in vector_entries(sequence):
        poh.hash(num_hashes - 1)
        produced.append(poh.record(mixin) if mixin else poh.tick())
    assert [as_tuple(e) for e in produced] == vector_entries(sequence)
    impl.verify(start, produced)


@pytest.mark.parametrize("impl", IMPLEMENTATIONS, ids=["rust", "reference"])
@pytest.mark.parametrize(
    "chain", [c for c in VECTORS["chains"] if c["num_hashes"] <= 31_250], ids=str
)
def test_hash_chains(impl, chain):
    poh = impl.Poh(bytes.fromhex(chain["start"]))
    poh.hash(chain["num_hashes"])
    assert poh.current_hash.hex() == chain["hash"]


@pytest.mark.parametrize("hashes_per_tick", [2, 3, 10, 64])
@pytest.mark.parametrize("seed", range(5))
def test_rust_matches_reference(hashes_per_tick, seed):
    start = sha256(f"start-{seed}".encode()).digest()
    rust_entries, rust_states = drive(solana_poh, start, hashes_per_tick, seed)
    ref_entries, ref_states = drive(reference, start, hashes_per_tick, seed)
    assert [as_tuple(e) for e in rust_entries] == [as_tuple(e) for e in ref_entries]
    assert rust_states == ref_states
    solana_poh.verify_tick_hash_counts(rust_entries, hashes_per_tick)


def tampered_variants(entry: PohEntry) -> list[PohEntry]:
    flipped = bytes([entry.hash[0] ^ 1]) + entry.hash[1:]
    return [
        PohEntry(entry.num_hashes, flipped, entry.mixin),
        PohEntry(entry.num_hashes + 1, entry.hash, entry.mixin),
        PohEntry(max(entry.num_hashes - 1, 0), entry.hash, entry.mixin),
        PohEntry(entry.num_hashes, entry.hash, None if entry.mixin else bytes(32)),
        PohEntry(entry.num_hashes, entry.hash, sha256(entry.mixin or b"").digest()),
    ]


@pytest.mark.parametrize("seed", range(3))
def test_tampering_fails_at_that_entry(seed):
    start = bytes(32)
    entries, _ = drive(solana_poh, start, 16, seed)
    for impl in IMPLEMENTATIONS:
        impl.verify(start, entries)
    solana_poh.verify_parallel(start, entries)

    rng = random.Random(seed)
    for index in rng.sample(range(len(entries)), 10):
        for bad in tampered_variants(entries[index]):
            tampered = [*entries[:index], bad, *entries[index + 1 :]]
            errors = []
            for check in (solana_poh.verify, solana_poh.verify_parallel, reference.verify):
                with pytest.raises(VerifyError) as caught:
                    check(start, tampered)
                errors.append((caught.value.index, caught.value.reason))
            assert errors == [(index, errors[0][1])] * 3


def test_tick_hash_counts():
    entries, _ = drive(solana_poh, bytes(32), 8, seed=1)
    ticks = [i for i, e in enumerate(entries) if e.is_tick]
    solana_poh.verify_tick_hash_counts(entries, 8)
    with pytest.raises(VerifyError) as caught:
        solana_poh.verify_tick_hash_counts(entries, 9)
    assert caught.value.index == ticks[0]


@pytest.mark.parametrize("case", VECTORS["hash_signatures"], ids=lambda c: str(len(c["leaves"])))
def test_hash_signatures(case):
    leaves = [bytes.fromhex(leaf) for leaf in case["leaves"]]
    assert solana_poh.hash_signatures(leaves).hex() == case["root"]
    assert reference.hash_signatures(leaves).hex() == case["root"]


@pytest.mark.parametrize("impl", IMPLEMENTATIONS, ids=["rust", "reference"])
def test_record_refuses_the_tick_hash(impl):
    poh = impl.Poh(bytes(32), 10)
    assert poh.hash(100) is True
    before = poh.current_hash
    assert poh.record(bytes(32)) is None
    assert poh.current_hash == before
    tick = poh.tick()
    assert (tick.num_hashes, tick.is_tick) == (10, True)


def test_invalid_input():
    with pytest.raises(ValueError, match="greater than 1"):
        Poh(bytes(32), 1)
    with pytest.raises(ValueError, match="greater than 1"):
        reference.Poh(bytes(32), 1)
    with pytest.raises(ValueError, match="32 bytes"):
        Poh(b"short")
    with pytest.raises(ValueError, match="32 bytes"):
        PohEntry(1, bytes(32), b"short")
    with pytest.raises(ValueError, match="power of two"):
        Poh(bytes(32), 10).remaining_hashes_in_slot(3)


def test_entry_is_a_value():
    entry = PohEntry(3, bytes(32), bytes(32))
    assert entry == PohEntry(3, bytes(32), bytes(32))
    assert len({entry, PohEntry(3, bytes(32), bytes(32))}) == 1
    assert "num_hashes=3" in repr(entry)
    with pytest.raises(AttributeError):
        entry.num_hashes = 4


def longest_main_thread_stall(work: Callable[[], object]) -> tuple[float, float]:
    done = threading.Event()
    worker = threading.Thread(target=lambda: (work(), done.set()))
    began = last = time.perf_counter()
    longest = 0.0
    worker.start()
    while not done.is_set():
        now = time.perf_counter()
        longest, last = max(longest, now - last), now
    worker.join()
    return longest, time.perf_counter() - began


def test_hashing_and_verifying_release_the_gil():
    n = 5_000_000
    poh = Poh(bytes(32))
    stall, total = longest_main_thread_stall(lambda: poh.hash(n))
    assert stall < total / 4

    entries = [PohEntry(n, poh.current_hash)]
    stall, total = longest_main_thread_stall(lambda: solana_poh.verify(bytes(32), entries))
    assert stall < total / 4
