//! The `build` subcommand: fold lichess dumps into the combined book.
//!
//! Accepts month tags (e.g. `2026-05`, resolved to a downloaded dump) or paths
//! to `.pgn`/`.pgn.zst` files, and merges every one into the single combined
//! book at `[storage].book` (or `--output`). The existing book is folded in
//! first so adding a month is incremental — the dumps already baked in are not
//! reprocessed. A progress bar tracks each (compressed) input. Every dump merged
//! is recorded in the book's `.sources` manifest so `list` can show it processed.

use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::Mmap;
use pgn_reader::{BufferedReader, RawHeader, SanPlus, Skip, Visitor};
use shakmaty::{Chess, Position};
use shared::{position_hash, Book, BookBuilder, Config, EncodedMove};

use crate::fetch;

/// Knobs forwarded from the CLI to the per-file processor.
pub struct BuildOpts {
    pub max_ply: usize,
    pub limit: u64,
    pub any_ending: bool,
    /// Write the combined book here instead of `[storage].book`.
    pub output: Option<PathBuf>,
    /// Ignore (and overwrite) any existing combined book instead of adding to it.
    pub fresh: bool,
}

pub fn run(cfg: &Config, targets: &[String], opts: &BuildOpts) -> Result<()> {
    let output = opts.output.clone().unwrap_or_else(|| cfg.book_path());
    let manifest_path = manifest_for(cfg, &output);

    // No targets → catch-up mode: every downloaded dump not yet in this book.
    let inputs = if targets.is_empty() {
        let pending = catch_up_inputs(cfg, &manifest_path, opts.fresh)?;
        if pending.is_empty() {
            tracing::info!("everything downloaded is already in the book — nothing to do");
            return Ok(());
        }
        tracing::info!(count = pending.len(), "catch-up: processing downloaded dumps");
        pending
    } else {
        resolve_inputs(cfg, targets)?
    };
    if inputs.is_empty() {
        bail!("nothing to process");
    }

    // Start from the existing book (incremental add) unless --fresh.
    let mut builder = BookBuilder::new();
    let mut sources: Vec<String> = Vec::new();
    if !opts.fresh && output.is_file() {
        tracing::info!(book = %output.display(), "folding in existing book");
        let existing = open_book(&output)
            .with_context(|| format!("opening existing book {}", output.display()))?;
        builder.absorb(&existing);
        sources = read_sources(&manifest_path);
    }

    for (path, dump_name) in &inputs {
        process_file(path, &mut builder, opts)
            .with_context(|| format!("processing {}", path.display()))?;
        if !sources.contains(dump_name) {
            sources.push(dump_name.clone());
        }
    }

    let bytes = builder.into_bytes();
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    File::create(&output)
        .with_context(|| format!("creating {}", output.display()))?
        .write_all(&bytes)?;
    write_sources(&manifest_path, &sources)?;
    tracing::info!(
        bytes = bytes.len(),
        path = %output.display(),
        sources = sources.len(),
        "wrote combined book"
    );
    Ok(())
}

fn open_book(path: &Path) -> Result<Book<Mmap>> {
    let file = File::open(path)?;
    // SAFETY: read-only and not mutated while mapped.
    let mmap = unsafe { Mmap::map(&file)? };
    Ok(Book::open(mmap)?)
}

/// The `.sources` manifest path for a given output book.
fn manifest_for(cfg: &Config, output: &Path) -> PathBuf {
    if output == cfg.book_path() {
        cfg.manifest_path()
    } else {
        let mut name = output.file_name().map(|f| f.to_os_string()).unwrap_or_default();
        name.push(".sources");
        output.with_file_name(name)
    }
}

fn read_sources(path: &Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn write_sources(path: &Path, sources: &[String]) -> Result<()> {
    let mut body = sources.join("\n");
    body.push('\n');
    std::fs::write(path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Map each target (a file path or a month tag) to `(input path, dump filename)`.
fn resolve_inputs(cfg: &Config, targets: &[String]) -> Result<Vec<(PathBuf, String)>> {
    let mut entries: Option<Vec<fetch::Entry>> = None;
    let mut out: Vec<(PathBuf, String)> = Vec::new();

    for target in targets {
        let path = Path::new(target);
        if path.is_file() {
            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(target)
                .to_string();
            push_unique(&mut out, path.to_path_buf(), name);
            continue;
        }

        let list = match &entries {
            Some(e) => e,
            None => {
                entries = Some(fetch::fetch_list(&cfg.source.list_url)?);
                entries.as_ref().unwrap()
            }
        };
        let matches = fetch::match_tag(list, target);
        if matches.is_empty() {
            tracing::warn!(target, "neither a file nor a known month — skipping");
            continue;
        }
        for entry in matches {
            let dump = cfg.download_path(&entry.filename);
            if dump.is_file() {
                push_unique(&mut out, dump, entry.filename.clone());
            } else {
                tracing::warn!(
                    tag = %entry.tag,
                    "not downloaded yet — run `kc-process get {}`",
                    entry.tag
                );
            }
        }
    }
    Ok(out)
}

fn push_unique(out: &mut Vec<(PathBuf, String)>, path: PathBuf, name: String) {
    if !out.iter().any(|(p, _)| p == &path) {
        out.push((path, name));
    }
}

/// Every downloaded dump in the downloads dir not yet folded into this book,
/// sorted by name. With `--fresh` the manifest is ignored, so all dumps are
/// (re)processed. Skips `.part` files from interrupted downloads.
fn catch_up_inputs(cfg: &Config, manifest_path: &Path, fresh: bool) -> Result<Vec<(PathBuf, String)>> {
    let already: Vec<String> = if fresh { Vec::new() } else { read_sources(manifest_path) };
    let dir = &cfg.storage.downloads;
    let mut out = Vec::new();
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        // No downloads dir yet just means nothing to catch up on.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    for entry in read {
        let path = entry?.path();
        let name = match path.file_name().and_then(|f| f.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let is_dump = name.ends_with(".pgn.zst") || name.ends_with(".pgn");
        if is_dump && !already.contains(&name) {
            out.push((path, name));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(out)
}

fn process_file(input: &Path, builder: &mut BookBuilder, opts: &BuildOpts) -> Result<()> {
    let (reader, progress) = open_input(input)?;
    let mut buffered = BufferedReader::new(reader);

    let mut visitor = BookVisitor::new(builder, opts.max_ply, opts.any_ending, progress.clone());
    while buffered.read_game(&mut visitor)?.is_some() {
        if opts.limit != 0 && visitor.games >= opts.limit {
            break;
        }
    }
    let (games, skipped) = (visitor.games, visitor.skipped_games);
    progress.finish_and_clear();
    tracing::info!(input = %input.display(), games, skipped, "processed dump");
    Ok(())
}

/// Open an input, wrapping it in a progress bar tracking bytes read from the
/// file (the *compressed* bytes for `.zst`, which still map monotonically to
/// progress through the dump). `-` reads stdin with an indeterminate spinner.
/// Returns the decoded reader plus the bar to share with the visitor.
fn open_input(path: &Path) -> Result<(Box<dyn Read>, ProgressBar)> {
    if path.as_os_str() == "-" {
        let pb = spinner();
        pb.set_prefix("stdin");
        let reader = Box::new(pb.wrap_read(BufReader::new(io::stdin())));
        return Ok((reader, pb));
    }

    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);
    let label = path.file_name().and_then(|f| f.to_str()).unwrap_or("input");
    let pb = byte_bar(total);
    pb.set_prefix(label.to_string());
    let counted = pb.wrap_read(file);

    let reader: Box<dyn Read> = if path.extension().and_then(|e| e.to_str()) == Some("zst") {
        Box::new(zstd::Decoder::new(counted)?)
    } else {
        Box::new(BufReader::new(counted))
    };
    Ok((reader, pb))
}

fn byte_bar(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "{prefix} [{bar:30}] {bytes}/{total_bytes} {msg} (eta {eta})",
        )
        .unwrap()
        .progress_chars("=> "),
    );
    pb
}

fn spinner() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner} {prefix} {bytes} {msg}").unwrap());
    pb
}

/// Visitor that replays each game and records every (position, move) into the
/// borrowed [`BookBuilder`]. Sidelined commentary and non-standard games are
/// skipped.
struct BookVisitor<'a> {
    builder: &'a mut BookBuilder,
    pos: Chess,
    max_ply: usize,
    require_board_end: bool,
    ply: usize,
    skip_current: bool,
    pending: Vec<(u64, EncodedMove)>,
    games: u64,
    skipped_games: u64,
    progress: ProgressBar,
}

impl<'a> BookVisitor<'a> {
    fn new(
        builder: &'a mut BookBuilder,
        max_ply: usize,
        any_ending: bool,
        progress: ProgressBar,
    ) -> Self {
        Self {
            builder,
            pos: Chess::default(),
            max_ply,
            require_board_end: !any_ending,
            ply: 0,
            skip_current: false,
            pending: Vec::new(),
            games: 0,
            skipped_games: 0,
            progress,
        }
    }
}

impl Visitor for BookVisitor<'_> {
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
                self.skip_current = true;
            }
        }
    }

    // We only want moves as actually played, not analysis sidelines.
    fn begin_variation(&mut self) -> Skip {
        Skip(true)
    }

    fn end_game(&mut self) -> Self::Result {
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

        // Refresh the bar's suffix occasionally so it shows live counts without
        // formatting on every single game.
        if (self.games + self.skipped_games) % 20_000 == 0 {
            self.progress
                .set_message(format!("{} games kept", self.games));
        }
    }
}
