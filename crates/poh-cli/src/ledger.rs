//! A line-based entry file: a header with the start hash, then one entry per line.
//!
//! ```text
//! poh-entries-v1 start=<hex> hashes_per_tick=<n|none>
//! <num_hashes> <hash>            a tick
//! <num_hashes> <hash> <mixin>    an entry with a mixin
//! ```

use std::{
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::Path,
};

use poh_core::{Hash, PohEntry};

const MAGIC: &str = "poh-entries-v1";

#[derive(Debug)]
pub struct Ledger {
    pub start: Hash,
    pub hashes_per_tick: Option<u64>,
    pub entries: Vec<PohEntry>,
}

#[derive(Debug)]
pub struct LedgerWriter {
    out: BufWriter<File>,
}

impl LedgerWriter {
    pub fn create(path: &Path, start: &Hash, hashes_per_tick: Option<u64>) -> io::Result<Self> {
        let mut out = BufWriter::new(File::create(path)?);
        let hashes_per_tick = hashes_per_tick.map_or("none".to_string(), |n| n.to_string());
        writeln!(
            out,
            "{MAGIC} start={start} hashes_per_tick={hashes_per_tick}"
        )?;
        Ok(Self { out })
    }

    pub fn write(&mut self, entry: &PohEntry) -> io::Result<()> {
        match entry.mixin {
            Some(mixin) => writeln!(self.out, "{} {} {mixin}", entry.num_hashes, entry.hash),
            None => writeln!(self.out, "{} {}", entry.num_hashes, entry.hash),
        }
    }

    pub fn finish(mut self) -> io::Result<()> {
        self.out.flush()
    }
}

pub fn read(path: &Path) -> io::Result<Ledger> {
    let mut lines = BufReader::new(File::open(path)?).lines();
    let header = lines.next().transpose()?.unwrap_or_default();
    let (start, hashes_per_tick) = parse_header(&header).ok_or_else(|| invalid(1, "bad header"))?;
    let entries = lines
        .enumerate()
        .map(|(i, line)| parse_entry(&line?).ok_or_else(|| invalid(i + 2, "bad entry")))
        .collect::<io::Result<_>>()?;
    Ok(Ledger {
        start,
        hashes_per_tick,
        entries,
    })
}

fn parse_header(line: &str) -> Option<(Hash, Option<u64>)> {
    let mut fields = line.split_whitespace();
    if fields.next()? != MAGIC {
        return None;
    }
    let start = fields.next()?.strip_prefix("start=")?.parse().ok()?;
    let hashes_per_tick = match fields.next()?.strip_prefix("hashes_per_tick=")? {
        "none" => None,
        n => Some(n.parse().ok()?),
    };
    fields.next().is_none().then_some((start, hashes_per_tick))
}

fn parse_entry(line: &str) -> Option<PohEntry> {
    let mut fields = line.split_whitespace();
    let num_hashes = fields.next()?.parse().ok()?;
    let hash = fields.next()?.parse().ok()?;
    let mixin = fields.next().map(str::parse).transpose().ok()?;
    fields.next().is_none().then_some(PohEntry {
        num_hashes,
        hash,
        mixin,
    })
}

fn invalid(line: usize, what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("line {line}: {what}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use poh_core::{Poh, hash};

    #[test]
    fn round_trip() {
        let path = std::env::temp_dir().join(format!("poh-ledger-{}.txt", std::process::id()));
        let start = hash(b"start");
        let mut poh = Poh::new(start, Some(4));
        poh.hash(1);
        let entries = [poh.record(hash(b"tx")).unwrap(), {
            poh.hash(u64::MAX);
            poh.tick().unwrap()
        }];

        let mut writer = LedgerWriter::create(&path, &start, Some(4)).unwrap();
        entries.iter().try_for_each(|e| writer.write(e)).unwrap();
        writer.finish().unwrap();
        let ledger = read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(ledger.start, start);
        assert_eq!(ledger.hashes_per_tick, Some(4));
        assert_eq!(ledger.entries, entries);
    }

    #[test]
    fn rejects_malformed_lines() {
        let zero = Hash::default();
        assert!(parse_header(&format!("{MAGIC} start={zero} hashes_per_tick=none")).is_some());
        assert!(parse_header(&format!("{MAGIC} start={zero}")).is_none());
        assert!(parse_header("poh-entries-v0 start=00 hashes_per_tick=none").is_none());
        assert!(parse_entry(&format!("3 {zero}")).is_some());
        assert!(parse_entry(&format!("3 {zero} {zero}")).is_some());
        assert!(parse_entry(&format!("3 {zero} {zero} extra")).is_none());
        assert!(parse_entry(&format!("x {zero}")).is_none());
        assert!(parse_entry("3 abc").is_none());
    }
}
