# solana-poh

Solana's Proof of History in Rust, with Python bindings. It generates entries with the same
state machine as Agave's `Poh` and verifies them on all cores, pointing at the first entry that
doesn't follow from the one before it. How PoH works and how Agave implements it is in
[my blog post](docs/PohBlog.md).

On a 4-vCPU AMD EPYC VM, one core generates 20.6 million hashes a second (mainnet asks a leader
for 10 million), and four cores verify a full 200 ms slot of 2 million hashes in 26 ms.

```
$ ./do demo --compare
generated 20 slots: 1,280 ticks, 3,073 records, 40,000,000 hashes in 2.05 s (19.5 M hashes/s)
transactions: 100,015 in 3,073 batches sent, 3,073 batches recorded
wrote 4,353 entries to entries.poh
verified 4,353 entries (40,000,000 hashes) on 4 threads in 0.54 s (73.6 M hashes/s)
sequential: 2.00 s (20.0 M hashes/s), 3.7x slower
```

In the demo one thread sends batches of fake transactions and another records them into the
chain the way Agave's `PohService` does. The entries go to a text file, which is read back and
verified. Unlike Agave, the generator doesn't wait for the wall clock, so it makes slots as fast
as it can hash. `poh verify <file>` checks any entry file and exits non-zero at the first bad
entry.

You need Rust and [uv](https://docs.astral.sh/uv/). `./do test` runs the Rust tests and pytest,
`./do check` runs rustfmt, clippy and ruff, and `./do bench` runs the benchmarks.

## Using it

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

The Python package has the same `Poh`, `PohEntry` and verify functions:
`pip install git+https://github.com/kubanemil/solana-poh` (Python 3.10+, and a Rust toolchain
to build it). A failed check raises `VerifyError` with the
`index` of the bad entry, and `hash`, `verify` and `verify_parallel` release the GIL.
`solana_poh.reference` is the whole algorithm in plain Python.

## Matching Agave

`Poh` is a port of Agave's
[`entry/src/poh.rs`](https://github.com/anza-xyz/agave/blob/898cf601074d61b735fbde5772abfba12599e56d/entry/src/poh.rs),
and its unit tests came with it, along with the verify and tick-count tests from `entry.rs`.
The expected hashes in `vectors/poh.json`, including a full mainnet slot, come from
`scripts/gen_vectors.py`, which uses a plain `hashlib` loop and never touches `Poh`. Property
tests tamper with one random entry (flip a hash bit, change `num_hashes`, add or drop a mixin)
and check that both verifiers fail at exactly that entry.

One difference is on purpose: a mixin entry with `num_hashes = 0` is rejected. Agave hashes it
as if it were 1, which lets `num_hashes` change without the hash changing.

MIT license.
