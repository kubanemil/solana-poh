use std::{fmt, str::FromStr};

use sha2::{Digest, Sha256, block_api::compress256};

pub const HASH_BYTES: usize = 32;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash([u8; HASH_BYTES]);

impl Hash {
    pub const fn new(bytes: [u8; HASH_BYTES]) -> Self {
        Self(bytes)
    }

    pub const fn to_bytes(self) -> [u8; HASH_BYTES] {
        self.0
    }

    pub const fn as_bytes(&self) -> &[u8; HASH_BYTES] {
        &self.0
    }
}

impl From<[u8; HASH_BYTES]> for Hash {
    fn from(bytes: [u8; HASH_BYTES]) -> Self {
        Self(bytes)
    }
}

impl From<Hash> for [u8; HASH_BYTES] {
    fn from(hash: Hash) -> Self {
        hash.0
    }
}

impl TryFrom<&[u8]> for Hash {
    type Error = ParseHashError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        <[u8; HASH_BYTES]>::try_from(bytes)
            .map(Self)
            .map_err(|_| ParseHashError::Length(bytes.len()))
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({self})")
    }
}

impl FromStr for Hash {
    type Err = ParseHashError;

    fn from_str(hex: &str) -> Result<Self, Self::Err> {
        let hex = hex.as_bytes();
        if hex.len() != 2 * HASH_BYTES {
            return Err(ParseHashError::HexLength(hex.len()));
        }
        let mut bytes = [0; HASH_BYTES];
        for (byte, &[high, low]) in bytes.iter_mut().zip(hex.as_chunks().0) {
            *byte = (nibble(high)? << 4) | nibble(low)?;
        }
        Ok(Self(bytes))
    }
}

fn nibble(digit: u8) -> Result<u8, ParseHashError> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        b'A'..=b'F' => Ok(digit - b'A' + 10),
        _ => Err(ParseHashError::Digit(digit.into())),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseHashError {
    Length(usize),
    HexLength(usize),
    Digit(char),
}

impl fmt::Display for ParseHashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length(len) => write!(f, "expected {HASH_BYTES} bytes, got {len}"),
            Self::HexLength(len) => write!(f, "expected {} hex digits, got {len}", 2 * HASH_BYTES),
            Self::Digit(digit) => write!(f, "invalid hex digit {digit:?}"),
        }
    }
}

impl std::error::Error for ParseHashError {}

pub fn hash(data: &[u8]) -> Hash {
    Hash(Sha256::digest(data).into())
}

pub fn hashv(parts: &[&[u8]]) -> Hash {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    Hash(hasher.finalize().into())
}

const SHA256_IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Applies [`hash`] `n` times, starting from `start`.
///
/// A 32-byte message plus SHA-256 padding fits in one 64-byte block, so each
/// step is a single compression with the padding written once up front.
pub fn hash_n(start: &Hash, n: u64) -> Hash {
    let mut block = [0u8; 64];
    block[..HASH_BYTES].copy_from_slice(&start.0);
    block[HASH_BYTES] = 0x80;
    block[62] = 0x01; // message length, 256 bits, big-endian
    for _ in 0..n {
        let mut state = SHA256_IV;
        compress256(&mut state, std::slice::from_ref(&block));
        for (out, word) in block.as_chunks_mut().0.iter_mut().zip(state) {
            *out = word.to_be_bytes();
        }
    }
    let mut out = [0u8; HASH_BYTES];
    out.copy_from_slice(&block[..HASH_BYTES]);
    Hash(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_known_digest() {
        assert_eq!(
            hash(b"abc").to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hashv(&[b"a", b"bc"]), hash(b"abc"));
    }

    #[test]
    fn hash_n_matches_naive_loop() {
        let mut naive = Hash::default();
        for n in 0..100 {
            assert_eq!(hash_n(&Hash::default(), n), naive);
            naive = hash(naive.as_ref());
        }
    }

    #[test]
    fn hex_round_trip() {
        let h = hash(b"solana");
        assert_eq!(h.to_string().parse::<Hash>(), Ok(h));
        assert_eq!(h.to_string().to_uppercase().parse::<Hash>(), Ok(h));
        assert_eq!("00".parse::<Hash>(), Err(ParseHashError::HexLength(2)));
        assert_eq!(
            "zz".repeat(32).parse::<Hash>(),
            Err(ParseHashError::Digit('z'))
        );
    }

    #[test]
    fn try_from_slice_checks_length() {
        assert_eq!(Hash::try_from(&[7u8; 32][..]), Ok(Hash::new([7; 32])));
        assert_eq!(
            Hash::try_from(&[7u8; 31][..]),
            Err(ParseHashError::Length(31))
        );
    }
}
