use std::{
    sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
    thread::{self, ScopedJoinHandle},
    time::{Duration, Instant},
};

use poh_core::{Hash, Poh, PohEntry, hash, hash_signatures};

// Agave's PohService hashes in batches of 64 between checks for new records.
const HASHES_PER_BATCH: u64 = 64;
const MAX_BATCH_TXS: u8 = 64;
const CHANNEL_CAPACITY: usize = 1024;

#[derive(Debug, Clone)]
pub struct Config {
    pub start: Hash,
    pub hashes_per_tick: u64,
    pub ticks_per_slot: u64,
    pub seconds: f64,
    pub tps: f64,
}

#[derive(Debug, Default)]
pub struct Report {
    pub batches_sent: u64,
    pub transactions_sent: u64,
    pub records: u64,
    pub ticks: u64,
    pub hashes: u64,
    pub elapsed: Duration,
}

/// Runs a transaction source and a PoH generator on their own threads and
/// passes every entry to `sink` in order. Ends at the first slot boundary
/// after the source stops and every batch it sent has been recorded.
pub fn run<E>(
    config: &Config,
    mut sink: impl FnMut(&PohEntry) -> Result<(), E>,
) -> Result<Report, E> {
    let began = Instant::now();
    thread::scope(|scope| {
        let (batch_tx, batch_rx) = sync_channel(CHANNEL_CAPACITY);
        let (entry_tx, entry_rx) = sync_channel(CHANNEL_CAPACITY);
        let source = scope.spawn(move || transaction_source(config, &batch_tx));
        let generator = scope.spawn(move || generate(config, &batch_rx, &entry_tx));

        let mut report = Report::default();
        for entry in &entry_rx {
            report.hashes += entry.num_hashes;
            if entry.is_tick() {
                report.ticks += 1;
            } else {
                report.records += 1;
            }
            // On error, dropping entry_rx stops the generator, which stops the source.
            sink(&entry)?;
        }
        report.elapsed = began.elapsed();
        (report.batches_sent, report.transactions_sent) = join(source);
        join(generator);
        Ok(report)
    })
}

fn join<T>(handle: ScopedJoinHandle<'_, T>) -> T {
    handle
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// Stands in for the banking stage: batches of 1..=64 transactions with
/// pseudo-random signatures, paced to `config.tps`. Returns batches and
/// transactions sent.
fn transaction_source(config: &Config, batches: &SyncSender<Hash>) -> (u64, u64) {
    let began = Instant::now();
    let (mut sent_batches, mut sent_txs) = (0u64, 0u64);
    while began.elapsed().as_secs_f64() < config.seconds {
        let seed = hash(&sent_batches.to_le_bytes());
        let len = 1 + seed.as_bytes()[0] % MAX_BATCH_TXS;
        let signatures: Vec<[u8; 64]> = (0..len).map(|i| signature(&seed, i)).collect();
        // Blocking send: a slow generator slows the source down instead of losing batches.
        if batches.send(hash_signatures(&signatures)).is_err() {
            break;
        }
        sent_batches += 1;
        sent_txs += u64::from(len);
        let due = Duration::from_secs_f64(sent_txs as f64 / config.tps);
        if let Some(wait) = due.checked_sub(began.elapsed()) {
            thread::sleep(wait);
        }
    }
    (sent_batches, sent_txs)
}

fn signature(seed: &Hash, index: u8) -> [u8; 64] {
    let mut signature = [0; 64];
    for (half, part) in signature.as_chunks_mut::<32>().0.iter_mut().zip(0u8..) {
        *half = hash(&[seed.as_ref(), &[index, part]].concat()).to_bytes();
    }
    signature
}

/// The PohService loop: record a waiting batch, otherwise hash a little and
/// tick when due. Stops early if the entry receiver is gone.
fn generate(config: &Config, batches: &Receiver<Hash>, entries: &SyncSender<PohEntry>) {
    let mut poh = Poh::new(config.start, Some(config.hashes_per_tick));
    let send = |entry: PohEntry| entries.send(entry).is_ok();
    loop {
        let sent = match batches.try_recv() {
            Ok(mixin) => record(&mut poh, mixin, send),
            Err(TryRecvError::Empty) => !poh.hash(HASHES_PER_BATCH) || poh.tick().is_none_or(send),
            Err(TryRecvError::Disconnected) => break,
        };
        if !sent {
            return;
        }
    }
    let slot_done = |poh: &Poh| {
        poh.tick_number().is_multiple_of(config.ticks_per_slot)
            && poh.remaining_hashes_until_tick() == config.hashes_per_tick
    };
    while !slot_done(&poh) {
        poh.hash(u64::MAX);
        if poh.tick().is_some_and(|tick| !send(tick)) {
            return;
        }
    }
}

/// Like Agave's `PohRecorder::record`: if the next hash belongs to a tick,
/// tick first, then record.
fn record(poh: &mut Poh, mixin: Hash, send: impl Fn(PohEntry) -> bool) -> bool {
    loop {
        if let Some(entry) = poh.record(mixin) {
            return send(entry);
        }
        if poh.tick().is_some_and(|tick| !send(tick)) {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poh_core::{verify, verify_tick_hash_counts};

    #[test]
    fn records_every_batch_and_ends_on_a_slot_boundary() {
        let config = Config {
            start: hash(b"test"),
            hashes_per_tick: 100,
            ticks_per_slot: 4,
            seconds: 0.05,
            tps: 20_000.0,
        };
        let mut entries = Vec::new();
        let report = run(&config, |entry| {
            entries.push(*entry);
            Ok::<_, ()>(())
        })
        .unwrap();

        assert!(report.batches_sent > 0);
        assert_eq!(report.records, report.batches_sent);
        assert_eq!(report.ticks % config.ticks_per_slot, 0);
        assert_eq!(report.hashes, report.ticks * config.hashes_per_tick);
        assert_eq!(verify(&config.start, &entries), Ok(()));
        assert_eq!(
            verify_tick_hash_counts(&entries, config.hashes_per_tick),
            Ok(())
        );
    }

    #[test]
    fn sink_error_stops_both_threads() {
        let config = Config {
            start: Hash::default(),
            hashes_per_tick: 100,
            ticks_per_slot: 4,
            seconds: 60.0,
            tps: 1_000.0,
        };
        let mut seen = 0;
        let result = run(&config, |_| {
            seen += 1;
            if seen == 10 { Err("full") } else { Ok(()) }
        });
        assert_eq!(result.err(), Some("full"));
    }
}
