//! `kc-process`: manage lichess dumps and turn them into known-chess books.
//!
//! Storage locations come from `config.toml` (see [`shared::config`]); dumps are
//! downloaded into `[storage].downloads` and books written to `[storage].books`.
//!
//! ```text
//! kc-process list [QUERY]          # show months, with downloaded/processed status
//! kc-process get 2026-05 latest    # download dumps for the given month tags
//! kc-process build 2026-05         # process downloaded dump(s) into book(s)
//! kc-process build dump.pgn.zst --max-ply 40 --output data/book.book
//! ```

mod build;
mod fetch;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use shared::Config;

#[derive(Parser)]
#[command(about = "Download lichess dumps and process them into known-chess books")]
struct Cli {
    /// Path to config.toml (otherwise KC_CONFIG, then the nearest config.toml).
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List available months and whether they're downloaded/processed.
    #[command(visible_aliases = ["ls", "search"])]
    List {
        /// Only show months whose tag contains this (e.g. 2025 or 2026-05).
        query: Option<String>,
    },

    /// Download dumps for one or more month tags (e.g. 2026-05, 2025, latest).
    Get {
        /// Month tags to download. A bare year (e.g. 2014) selects all of its
        /// months.
        #[arg(required = true)]
        tags: Vec<String>,

        /// How many dumps to download in parallel (max 5).
        #[arg(short, long, default_value_t = 5, value_parser = clap::value_parser!(u8).range(1..=5))]
        jobs: u8,
    },

    /// Fold dump(s) into the combined book. With no targets, processes every
    /// downloaded dump not yet in the book (catch-up mode).
    Build {
        /// Month tags (e.g. 2026-05) and/or paths to .pgn[.zst] files. Empty =
        /// catch up on all downloaded-but-unprocessed dumps.
        targets: Vec<String>,

        /// Stop recording each game after this many plies (0 = whole game).
        #[arg(long, default_value_t = 0)]
        max_ply: usize,

        /// Process at most this many games per input, counted before the
        /// game-ending filter (0 = all). Handy for tests.
        #[arg(long, default_value_t = 0)]
        limit: u64,

        /// Keep games regardless of how they ended (default: only board endings).
        #[arg(long, default_value_t = false)]
        any_ending: bool,

        /// Parser threads (0 = one per CPU core).
        #[arg(short, long, default_value_t = 0)]
        jobs: usize,

        /// Write the combined book here instead of [storage].book.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Start a new book instead of adding to the existing combined book.
        #[arg(long, default_value_t = false)]
        fresh: bool,

        /// Spill accumulated positions to disk once worker memory crosses this
        /// many GiB, keeping peak RAM bounded when processing many dumps at
        /// once (0 = never spill).
        #[arg(long, default_value_t = 40)]
        max_mem_gb: u64,
    },

    /// Finish a `build` that crashed during the final merge by folding the
    /// leftover `<book>.spill*` files into the book — no dumps are re-parsed.
    MergeSpills {
        /// Month tags (e.g. 2018-01) or dump filenames the crashed build was
        /// processing, recorded in the `.sources` manifest so `list` shows them
        /// processed. Needed when the dumps were deleted after spilling, since
        /// they can no longer be auto-detected on disk.
        record: Vec<String>,

        /// The book whose spill files to merge (defaults to [storage].book).
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Merge only the spills into a new book, ignoring any existing one.
        #[arg(long, default_value_t = false)]
        fresh: bool,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = Config::load(cli.config.as_deref())?;

    match cli.command {
        Command::List { query } => fetch::cmd_list(&cfg, query.as_deref()),
        Command::Get { tags, jobs } => fetch::cmd_get(&cfg, &tags, jobs as usize),
        Command::Build {
            targets,
            max_ply,
            limit,
            any_ending,
            jobs,
            output,
            fresh,
            max_mem_gb,
        } => build::run(
            &cfg,
            &targets,
            &build::BuildOpts {
                max_ply,
                limit,
                any_ending,
                jobs,
                output,
                fresh,
                max_mem_bytes: max_mem_gb.saturating_mul(1 << 30),
            },
        ),
        Command::MergeSpills { record, output, fresh } => {
            build::resume_merge(&cfg, output, fresh, &record)
        }
    }
}
