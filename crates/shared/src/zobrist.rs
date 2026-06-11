//! Position hashing.
//!
//! We identify a chess position by its 64-bit Zobrist hash. Two positions with
//! the same hash are treated as the same node in the opening tree, which is
//! exactly what we want: transpositions (different move orders reaching the same
//! position) collapse onto one another, so the book stays small and lookups are
//! a single `u64` key.

use shakmaty::zobrist::{Zobrist64, ZobristHash};
use shakmaty::{EnPassantMode, Position};

/// Compute the canonical Zobrist hash for a position.
///
/// [`EnPassantMode::Legal`] folds the en-passant square into the hash only when
/// an en-passant capture is actually legal, so positions that look different but
/// play identically share a hash.
pub fn position_hash<P: Position + ZobristHash>(pos: &P) -> u64 {
    let Zobrist64(hash) = pos.zobrist_hash(EnPassantMode::Legal);
    hash
}
