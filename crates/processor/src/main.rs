//! `kc-process` — turn a lichess PGN dump into a known-chess book.
//!
//! Usage:
//!
//! ```text
//! kc-process --input lichess.pgn.zst --output data/book.book [--max-ply 0]
//! ```
//!
//! The input may be a plain `.pgn`, a zstd-compressed `.pgn.zst` (as lichess
//! ships them), or `-` for stdin. Output is the binary book described in
//! `shared::book`.

use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use pgn_reader::{BufferedReader, RawHeader, SanPlus, Skip, Visitor};
use shakmaty::{Chess, Position};
use shared::{position_hash, BookBuilder, EncodedMove};

#[derive(Parser)]
#[command(about = "Process a lichess PGN dump into a known-chess book")]
struct Args {
    /// Input PGN file (.pgn or .pgn.zst), or `-` for stdin.
    #[arg(short, long)]
    input: String,

    /// Output book file.
    #[arg(short, long, default_value = "data/book.book")]
    output: PathBuf,

    /// Stop recording each game after this many plies (0 = whole game).
    ///
    /// Capping keeps the book small for an opening trainer; leaving it at 0
    /// records full games so the "one known game plays itself out" endgame
    /// works all the way to the result.
    #[arg(long, default_value_t = 0)]
    max_ply: usize,

    /// Stop after reading this many games (0 = all). Handy for quick tests.
    #[arg(long, default_value_t = 0)]
    limit: u64,

    /// Keep games regardless of how they ended. By default only games whose
    /// final position is checkmate or stalemate are recorded, so every line in
    /// the book plays out to a real board result (a win, loss, or draw) rather
    /// than a resignation or flag-fall.
    #[arg(long, default_value_t = false)]
    any_ending: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    let reader = open_input(&args.input)?;
    let mut buffered = BufferedReader::new(reader);

    let mut visitor = BookVisitor::new(args.max_ply, args.any_ending);
    while buffered.read_game(&mut visitor)?.is_some() {
        if args.limit != 0 && visitor.games >= args.limit {
            break;
        }
        if visitor.games % 100_000 == 0 && visitor.games > 0 {
            tracing::info!(
                games = visitor.games,
                positions = visitor.builder.position_count(),
                "processing"
            );
        }
    }

    tracing::info!(
        games = visitor.games,
        positions = visitor.builder.position_count(),
        skipped = visitor.skipped_games,
        "done reading; serializing book"
    );

    let bytes = visitor.builder.into_bytes();
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut out = File::create(&args.output)
        .with_context(|| format!("creating {}", args.output.display()))?;
    out.write_all(&bytes)?;
    tracing::info!(bytes = bytes.len(), path = %args.output.display(), "wrote book");
    Ok(())
}

fn open_input(path: &str) -> Result<Box<dyn Read>> {
    if path == "-" {
        return Ok(Box::new(BufReader::new(io::stdin())));
    }
    let file = File::open(path).with_context(|| format!("opening {path}"))?;
    if path.ends_with(".zst") {
        Ok(Box::new(zstd::Decoder::new(file)?))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

struct BookVisitor {
    builder: BookBuilder,
    pos: Chess,
    max_ply: usize,
    require_board_end: bool,
    ply: usize,
    /// Set when the current game can't be replayed from the standard start
    /// (a variant or a `FEN` setup header) — we skip those entirely.
    skip_current: bool,
    /// Moves recorded for the in-flight game. We only flush these into the book
    /// once the game qualifies (see [`Self::require_board_end`]).
    pending: Vec<(u64, EncodedMove)>,
    games: u64,
    skipped_games: u64,
}

impl BookVisitor {
    fn new(max_ply: usize, any_ending: bool) -> Self {
        Self {
            builder: BookBuilder::new(),
            pos: Chess::default(),
            max_ply,
            require_board_end: !any_ending,
            ply: 0,
            skip_current: false,
            pending: Vec::new(),
            games: 0,
            skipped_games: 0,
        }
    }
}

impl Visitor for BookVisitor {
    type Result = ();

    fn begin_game(&mut self) {
        self.pos = Chess::default();
        self.ply = 0;
        self.skip_current = false;
        self.pending.clear();
    }

    fn header(&mut self, key: &[u8], value: RawHeader<'_>) {
        // Skip non-standard variants and games that start from a custom FEN —
        // our book assumes the standard initial position.
        match key {
            b"FEN" => self.skip_current = true,
            b"Variant" if value.as_bytes() != b"Standard" => self.skip_current = true,
            _ => {}
        }
    }

    fn end_headers(&mut self) -> Skip {
        // Skip accounting happens in `end_game`, which runs for every game.
        Skip(self.skip_current)
    }

    fn san(&mut self, san_plus: SanPlus) {
        if self.skip_current {
            return;
        }
        if self.max_ply != 0 && self.ply >= self.max_ply {
            self.skip_current = true; // ignore the rest of this game's moves
            return;
        }
        match san_plus.san.to_move(&self.pos) {
            Ok(mv) => {
                let hash = position_hash(&self.pos);
                self.pending.push((hash, EncodedMove::encode(&mv)));
                self.pos.play_unchecked(&mv);
                self.ply += 1;
            }
            Err(_) => {
                // Illegal/unparseable SAN — abandon the rest of this game.
                self.skip_current = true;
            }
        }
    }

    // We only want moves as actually played, not analysis sidelines.
    fn begin_variation(&mut self) -> Skip {
        Skip(true)
    }

    fn end_game(&mut self) -> Self::Result {
        // A game qualifies if it played at least one legal move and either we
        // accept any ending, or the final position is checkmate/stalemate.
        let board_ended = self.pos.is_checkmate() || self.pos.is_stalemate();
        let qualifies = !self.skip_current
            && !self.pending.is_empty()
            && (!self.require_board_end || board_ended);

        if qualifies {
            for (hash, mv) in self.pending.drain(..) {
                self.builder.record(hash, mv);
            }
            self.games += 1;
        } else {
            self.skipped_games += 1;
        }
        self.pending.clear();
    }
}
