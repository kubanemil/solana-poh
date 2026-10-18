# solana-poh

A Rust implementation of Solana's Proof of History with Python bindings. `poh-core` generates
entries with the same state machine as Agave's `Poh` and verifies them sequentially or in
parallel, reporting the first entry that does not follow from the one before it. The Python
package `solana_poh` wraps it through PyO3 and keeps a short pure-Python version of the same
algorithm for reading. One core of a 4-vCPU EPYC VM generates 20.6 million hashes a second,
twice what mainnet asks of a leader, and its 4 cores verify a full 200 ms slot in 26 ms.

## Why Proof of History

A Solana leader hashes nonstop, and every SHA-256 takes the previous result as input, so the
chain cannot be produced faster than one hash after another. Transactions are mixed into the
chain as they arrive, which fixes their order and how much time passed between them before
any validator votes. Producing the chain is strictly sequential, but checking it splits at
every entry, so a validator verifies a slot on all its cores in a fraction of the time the
leader spent. [The blog post](docs/PohBlog.md) walks through how Agave implements it.

```
  transactions --> mixin = Merkle root of their signatures
                     |
                     v
  start hash --> Poh: hash ... hash, record(mixin), hash ... hash, tick, ...
                     |
                     v
  entries:  {num_hashes: 1001, hash, mixin}  {num_hashes: 30249, hash}  ...
                     |
                     v
  verify_parallel:  entry 0 from the start hash  \
                    entry 1 from entry 0's hash   >  one task per entry, any order
                    entry 2 from entry 1's hash  /
                     |
                     v
  Ok(()) or VerifyError { index of the first bad entry, what was wrong }
```

## Quick start

Rust:

```toml
[dependencies]
poh-core = { git = "https://github.com/kubanemil/solana-poh" }
```

```rust
use poh_core::{Hash, Poh, hash, verify_parallel};

let start = Hash::default();
let mut poh = Poh::new(start, Some(31_250));
let mut entries = Vec::new();

poh.hash(1_000);
entries.extend(poh.record(hash(b"tx batch")));
poh.hash(u64::MAX); // stops one hash short of the tick
entries.extend(poh.tick());

verify_parallel(&start, &entries)?;
```

Python (3.10 or newer; building from source needs a Rust toolchain):

```
pip install git+https://github.com/kubanemil/solana-poh
```

```python
from hashlib import sha256
from solana_poh import Poh, PohEntry, VerifyError, verify_parallel

start = bytes(32)
poh = Poh(start, hashes_per_tick=31_250)
poh.hash(1_000)
entries = [poh.record(sha256(b"tx batch").digest())]
poh.hash(31_250)  # stops one hash short of the tick
entries.append(poh.tick())
verify_parallel(start, entries)

entries[0] = PohEntry(entries[0].num_hashes + 1, entries[0].hash, entries[0].mixin)
try:
    verify_parallel(start, entries)
except VerifyError as err:
    print(err.index, err)  # 0 entry 0: hash does not follow from the previous entry (...)
```

Hashing and verification release the GIL, so other Python threads keep running.
`solana_poh.reference` has the same `Poh`, `PohEntry` and `verify` in plain Python.

The demo replaces the Kafka setup this repo used to have. A source thread sends batches of
simulated transactions, a generator thread records them the way Agave's `PohService` does, and
the main thread writes entries to `entries.poh`, then reads the file back and verifies it. The
channels are bounded, so a slow generator slows the source down instead of losing batches.

```
$ ./do demo
generated 21 slots: 1,344 ticks, 3,073 records, 42,000,000 hashes in 2.08 s (20.2 M hashes/s)
transactions: 100,015 in 3,073 batches sent, 3,073 batches recorded
wrote 4,417 entries to entries.poh
verified 4,417 entries (42,000,000 hashes): sequential 2.04 s (20.6 M hashes/s), parallel (4 threads) 0.59 s (71.7 M hashes/s, 3.5x)
```

Unlike Agave, the generator does not wait for the wall clock, so a slot takes as long as its
hashes do. `poh verify <file>` checks any entry file and exits non-zero on the first bad entry.

## How it matches Agave

`Poh` is a port of `solana_entry::poh::Poh` from Agave's `entry/src/poh.rs` (at
[`898cf60`](https://github.com/anza-xyz/agave/blob/898cf601074d61b735fbde5772abfba12599e56d/entry/src/poh.rs)),
including low-power mode and `remaining_hashes_in_slot`: `hash()` stops one hash short of the
tick, `record()` refuses that last hash, and `tick()` closes the tick. Its unit tests are ported,
as are the verification and tick-count tests from `entry/src/entry.rs` and the Merkle tests from
`merkle-tree`. `verify_tick_hash_count` accepts and rejects exactly what Agave's does, including
the count carried between slices of a slot.

`vectors/poh.json` comes from `scripts/gen_vectors.py`, which computes expected hashes with a
plain `hashlib` loop and never touches `Poh`. It holds Agave's own test case, a hand-laid
tick and record sequence, and a full mainnet slot of 64 ticks of 31,250 hashes. The Rust and
Python tests verify each sequence and replay it through `Poh` entry by entry.

Property tests drive `Poh` with random hash, record and tick calls and check the hash accounting
after every step. They also flip a bit of a hash, change `num_hashes`, or add, change or drop a
mixin at a random entry, and check that both verifiers fail at exactly that entry. pytest checks
that the Rust module and the pure-Python version produce the same entries and report the same
failures.

## Benchmarks

AMD EPYC-Rome Processor (as `/proc/cpuinfo` names it), 4 vCPUs with SHA extensions, Rust
1.99.1, Python 3.14.4. A slot is 64 ticks of 31,250 hashes, mainnet since the 200 ms slot
feature gates, with 16 recorded batches per tick.

| `cargo bench` | Time | Hashes per second |
|---|---:|---:|
| `hash_n`, one tick (31,250 hashes) | 1.52 ms | 20.6 M |
| `Sha256::digest` in a loop, one tick | 2.25 ms | 13.9 M |
| `Poh::hash` and `tick`, one tick | 1.53 ms | 20.5 M |
| `verify`, one slot | 97.4 ms | 20.5 M |
| `verify_parallel`, one slot, 1 thread | 97.6 ms | 20.5 M |
| `verify_parallel`, one slot, 2 threads | 49.6 ms | 40.3 M |
| `verify_parallel`, one slot, 4 threads | 26.0 ms | 77.0 M |

| `scripts/bench.py`, one slot | `solana_poh` | `solana_poh.reference` | Speedup |
|---|---:|---:|---:|
| generate | 100 ms | 1,252 ms | 13x |
| verify | 96 ms | 1,228 ms | 13x |
| verify_parallel, 4 threads | 27 ms | 1,228 ms | 46x |

Pure Python is only 13 times slower because `hashlib` does the hashing in C. Most of its time
goes to the interpreter loop around each call.

## Decisions

| Choice | Not chosen | Why |
|---|---|---|
| `sha2` and a 32-byte `Hash` of our own | `solana-hash` and `solana-sha256-hasher` | Same bytes and the same SHA-256, with no Solana crates in the tree. `Hash::to_bytes()` converts to any other type. |
| `hash_n`: one compression per hash, padding written once | `Sha256::digest` per hash | A 32-byte input is exactly one padded SHA-256 block, so the hasher's buffering is pure overhead. 1.48x faster. |
| The mixin lives on `PohEntry` | Agave's split into `PohEntry` and `Entry { transactions }` | An entry file verifies on its own. Turning transactions into a mixin is `hash_signatures`, Agave's Merkle root. |
| Reject a mixin entry with `num_hashes = 0` | Agave's `next_hash`, which hashes it the same as `num_hashes = 1` | `Poh` never produces one, and accepting it lets `num_hashes` change without the hash changing. |
| Split verification at entry boundaries | Splitting long entries into chunks | Same as Agave's `EntrySlice::verify`. A slot has at least 64 entries, enough for every core here. |
| Python `verify` raises `VerifyError` with `index` | Returning a bool | The index of the bad entry is the useful part of a failure. |
| abi3 wheel for Python 3.10+ | One wheel per Python version | One build covers every supported version. |
| Text entry file | JSON or a binary format | Readable with `head` and diffable. A real ledger format is on the roadmap. |

## Layout

```
crates/poh-core/          the library
  src/poh.rs              Poh and PohEntry, ported from Agave's entry/src/poh.rs
  src/verify.rs           verify, verify_parallel, verify_tick_hash_count
  src/hash.rs             Hash, SHA-256 helpers, the one-block hash loop
  src/merkle.rs           hash_signatures, Agave's Merkle root of signatures
  tests/                  vector and property tests
  benches/                criterion benches
crates/poh-cli/           the poh binary: demo and verify
crates/poh-py/            PyO3 bindings, built by maturin as solana_poh._core
python/solana_poh/        the Python package and reference.py
python/tests/             pytest
scripts/                  vector generator and the Python benchmark
vectors/poh.json          expected hashes, shared by the Rust and Python tests
docs/PohBlog.md           the blog post
```

## Development

You need Rust (tested on 1.99) and [uv](https://docs.astral.sh/uv/). uv installs pytest,
maturin and ruff at the versions pinned in `uv.lock`.

```
./do test      cargo tests, then build the Python module and run pytest
./do check     rustfmt --check, clippy with warnings as errors, ruff
./do fmt       format Rust and Python
./do bench     criterion benches, then the Python benchmark
./do demo      run poh demo; flags pass through (./do demo --seconds 5 --tps 100000)
./do vectors   regenerate vectors/poh.json
```

CI runs `check`, the Rust tests and a short demo, then builds the release wheel and runs pytest
against the installed wheel.

## Roadmap

Milestone 2:

- A GPU verifier: one GPU thread per entry, for slots with thousands of entries.
- SIMD multi-buffer SHA-256: hash 8 entries at once with AVX2 (16 with AVX-512), and measure it
  against SHA extensions on one core.
- A slot and leader schedule simulation: leaders rotating every 4 slots, ticks paced to the wall
  clock like Agave's `PohService`, and a verifier that checks slot boundaries.
- A ledger format: binary entries with slot boundaries and an index, so the verifier can stream
  a file and seek to a slot.

## License

MIT
