//! Shared types for known-chess.
//!
//! This crate defines the data that flows between the `processor` (which turns
//! a lichess PGN dump into a position book) and the `server` (which serves that
//! book to the frontend). Both sides agree on:
//!
//! * how a position is hashed ([`zobrist`]),
//! * how a move is packed into 16 bits ([`mv`]),
//! * and the on-disk book format ([`book`]).

pub mod book;
pub mod config;
pub mod mv;
pub mod zobrist;

pub use book::{
    write_merged, write_merged_many, write_merged_many_progress, Book, BookBuilder, MoveStat,
    SortedEntries, WriteStats,
};
pub use config::Config;
pub use mv::{canonical_index, canonical_legal, EncodedMove};
pub use zobrist::position_hash;
