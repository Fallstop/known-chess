//! The position book: the shared, compact, fast-lookup format.
//!
//! # Why the format looks like this
//!
//! Position keys are uniform 64-bit Zobrist hashes, and almost every entry is
//! a position seen exactly once with a single move. A fixed-width table burns
//! ~22 bytes on each of those; this format gets the same information into ~6
//! by spending bits only where there is actual entropy:
//!
//! * Entries are sorted by hash, so instead of raw 8-byte keys we store the
//!   *gaps* between consecutive hashes, Golomb-Rice coded. For n uniform
//!   64-bit keys a gap costs ~(64 − log₂ n) + 2 bits — near the
//!   information-theoretic floor for storing an n-element set — instead of 64.
//! * A move is an 8-bit index into the position's legal moves sorted by their
//!   [`crate::EncodedMove`] packing (see [`crate::canonical_legal`]). Chess
//!   positions never have more than 218 legal moves, and every consumer holds
//!   the live position when it needs the concrete move back.
//! * Move counts (and the per-position move count) are Elias-gamma coded:
//!   the overwhelmingly common count of 1 is a single bit. Counts are u64
//!   end-to-end (the v1 format saturated at u32).
//!
//! # On-disk layout (all little-endian, bitstreams LSB-first within bytes)
//!
//! ```text
//! header (24 bytes)
//!   magic    [u8; 8] = b"KNCHESS2"
//!   version  u32     = FORMAT_VERSION
//!   count    u64     = number of position entries
//!   rice_k   u8      = Golomb-Rice parameter for hash gaps
//!   _pad     [u8; 3]
//!
//! block index  (ceil(count / 256) records of 16 bytes)
//!   first_hash u64   hash of the block's first entry, stored in full
//!   data_off   u64   byte offset of the block within the data section
//!
//! data: one bitstream per block of up to 256 entries, byte-aligned at start
//!   entry 0:   hash implicit (the index's first_hash)
//!   entry i>0: Rice(rice_k) of (hash − prev_hash − 1)
//!   then per entry:
//!     nmoves   Elias-gamma
//!     per move (most-played first):
//!       index  8 bits        canonical legal-move index
//!       count  Elias-gamma
//! ```
//!
//! Lookup binary-searches the block index and decodes at most one 256-entry
//! block, so the server still answers straight out of an `mmap`. Incremental
//! builds use [`write_merged`] to stream-merge the existing book with new
//! sorted entries — the old book is never loaded back into memory, which keeps
//! fold RAM proportional to the *new* month, not the whole history.

use std::collections::HashMap;
use std::io::{self, Write};

pub const MAGIC: &[u8; 8] = b"KNCHESS2";
pub const FORMAT_VERSION: u32 = 2;

const HEADER_LEN: usize = 24;
const INDEX_REC_LEN: usize = 16;
const BLOCK_ENTRIES: u64 = 256;
/// A Rice quotient this long escapes to a raw 64-bit value, bounding the
/// unary run for adversarially large gaps.
const RICE_ESCAPE: u32 = 24;

/// One move played from a position: its canonical legal-move index (see
/// [`crate::canonical_legal`]) and how many games chose it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveStat {
    pub index: u8,
    pub count: u64,
}

// ---------------------------------------------------------------------------
// Bit-level primitives
// ---------------------------------------------------------------------------

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    acc: u64,
    len: u32,
}

impl BitWriter {
    /// Append the low `n` (≤ 32) bits of `val`.
    fn push(&mut self, val: u64, n: u32) {
        if n == 0 {
            return;
        }
        self.acc |= (val & ((1u64 << n) - 1)) << self.len;
        self.len += n;
        while self.len >= 8 {
            self.bytes.push(self.acc as u8);
            self.acc >>= 8;
            self.len -= 8;
        }
    }

    /// Append the low `n` (≤ 64) bits of `val`.
    fn push_bits(&mut self, val: u64, n: u32) {
        if n > 32 {
            self.push(val, 32);
            self.push(val >> 32, n - 32);
        } else {
            self.push(val, n);
        }
    }

    /// Append `q` one-bits (no terminator).
    fn push_ones(&mut self, q: u32) {
        let mut rem = q;
        while rem >= 32 {
            self.push(u64::MAX, 32);
            rem -= 32;
        }
        self.push(u64::MAX, rem);
    }

    /// Pad with zero bits to the next byte boundary.
    fn align(&mut self) {
        if self.len > 0 {
            self.push(0, 8 - self.len);
        }
    }

    /// Current length in whole bytes; exact after [`BitWriter::align`].
    fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    fn finish(mut self) -> Vec<u8> {
        self.align();
        self.bytes
    }
}

struct BitReader<'a> {
    bytes: &'a [u8],
    pos: usize,
    acc: u64,
    len: u32,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0, acc: 0, len: 0 }
    }

    /// Read `n` (≤ 32) bits. Reads past the end yield zeros rather than
    /// panicking; callers bound their decoding by the entry counts in the
    /// header, so this only matters for corrupt files.
    fn read(&mut self, n: u32) -> u64 {
        if n == 0 {
            return 0;
        }
        while self.len < n {
            let byte = self.bytes.get(self.pos).copied().unwrap_or(0);
            self.acc |= u64::from(byte) << self.len;
            self.pos += 1;
            self.len += 8;
        }
        let val = self.acc & ((1u64 << n) - 1);
        self.acc >>= n;
        self.len -= n;
        val
    }

    /// Read `n` (≤ 64) bits.
    fn read_bits(&mut self, n: u32) -> u64 {
        if n > 32 {
            let lo = self.read(32);
            lo | (self.read(n - 32) << 32)
        } else {
            self.read(n)
        }
    }

    /// Count one-bits up to `cap`. Consumes the terminating zero unless `cap`
    /// is reached first.
    fn read_ones(&mut self, cap: u32) -> u32 {
        let mut q = 0;
        while q < cap {
            if self.read(1) == 1 {
                q += 1;
            } else {
                return q;
            }
        }
        q
    }
}

/// Golomb-Rice parameter tuned to the expected gap between `n` sorted uniform
/// 64-bit keys: k ≈ log₂(2⁶⁴ / n) puts the average quotient near 1.
fn rice_k_for(n: u64) -> u8 {
    if n == 0 {
        return 63;
    }
    (u64::MAX / n).max(1).ilog2() as u8
}

fn rice_encode(w: &mut BitWriter, v: u64, k: u32) {
    let q = v >> k;
    if q < u64::from(RICE_ESCAPE) {
        w.push_ones(q as u32);
        w.push(0, 1);
        w.push_bits(v, k);
    } else {
        w.push_ones(RICE_ESCAPE);
        w.push_bits(v, 64);
    }
}

fn rice_decode(r: &mut BitReader, k: u32) -> u64 {
    let q = r.read_ones(RICE_ESCAPE);
    if q == RICE_ESCAPE {
        r.read_bits(64)
    } else {
        (u64::from(q) << k) | r.read_bits(k)
    }
}

/// Elias-gamma for `v ≥ 1`: with n = ⌊log₂ v⌋, emit n one-bits, a zero, then
/// the n low bits of v − 2ⁿ. v = 1 costs a single bit.
fn gamma_encode(w: &mut BitWriter, v: u64) {
    debug_assert!(v >= 1);
    let n = v.ilog2();
    w.push_ones(n);
    w.push(0, 1);
    w.push_bits(v - (1u64 << n), n);
}

fn gamma_decode(r: &mut BitReader) -> u64 {
    let n = r.read_ones(64);
    (1u64 << n) + r.read_bits(n)
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Per-position move tallies. Nearly every position has exactly one recorded
/// move, so that case stays inline; the rare multi-move positions spill to a
/// boxed vec, keeping the builder's per-entry footprint at 16 bytes.
enum Slot {
    One(u8, u64),
    Many(Box<Vec<(u8, u64)>>),
}

impl Slot {
    fn add(&mut self, index: u8, n: u64) {
        match self {
            Slot::One(i, c) if *i == index => *c += n,
            Slot::One(i, c) => {
                *self = Slot::Many(Box::new(vec![(*i, *c), (index, n)]));
            }
            Slot::Many(v) => match v.iter_mut().find(|(i, _)| *i == index) {
                Some(entry) => entry.1 += n,
                None => v.push((index, n)),
            },
        }
    }

    fn append_to(&self, out: &mut Vec<(u8, u64)>) {
        match self {
            Slot::One(i, c) => out.push((*i, *c)),
            Slot::Many(v) => out.extend_from_slice(v),
        }
    }
}

/// Accumulates (position, move-index) tallies for *new* games, then sorts them
/// for a streaming merge with the existing book via [`write_merged`].
#[derive(Default)]
pub struct BookBuilder {
    positions: HashMap<u64, Slot>,
}

impl BookBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that the move with canonical legal-move `index` was played from
    /// the position with the given `hash`.
    pub fn record(&mut self, hash: u64, index: u8) {
        self.record_n(hash, index, 1);
    }

    pub fn record_n(&mut self, hash: u64, index: u8, n: u64) {
        match self.positions.entry(hash) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(Slot::One(index, n));
            }
            std::collections::hash_map::Entry::Occupied(mut e) => e.get_mut().add(index, n),
        }
    }

    pub fn position_count(&self) -> usize {
        self.positions.len()
    }

    /// Drain into hash-sorted entries ready for [`write_merged`].
    pub fn into_sorted(self) -> SortedEntries {
        let mut entries: Vec<(u64, Slot)> = self.positions.into_iter().collect();
        entries.sort_unstable_by_key(|(h, _)| *h);
        SortedEntries(entries)
    }
}

/// Hash-sorted output of [`BookBuilder::into_sorted`].
pub struct SortedEntries(Vec<(u64, Slot)>);

impl SortedEntries {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// What [`write_merged`] wrote, for logging.
#[derive(Debug, Clone, Copy)]
pub struct WriteStats {
    pub positions: u64,
    pub bytes: u64,
}

struct BookWriter {
    index: Vec<(u64, u64)>,
    data: BitWriter,
    rice_k: u8,
    count: u64,
    prev_hash: u64,
}

impl BookWriter {
    fn new(rice_k: u8) -> Self {
        Self {
            index: Vec::new(),
            data: BitWriter::default(),
            rice_k,
            count: 0,
            prev_hash: 0,
        }
    }

    /// Append one entry. `hash` must be strictly greater than the previous
    /// entry's, and `moves` already sorted most-played-first.
    fn push(&mut self, hash: u64, moves: &[(u8, u64)]) {
        if self.count % BLOCK_ENTRIES == 0 {
            self.data.align();
            self.index.push((hash, self.data.byte_len() as u64));
        } else {
            debug_assert!(hash > self.prev_hash);
            rice_encode(&mut self.data, hash - self.prev_hash - 1, u32::from(self.rice_k));
        }
        gamma_encode(&mut self.data, moves.len() as u64);
        for &(index, count) in moves {
            self.data.push(u64::from(index), 8);
            gamma_encode(&mut self.data, count);
        }
        self.prev_hash = hash;
        self.count += 1;
    }

    fn finish<W: Write>(self, out: &mut W) -> io::Result<WriteStats> {
        let data = self.data.finish();
        let mut head = Vec::with_capacity(HEADER_LEN + self.index.len() * INDEX_REC_LEN);
        head.extend_from_slice(MAGIC);
        head.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        head.extend_from_slice(&self.count.to_le_bytes());
        head.push(self.rice_k);
        head.extend_from_slice(&[0u8; 3]);
        for (first_hash, off) in &self.index {
            head.extend_from_slice(&first_hash.to_le_bytes());
            head.extend_from_slice(&off.to_le_bytes());
        }
        out.write_all(&head)?;
        out.write_all(&data)?;
        out.flush()?;
        Ok(WriteStats {
            positions: self.count,
            bytes: (head.len() + data.len()) as u64,
        })
    }
}

/// Sum duplicate move indexes, then order most-played-first (ties by index).
fn combine_moves(moves: &mut Vec<(u8, u64)>) {
    moves.sort_unstable_by_key(|m| m.0);
    moves.dedup_by(|next, kept| {
        if next.0 == kept.0 {
            kept.1 += next.1;
            true
        } else {
            false
        }
    });
    moves.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
}

/// Stream-merge an existing book (if any) with newly built entries, writing a
/// complete new book to `out`. Move counts for positions present in both are
/// summed; neither input is held in memory beyond one entry at a time.
pub fn write_merged<B: AsRef<[u8]>, W: Write>(
    old: Option<&Book<B>>,
    new: &SortedEntries,
    mut out: W,
) -> io::Result<WriteStats> {
    let estimate = old.map_or(0, |b| b.position_count()) + new.0.len() as u64;
    let mut writer = BookWriter::new(rice_k_for(estimate));

    let mut old_it = old.map(|b| b.iter().peekable());
    let mut new_it = new.0.iter().peekable();
    let mut moves: Vec<(u8, u64)> = Vec::new();

    loop {
        let old_hash = old_it.as_mut().and_then(|it| it.peek().map(|(h, _)| *h));
        let new_hash = new_it.peek().map(|(h, _)| *h);
        moves.clear();
        let hash = match (old_hash, new_hash) {
            (None, None) => break,
            (Some(oh), Some(nh)) if oh == nh => {
                let (_, old_moves) = old_it.as_mut().unwrap().next().unwrap();
                moves.extend(old_moves.iter().map(|m| (m.index, m.count)));
                let (_, slot) = new_it.next().unwrap();
                slot.append_to(&mut moves);
                oh
            }
            (Some(oh), nh) if nh.is_none_or(|nh| oh < nh) => {
                let (_, old_moves) = old_it.as_mut().unwrap().next().unwrap();
                moves.extend(old_moves.iter().map(|m| (m.index, m.count)));
                oh
            }
            (_, Some(nh)) => {
                let (_, slot) = new_it.next().unwrap();
                slot.append_to(&mut moves);
                nh
            }
            (Some(_), None) => unreachable!("covered by the old-only arm"),
        };
        combine_moves(&mut moves);
        writer.push(hash, &moves);
    }
    writer.finish(&mut out)
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

/// A read-only view over a serialized book.
///
/// Borrows the backing bytes (typically an `mmap`), so construction is just
/// header validation — no parsing or allocation.
pub struct Book<B: AsRef<[u8]>> {
    bytes: B,
    count: u64,
    n_blocks: usize,
    rice_k: u32,
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
        let count = u64::from_le_bytes(buf[12..20].try_into().unwrap());
        let rice_k = buf[20];
        if rice_k >= 64 {
            return Err(BookError::Corrupt("rice parameter out of range"));
        }
        let n_blocks = usize::try_from(count.div_ceil(BLOCK_ENTRIES))
            .map_err(|_| BookError::Corrupt("entry count overflows"))?;
        if buf.len() < HEADER_LEN + n_blocks * INDEX_REC_LEN {
            return Err(BookError::Truncated);
        }
        Ok(Self {
            bytes,
            count,
            n_blocks,
            rice_k: u32::from(rice_k),
        })
    }

    pub fn position_count(&self) -> u64 {
        self.count
    }

    fn block_index(&self, block: usize) -> (u64, u64) {
        let off = HEADER_LEN + block * INDEX_REC_LEN;
        let buf = self.bytes.as_ref();
        (
            u64::from_le_bytes(buf[off..off + 8].try_into().unwrap()),
            u64::from_le_bytes(buf[off + 8..off + 16].try_into().unwrap()),
        )
    }

    fn data(&self) -> &[u8] {
        &self.bytes.as_ref()[HEADER_LEN + self.n_blocks * INDEX_REC_LEN..]
    }

    fn block_len(&self, block: usize) -> u64 {
        (self.count - block as u64 * BLOCK_ENTRIES).min(BLOCK_ENTRIES)
    }

    /// Look up the moves played from `hash`, most popular first. Returns an
    /// empty vec for an unknown position.
    pub fn lookup(&self, hash: u64) -> Vec<MoveStat> {
        if self.count == 0 {
            return Vec::new();
        }
        // Last block whose first hash is ≤ the target.
        let (mut lo, mut hi) = (0usize, self.n_blocks);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.block_index(mid).0 <= hash {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo == 0 {
            return Vec::new();
        }
        let block = lo - 1;
        let (first_hash, data_off) = self.block_index(block);
        let mut reader = BitReader::new(&self.data()[data_off as usize..]);
        let mut cur = first_hash;
        for i in 0..self.block_len(block) {
            if i > 0 {
                cur += 1 + rice_decode(&mut reader, self.rice_k);
            }
            if cur > hash {
                break;
            }
            if cur == hash {
                return decode_moves(&mut reader);
            }
            skip_moves(&mut reader);
        }
        Vec::new()
    }

    /// Iterate every entry in hash order. Used for streaming merges.
    pub fn iter(&self) -> BookIter<'_, B> {
        BookIter {
            book: self,
            remaining: self.count,
            block: 0,
            in_block: 0,
            cur_hash: 0,
            reader: None,
        }
    }
}

fn decode_moves(reader: &mut BitReader) -> Vec<MoveStat> {
    let nmoves = gamma_decode(reader) as usize;
    (0..nmoves)
        .map(|_| MoveStat {
            index: reader.read(8) as u8,
            count: gamma_decode(reader),
        })
        .collect()
}

fn skip_moves(reader: &mut BitReader) {
    let nmoves = gamma_decode(reader);
    for _ in 0..nmoves {
        reader.read(8);
        gamma_decode(reader);
    }
}

/// Sequential decoder over a book's entries, in hash order.
pub struct BookIter<'a, B: AsRef<[u8]>> {
    book: &'a Book<B>,
    remaining: u64,
    block: usize,
    in_block: u64,
    cur_hash: u64,
    reader: Option<BitReader<'a>>,
}

impl<B: AsRef<[u8]>> Iterator for BookIter<'_, B> {
    type Item = (u64, Vec<MoveStat>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        if self.in_block == 0 {
            let (first_hash, data_off) = self.book.block_index(self.block);
            self.reader = Some(BitReader::new(&self.book.data()[data_off as usize..]));
            self.cur_hash = first_hash;
        }
        let reader = self.reader.as_mut().expect("reader set at block start");
        if self.in_block > 0 {
            self.cur_hash += 1 + rice_decode(reader, self.book.rice_k);
        }
        let moves = decode_moves(reader);

        self.remaining -= 1;
        self.in_block += 1;
        if self.in_block == BLOCK_ENTRIES {
            self.in_block = 0;
            self.block += 1;
        }
        Some((self.cur_hash, moves))
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
    #[error("book file is corrupt: {0}")]
    Corrupt(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fresh(builder: BookBuilder) -> Vec<u8> {
        let mut out = Vec::new();
        write_merged(None::<&Book<&[u8]>>, &builder.into_sorted(), &mut out).unwrap();
        out
    }

    #[test]
    fn bit_codecs_round_trip() {
        let mut w = BitWriter::default();
        let values = [0u64, 1, 2, 23, 24, 25, 1000, 1 << 38, u64::MAX / 3, u64::MAX];
        for &v in &values {
            rice_encode(&mut w, v, 38);
            rice_encode(&mut w, v, 0); // worst case: every quotient escapes
        }
        let gammas = [1u64, 2, 3, 7, 8, 1_000_000, u64::MAX];
        for &v in &gammas {
            gamma_encode(&mut w, v);
        }
        let bytes = w.finish();
        let mut r = BitReader::new(&bytes);
        for &v in &values {
            assert_eq!(rice_decode(&mut r, 38), v);
            assert_eq!(rice_decode(&mut r, 0), v);
        }
        for &v in &gammas {
            assert_eq!(gamma_decode(&mut r), v);
        }
    }

    #[test]
    fn round_trip() {
        let mut b = BookBuilder::new();
        b.record(42, 3);
        b.record(42, 3);
        b.record(42, 7);
        b.record(7, 0);
        let bytes = write_fresh(b);

        let book = Book::open(bytes.as_slice()).unwrap();
        assert_eq!(book.position_count(), 2);

        let moves = book.lookup(42);
        assert_eq!(moves.len(), 2);
        // Most popular first.
        assert_eq!(moves[0], MoveStat { index: 3, count: 2 });
        assert_eq!(moves[1], MoveStat { index: 7, count: 1 });

        assert_eq!(book.lookup(7).len(), 1);
        assert!(book.lookup(999).is_empty());
        assert!(book.lookup(0).is_empty());
    }

    #[test]
    fn empty_book() {
        let bytes = write_fresh(BookBuilder::new());
        let book = Book::open(bytes.as_slice()).unwrap();
        assert_eq!(book.position_count(), 0);
        assert!(book.lookup(123).is_empty());
        assert_eq!(book.iter().count(), 0);
    }

    /// Deterministic pseudo-random hashes (an LCG) spanning many blocks.
    fn test_hashes(n: usize) -> Vec<u64> {
        let mut x: u64 = 0x9E3779B97F4A7C15;
        let mut v: Vec<u64> = (0..n)
            .map(|_| {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                x
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    #[test]
    fn multi_block_lookup_and_iter() {
        let hashes = test_hashes(1500);
        let mut b = BookBuilder::new();
        for (i, &h) in hashes.iter().enumerate() {
            b.record_n(h, (i % 200) as u8, (i as u64 % 5) + 1);
        }
        let bytes = write_fresh(b);
        let book = Book::open(bytes.as_slice()).unwrap();
        assert_eq!(book.position_count(), hashes.len() as u64);

        for (i, &h) in hashes.iter().enumerate() {
            let moves = book.lookup(h);
            assert_eq!(moves.len(), 1, "hash #{i}");
            assert_eq!(moves[0], MoveStat { index: (i % 200) as u8, count: (i as u64 % 5) + 1 });
            // Neighbouring non-members miss.
            if !hashes.contains(&(h ^ 1)) {
                assert!(book.lookup(h ^ 1).is_empty());
            }
        }

        let decoded: Vec<u64> = book.iter().map(|(h, _)| h).collect();
        assert_eq!(decoded, hashes);
    }

    #[test]
    fn merge_sums_and_unions() {
        let mut a = BookBuilder::new();
        a.record_n(10, 1, 5);
        a.record_n(20, 2, 1);
        a.record_n(20, 3, 9);
        let base = write_fresh(a);
        let base = Book::open(base.as_slice()).unwrap();

        let mut b = BookBuilder::new();
        b.record_n(20, 2, 4); // sums with existing index 2
        b.record_n(20, 5, 1); // new move at a known position
        b.record_n(15, 0, 2); // brand-new position interleaved between 10 and 20
        let mut merged = Vec::new();
        let stats = write_merged(Some(&base), &b.into_sorted(), &mut merged).unwrap();
        assert_eq!(stats.positions, 3);
        assert_eq!(stats.bytes, merged.len() as u64);

        let book = Book::open(merged.as_slice()).unwrap();
        assert_eq!(book.lookup(10), vec![MoveStat { index: 1, count: 5 }]);
        assert_eq!(book.lookup(15), vec![MoveStat { index: 0, count: 2 }]);
        assert_eq!(
            book.lookup(20),
            vec![
                MoveStat { index: 3, count: 9 },
                MoveStat { index: 2, count: 5 },
                MoveStat { index: 5, count: 1 },
            ]
        );
    }

    #[test]
    fn merge_at_scale_preserves_everything() {
        let hashes = test_hashes(3000);
        let (first_half, second_half) = hashes.split_at(1500);

        let mut a = BookBuilder::new();
        for &h in first_half {
            a.record(h, 1);
        }
        let base = write_fresh(a);
        let base = Book::open(base.as_slice()).unwrap();

        let mut b = BookBuilder::new();
        for &h in second_half {
            b.record(h, 2);
        }
        for &h in first_half.iter().step_by(3) {
            b.record(h, 1); // bump every third existing entry to count 2
        }
        let mut merged = Vec::new();
        write_merged(Some(&base), &b.into_sorted(), &mut merged).unwrap();
        let book = Book::open(merged.as_slice()).unwrap();

        assert_eq!(book.position_count(), hashes.len() as u64);
        for (i, &h) in first_half.iter().enumerate() {
            let expected = if i % 3 == 0 { 2 } else { 1 };
            assert_eq!(book.lookup(h), vec![MoveStat { index: 1, count: expected }], "old #{i}");
        }
        for &h in second_half {
            assert_eq!(book.lookup(h), vec![MoveStat { index: 2, count: 1 }]);
        }
    }
}
