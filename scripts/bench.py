"""Rust bindings vs the pure-Python reference, on one 200 ms mainnet slot."""

import os
import time
from collections.abc import Callable
from hashlib import sha256

import solana_poh
from solana_poh import reference

HASHES_PER_TICK = 31_250
TICKS_PER_SLOT = 64
SLOT_HASHES = HASHES_PER_TICK * TICKS_PER_SLOT


def best_of(runs: int, fn: Callable[[], object]) -> float:
    times = []
    for _ in range(runs):
        start = time.perf_counter()
        fn()
        times.append(time.perf_counter() - start)
    return min(times)


def slot(impl) -> list:
    poh = impl.Poh(bytes(32), HASHES_PER_TICK)
    entries = []
    for tick in range(TICKS_PER_SLOT):
        for record in range(16):
            poh.hash(HASHES_PER_TICK // 17)
            entries.append(poh.record(sha256(bytes([tick, record])).digest()))
        poh.hash(HASHES_PER_TICK)
        entries.append(poh.tick())
    return entries


def main() -> None:
    start = bytes(32)
    entries = slot(solana_poh)
    python_verify = best_of(1, lambda: reference.verify(start, entries))
    rows = [
        ("generate", best_of(5, lambda: slot(solana_poh)), best_of(1, lambda: slot(reference))),
        ("verify", best_of(5, lambda: solana_poh.verify(start, entries)), python_verify),
        (
            f"verify_parallel ({os.cpu_count()} threads)",
            best_of(5, lambda: solana_poh.verify_parallel(start, entries)),
            python_verify,
        ),
    ]
    print(
        f"One slot: {TICKS_PER_SLOT} ticks x {HASHES_PER_TICK:,} hashes, {len(entries)} entries\n"
    )
    print("| Operation | solana_poh (Rust) | reference (Python) | Speedup |")
    print("|---|---|---|---|")
    for name, rust, python in rows:
        print(f"| {name} | {cell(rust)} | {cell(python)} | {python / rust:.0f}x |")


def cell(seconds: float) -> str:
    return f"{seconds * 1e3:,.0f} ms ({SLOT_HASHES / seconds / 1e6:.1f} M hashes/s)"


if __name__ == "__main__":
    main()
