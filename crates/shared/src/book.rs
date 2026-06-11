//! The position book: the shared, fast-lookup format.
//!
//! # On-disk layout (all little-endian)
//!
//! ```text
//! magic    [u8; 8]  = b"KNCHESS1"
//! version  u32      = FORMAT_VERSION
//! count    u32      = number of position entries
//!
//! entry table  (count * 16 bytes, sorted ascending by hash)
//!   hash       u64
//!   moves_off  u32   byte offset into the moves blob
//!   moves_len  u16   number of move records at this position
//!   _pad       u16
//!
//! moves blob   (records of 6 bytes each)
//!   mv         u16   EncodedMove
//!   count      u32   number of games that played this move from here
//! ```
//!
//! Lookup is a binary search over the fixed-width entry table — O(log n) with no
//! allocation and no deserialization, so the server can `mmap` the file and
//! answer queries straight out of the page cache.

use std::collections::HashMap;

use crate::mv::EncodedMove;

pub const MAGIC: &[u8; 8] = b"KNCHESS1";
pub const FORMAT_VERSION: u32 = 1;

const HEADER_LEN: usize = 16;
const ENTRY_LEN: usize = 16;
const MOVE_REC_LEN: usize = 6;

/// One move played from a position, with how many games chose it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveStat {
    pub mv: EncodedMove,
    pub count: u32,
}

/// Accumulates positions and move counts, then serializes the book.
///
/// The processor walks every game, and for each ply records "from this position,
/// this move was played" via [`BookBuilder::record`].
#[derive(Default)]
pub struct BookBuilder {
    positions: HashMap<u64, HashMap<u16, u64>>,
}

impl BookBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `mv` was played from the position with the given `hash`.
    pub fn record(&mut self, hash: u64, mv: EncodedMove) {
        *self
            .positions
            .entry(hash)
            .or_default()
            .entry(mv.0)
            .or_insert(0) += 1;
    }

    pub fn position_count(&self) -> usize {
        self.positions.len()
    }

    /// Serialize the accumulated data into the on-disk format.
    pub fn into_bytes(self) -> Vec<u8> {
        let mut entries: Vec<(u64, &HashMap<u16, u64>)> =
            self.positions.iter().map(|(h, m)| (*h, m)).collect();
        entries.sort_unstable_by_key(|(h, _)| *h);

        let count = entries.len();
        let moves_blob_start = HEADER_LEN + count * ENTRY_LEN;

        let mut out = Vec::with_capacity(moves_blob_start);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&(count as u32).to_le_bytes());

        // First pass: emit the entry table, tracking each position's offset into
        // the moves blob that we build in the second pass.
        let mut moves_blob: Vec<u8> = Vec::new();
        for (hash, moves) in &entries {
            let moves_off = moves_blob.len() as u32;
            let moves_len = moves.len() as u16;

            out.extend_from_slice(&hash.to_le_bytes());
            out.extend_from_slice(&moves_off.to_le_bytes());
            out.extend_from_slice(&moves_len.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // padding

            // Emit move records sorted by descending popularity for nicer reads.
            let mut recs: Vec<(u16, u64)> = moves.iter().map(|(m, c)| (*m, *c)).collect();
            recs.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            for (mv, cnt) in recs {
                moves_blob.extend_from_slice(&mv.to_le_bytes());
                moves_blob.extend_from_slice(&(cnt.min(u32::MAX as u64) as u32).to_le_bytes());
            }
        }

        out.extend_from_slice(&moves_blob);
        out
    }
}

/// A read-only view over a serialized book.
///
/// Borrows the backing bytes (typically an `mmap`), so construction is just
/// header validation — no parsing or allocation.
pub struct Book<B: AsRef<[u8]>> {
    bytes: B,
    count: usize,
    moves_blob_start: usize,
}

impl<B: AsRef<[u8]>> Book<B> {
    pub fn open(bytes: B) -> Result<Self, BookError> {
        let buf = bytes.as_ref();
        if buf.len() < HEADER_LEN {
            return Err(BookError::Truncated);
        }
        if &buf[0..8] != MAGIC {
            return Err(BookError::BadMagic);
        }
        let version = u32::from_le_bytes(buf[8..12].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(BookError::Version(version));
        }
        let count = u32::from_le_bytes(buf[12..16].try_into().unwrap()) as usize;
        let moves_blob_start = HEADER_LEN + count * ENTRY_LEN;
        if buf.len() < moves_blob_start {
            return Err(BookError::Truncated);
        }
        Ok(Self {
            bytes,
            count,
            moves_blob_start,
        })
    }

    pub fn position_count(&self) -> usize {
        self.count
    }

    /// Look up the moves played from `hash`. Returns an empty vec for an unknown
    /// position. The result is sorted most-popular-first.
    pub fn lookup(&self, hash: u64) -> Vec<MoveStat> {
        match self.find_entry(hash) {
            Some(idx) => self.read_moves(idx),
            None => Vec::new(),
        }
    }

    fn entry_hash(&self, idx: usize) -> u64 {
        let off = HEADER_LEN + idx * ENTRY_LEN;
        u64::from_le_bytes(self.bytes.as_ref()[off..off + 8].try_into().unwrap())
    }

    fn find_entry(&self, hash: u64) -> Option<usize> {
        let (mut lo, mut hi) = (0usize, self.count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.entry_hash(mid).cmp(&hash) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    fn read_moves(&self, idx: usize) -> Vec<MoveStat> {
        let buf = self.bytes.as_ref();
        let off = HEADER_LEN + idx * ENTRY_LEN;
        let moves_off = u32::from_le_bytes(buf[off + 8..off + 12].try_into().unwrap()) as usize;
        let moves_len = u16::from_le_bytes(buf[off + 12..off + 14].try_into().unwrap()) as usize;

        let base = self.moves_blob_start + moves_off;
        (0..moves_len)
            .map(|i| {
                let r = base + i * MOVE_REC_LEN;
                let mv = u16::from_le_bytes(buf[r..r + 2].try_into().unwrap());
                let count = u32::from_le_bytes(buf[r + 2..r + 6].try_into().unwrap());
                MoveStat {
                    mv: EncodedMove(mv),
                    count,
                }
            })
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BookError {
    #[error("not a known-chess book (bad magic)")]
    BadMagic,
    #[error("unsupported book format version {0}")]
    Version(u32),
    #[error("book file is truncated")]
    Truncated,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut b = BookBuilder::new();
        b.record(42, EncodedMove(100));
        b.record(42, EncodedMove(100));
        b.record(42, EncodedMove(200));
        b.record(7, EncodedMove(5));

        let bytes = b.into_bytes();
        let book = Book::open(bytes.as_slice()).unwrap();
        assert_eq!(book.position_count(), 2);

        let moves = book.lookup(42);
        assert_eq!(moves.len(), 2);
        // Most popular first.
        assert_eq!(moves[0], MoveStat { mv: EncodedMove(100), count: 2 });
        assert_eq!(moves[1], MoveStat { mv: EncodedMove(200), count: 1 });

        assert_eq!(book.lookup(7).len(), 1);
        assert!(book.lookup(999).is_empty());
    }
}
