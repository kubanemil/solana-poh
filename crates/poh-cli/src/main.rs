mod demo;
mod ledger;

use std::{
    error::Error,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

use poh_core::{hash, verify, verify_parallel, verify_tick_hash_count};

use crate::ledger::{Ledger, LedgerWriter};

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

const USAGE: &str = "\
poh: Solana Proof of History generator and verifier

Usage:
  poh demo [options]      Generate entries from a simulated transaction stream,
                          write them to a file, then verify the file
  poh verify <file>       Verify an entry file
  poh help                Show this help

Demo options:
  --seconds <s>           How long the transaction source runs [default: 2]
  --tps <n>               Transactions per second it sends [default: 50000]
  --hashes-per-tick <n>   [default: 31250, mainnet with 200 ms slots]
  --ticks-per-slot <n>    [default: 64]
  --out <file>            [default: entries.poh]

Both commands take --threads <n> for parallel verification [default: all cores].
";

#[derive(Debug)]
enum Command {
    Demo { config: demo::Config, out: PathBuf },
    Verify { file: PathBuf },
    Help,
}

fn main() -> ExitCode {
    match parse_args().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args() -> Result<Command> {
    use lexopt::prelude::*;

    let mut parser = lexopt::Parser::from_env();
    let mut threads = None;
    let mut config = demo::Config {
        start: hash(b"solana-poh demo"),
        hashes_per_tick: 31_250,
        ticks_per_slot: 64,
        seconds: 2.0,
        tps: 50_000.0,
    };
    let mut out = PathBuf::from("entries.poh");
    let mut file = None;

    let command = match parser.next()? {
        Some(Value(command)) => command.string()?,
        Some(Short('h') | Long("help")) | None => return Ok(Command::Help),
        Some(arg) => return Err(arg.unexpected().into()),
    };
    while let Some(arg) = parser.next()? {
        match (command.as_str(), arg) {
            (_, Short('h') | Long("help")) => return Ok(Command::Help),
            (_, Long("threads")) => threads = Some(parser.value()?.parse()?),
            ("demo", Long("seconds")) => config.seconds = parser.value()?.parse()?,
            ("demo", Long("tps")) => config.tps = parser.value()?.parse()?,
            ("demo", Long("hashes-per-tick")) => {
                config.hashes_per_tick = parser.value()?.parse()?
            }
            ("demo", Long("ticks-per-slot")) => config.ticks_per_slot = parser.value()?.parse()?,
            ("demo", Long("out")) => out = parser.value()?.into(),
            ("verify", Value(path)) if file.is_none() => file = Some(PathBuf::from(path)),
            (_, arg) => return Err(arg.unexpected().into()),
        }
    }
    if let Some(threads) = threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()?;
    }

    match command.as_str() {
        "demo" => {
            if config.hashes_per_tick < 2 || config.ticks_per_slot == 0 {
                return Err(
                    "--hashes-per-tick must be at least 2 and --ticks-per-slot at least 1".into(),
                );
            }
            if !(config.tps > 0.0 && config.seconds >= 0.0) {
                return Err("--tps must be positive and --seconds not negative".into());
            }
            Ok(Command::Demo { config, out })
        }
        "verify" => Ok(Command::Verify {
            file: file.ok_or("verify needs a file")?,
        }),
        "help" => Ok(Command::Help),
        other => Err(format!("unknown command {other:?}, see `poh help`").into()),
    }
}

fn run(command: Command) -> Result<()> {
    match command {
        Command::Demo { config, out } => run_demo(&config, &out),
        Command::Verify { file } => verify_file(&file),
        Command::Help => {
            print!("{USAGE}");
            Ok(())
        }
    }
}

fn run_demo(config: &demo::Config, out: &Path) -> Result<()> {
    let mut writer = LedgerWriter::create(out, &config.start, Some(config.hashes_per_tick))?;
    let report = demo::run(config, |entry| writer.write(entry))?;
    writer.finish()?;

    let slots = report.ticks / config.ticks_per_slot;
    println!(
        "generated {slots} slots: {} ticks, {} records, {} hashes in {} ({}/s)",
        count(report.ticks),
        count(report.records),
        count(report.hashes),
        secs(report.elapsed),
        rate(report.hashes, report.elapsed),
    );
    println!(
        "transactions: {} in {} batches sent, {} batches recorded",
        count(report.transactions_sent),
        count(report.batches_sent),
        count(report.records),
    );
    if report.records != report.batches_sent {
        return Err("some batches were not recorded".into());
    }
    println!(
        "wrote {} entries to {}",
        count(report.ticks + report.records),
        out.display()
    );
    verify_file(out)
}

fn verify_file(path: &Path) -> Result<()> {
    let Ledger {
        start,
        hashes_per_tick,
        entries,
    } = ledger::read(path)?;
    let hashes = entries.iter().map(|e| e.num_hashes).sum();

    let sequential = timed(|| verify(&start, &entries))?;
    let parallel = timed(|| verify_parallel(&start, &entries))?;
    if let Some(hashes_per_tick) = hashes_per_tick {
        verify_tick_hash_count(&entries, &mut 0, hashes_per_tick)?;
    }
    println!(
        "verified {} entries ({} hashes): sequential {} ({}/s), parallel ({} threads) {} ({}/s, {:.1}x)",
        count(entries.len() as u64),
        count(hashes),
        secs(sequential),
        rate(hashes, sequential),
        rayon::current_num_threads(),
        secs(parallel),
        rate(hashes, parallel),
        sequential.as_secs_f64() / parallel.as_secs_f64(),
    );
    Ok(())
}

fn timed<E>(check: impl FnOnce() -> Result<(), E>) -> Result<Duration, E> {
    let began = Instant::now();
    check().map(|()| began.elapsed())
}

fn secs(elapsed: Duration) -> String {
    format!("{:.2} s", elapsed.as_secs_f64())
}

fn rate(hashes: u64, elapsed: Duration) -> String {
    format!(
        "{:.1} M hashes",
        hashes as f64 / elapsed.as_secs_f64() / 1e6
    )
}

fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}
