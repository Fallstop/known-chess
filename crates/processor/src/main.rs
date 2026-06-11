//! `kc-process` — manage lichess dumps and turn them into known-chess books.
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
        /// Month tags to download.
        #[arg(required = true)]
        tags: Vec<String>,
    },

    /// Process downloaded dump(s) or PGN file(s) into book(s).
    Build {
        /// Month tags (e.g. 2026-05) and/or paths to .pgn[.zst] files.
        #[arg(required = true)]
        targets: Vec<String>,

        /// Stop recording each game after this many plies (0 = whole game).
        #[arg(long, default_value_t = 0)]
        max_ply: usize,

        /// Stop after this many games per input (0 = all). Handy for tests.
        #[arg(long, default_value_t = 0)]
        limit: u64,

        /// Keep games regardless of how they ended (default: only board endings).
        #[arg(long, default_value_t = false)]
        any_ending: bool,

        /// Write the combined book here instead of [storage].book.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Start a new book instead of adding to the existing combined book.
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
        Command::Get { tags } => fetch::cmd_get(&cfg, &tags),
        Command::Build {
            targets,
            max_ply,
            limit,
            any_ending,
            output,
            fresh,
        } => build::run(
            &cfg,
            &targets,
            &build::BuildOpts {
                max_ply,
                limit,
                any_ending,
                output,
                fresh,
            },
        ),
    }
}
