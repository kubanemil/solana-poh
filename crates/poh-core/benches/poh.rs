use std::{hint::black_box, thread::available_parallelism};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use poh_core::{Hash, Poh, PohEntry, hash, hash_n, verify, verify_parallel};
use rayon::ThreadPoolBuilder;

// Mainnet since the 200 ms slot feature gates: 64 ticks of 31,250 hashes.
const HASHES_PER_TICK: u64 = 31_250;
const TICKS_PER_SLOT: u64 = 64;
const RECORDS_PER_TICK: u64 = 16;

fn slot(start: Hash) -> Vec<PohEntry> {
    let mut poh = Poh::new(start, Some(HASHES_PER_TICK));
    let mut entries = Vec::new();
    for tick in 0..TICKS_PER_SLOT {
        for record in 0..RECORDS_PER_TICK {
            poh.hash(HASHES_PER_TICK / (RECORDS_PER_TICK + 1));
            entries.extend(poh.record(hash(&[tick as u8, record as u8])));
        }
        poh.hash(u64::MAX);
        entries.extend(poh.tick());
    }
    entries
}

fn generate(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate");
    group.throughput(Throughput::Elements(HASHES_PER_TICK));
    group.bench_function("hash_n", |b| {
        b.iter(|| hash_n(black_box(&Hash::default()), HASHES_PER_TICK))
    });
    group.bench_function("sha256_digest_loop", |b| {
        b.iter(|| (0..HASHES_PER_TICK).fold(black_box(Hash::default()), |h, _| hash(h.as_ref())))
    });
    group.bench_function("poh_tick", |b| {
        let mut poh = Poh::new(Hash::default(), Some(HASHES_PER_TICK));
        b.iter(|| {
            poh.hash(u64::MAX);
            poh.tick()
        })
    });
    group.finish();
}

fn verify_slot(c: &mut Criterion) {
    let start = Hash::default();
    let entries = slot(start);
    let mut group = c.benchmark_group("verify_slot");
    group.throughput(Throughput::Elements(HASHES_PER_TICK * TICKS_PER_SLOT));
    group.sample_size(10);
    group.bench_function("sequential", |b| {
        b.iter(|| verify(&start, black_box(&entries)))
    });
    let cores = available_parallelism().map_or(1, |n| n.get());
    for threads in (0..).map(|i| 1 << i).take_while(|&t| t <= cores) {
        let Ok(pool) = ThreadPoolBuilder::new().num_threads(threads).build() else {
            continue;
        };
        group.bench_with_input(
            BenchmarkId::new("parallel", threads),
            &entries,
            |b, entries| b.iter(|| pool.install(|| verify_parallel(&start, black_box(entries)))),
        );
    }
    group.finish();
}

criterion_group!(benches, generate, verify_slot);
criterion_main!(benches);
