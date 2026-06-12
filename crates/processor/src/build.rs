//! The `build` subcommand: fold lichess dumps into the combined book.
//!
//! Accepts month tags (e.g. `2026-05`, resolved to a downloaded dump) or paths
//! to `.pgn`/`.pgn.zst` files, and merges every one into the single combined
//! book at `[storage].book` (or `--output`). Parsing is parallel: the main
//! thread decompresses and splits each dump into batches of whole games, a
//! pool of worker threads (`--jobs`) replays them into per-thread tallies, and
//! the sorted runs are k-way stream-merged with the existing (mmap'd) book
//! into a temp file that replaces it. To bound RAM, a worker whose tally grows
//! past its share of `--max-mem-gb` spills it to an on-disk book that the final
//! merge folds in just like the existing book (so processing many dumps at once
//! never accumulates the whole corpus in memory). Dumps already baked in are
//! never reprocessed and the old book is never loaded into RAM. A progress bar
//! tracks each (compressed) input. Every dump merged is recorded in the book's
//! `.sources` manifest so `list` can show it processed.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::Mmap;
use pgn_reader::{BufferedReader, RawHeader, SanPlus, Skip, Visitor};
use shakmaty::{Chess, Position};
use shared::{
    canonical_index, position_hash, write_merged, write_merged_many_progress, Book, BookBuilder,
    Config,
};

use crate::fetch;

/// Raw-PGN batch size handed to each parser thread. Big enough that channel
/// and lock traffic is noise, small enough to keep all workers fed.
const BATCH_BYTES: usize = 2 << 20;

/// Knobs forwarded from the CLI to the per-file processor.
pub struct BuildOpts {
    pub max_ply: usize,
    pub limit: u64,
    pub any_ending: bool,
    /// Parser threads (0 = one per CPU core).
    pub jobs: usize,
    /// Write the combined book here instead of `[storage].book`.
    pub output: Option<PathBuf>,
    /// Ignore (and overwrite) any existing combined book instead of adding to it.
    pub fresh: bool,
    /// Spill a worker's tally to disk once it crosses its share of this many
    /// bytes of estimated heap (0 = never spill, keep everything in RAM).
    pub max_mem_bytes: u64,
}

/// Conservative heap estimate per position stored in a [`BookBuilder`]'s
/// hashmap (8-byte key + 16-byte slot + table/control overhead). Used only to
/// decide when to spill, so over-estimating just spills a little early.
const EST_BYTES_PER_POSITION: u64 = 48;

pub fn run(cfg: &Config, targets: &[String], opts: &BuildOpts) -> Result<()> {
    let output = opts.output.clone().unwrap_or_else(|| cfg.book_path());
    let manifest_path = manifest_for(cfg, &output);

    // No targets → catch-up mode: every downloaded dump not yet in this book.
    let inputs = if targets.is_empty() {
        let pending = catch_up_inputs(cfg, &manifest_path, opts.fresh)?;
        if pending.is_empty() {
            tracing::info!("everything downloaded is already in the book, nothing to do");
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

    // Keep the existing book for a streaming merge (incremental add) unless
    // --fresh. It stays mmap'd, never loaded back into memory.
    let mut sources: Vec<String> = Vec::new();
    let existing: Option<Book<Mmap>> = if !opts.fresh && output.is_file() {
        tracing::info!(book = %output.display(), "will stream-merge with existing book");
        sources = read_sources(&manifest_path);
        Some(
            open_book(&output)
                .with_context(|| format!("opening existing book {}", output.display()))?,
        )
    } else {
        None
    };

    let jobs = match opts.jobs {
        0 => std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
        n => n,
    };
    let (spill_paths, tallies) = process_inputs(&inputs, opts, jobs, &output)?;
    for ((path, dump_name), (games, skipped)) in inputs.iter().zip(&tallies) {
        tracing::info!(input = %path.display(), games, skipped, "processed dump");
        if !sources.contains(dump_name) {
            sources.push(dump_name.clone());
        }
    }

    // Write to a temp file and rename: the existing book is being read out of
    // an mmap of the destination path while we write.
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let tmp = {
        let mut name = output.as_os_str().to_os_string();
        name.push(".tmp");
        PathBuf::from(name)
    };
    let mut file = io::BufWriter::new(
        File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?,
    );

    // Spilled tallies are byte-identical to a book, so the merge folds them in
    // alongside the existing book as extra mmap'd sources.
    let spill_books: Vec<Book<Mmap>> = spill_paths
        .iter()
        .map(|p| open_book(p).with_context(|| format!("opening spill {}", p.display())))
        .collect::<Result<_>>()?;
    let mut olds: Vec<&Book<Mmap>> = Vec::with_capacity(spill_books.len() + 1);
    olds.extend(existing.as_ref());
    olds.extend(&spill_books);

    // Every run was spilled to disk during processing, so the merge reads them
    // all as mmap'd `olds`; nothing is held in RAM as an in-memory run.
    let estimate = olds.iter().map(|b| b.position_count()).sum::<u64>();
    let pb = position_bar(estimate);
    let stats = write_merged_many_progress(&olds, &[], &mut file, |written| {
        pb.set_position(written);
    })
    .with_context(|| format!("writing {}", tmp.display()))?;
    pb.finish_and_clear();
    drop(file);
    drop(olds);
    drop(existing); // unmap before replacing the file underneath
    drop(spill_books); // unmap before deleting the spill files
    std::fs::rename(&tmp, &output)
        .with_context(|| format!("renaming {} into place", tmp.display()))?;
    for path in &spill_paths {
        std::fs::remove_file(path).ok();
    }

    write_sources(&manifest_path, &sources)?;
    tracing::info!(
        positions = stats.positions,
        bytes = stats.bytes,
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
            tracing::warn!(target, "neither a file nor a known month, skipping");
            continue;
        }
        for entry in matches {
            let dump = cfg.download_path(&entry.filename);
            if dump.is_file() {
                push_unique(&mut out, dump, entry.filename.clone());
            } else {
                tracing::warn!(
                    tag = %entry.tag,
                    "not downloaded yet, run `kc-process get {}`",
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

/// One sorted run per worker, the on-disk spill-book paths, and per-input
/// (kept, skipped) game tallies — everything [`process_inputs`] hands back.
type Processed = (Vec<PathBuf>, Vec<(u64, u64)>);

/// Parse every input with a pool of parser threads. The main thread
/// decompresses each dump and splits it into batches of whole games; workers
/// replay games into per-thread builders. A worker whose builder grows past its
/// share of `max_mem_bytes` spills it to an on-disk book (named off
/// `spill_base`) and starts a fresh one, bounding peak RAM regardless of how
/// many dumps are processed at once. When the channel drains each worker spills
/// its tail too, so no run is ever held in RAM — the final merge reads every run
/// as an mmap'd book. Returns the spill-book paths and per-input (kept, skipped)
/// tallies in input order.
fn process_inputs(
    inputs: &[(PathBuf, String)],
    opts: &BuildOpts,
    jobs: usize,
    spill_base: &Path,
) -> Result<Processed> {
    let tallies: Vec<(AtomicU64, AtomicU64)> = inputs
        .iter()
        .map(|_| (AtomicU64::new(0), AtomicU64::new(0)))
        .collect();
    // Each worker keeps its own builder, so split the budget across them. 0
    // disables spilling (u64::MAX is never reached).
    let per_worker_budget = if opts.max_mem_bytes == 0 {
        u64::MAX
    } else {
        (opts.max_mem_bytes / jobs as u64).max(1)
    };
    let spill_counter = AtomicU64::new(0);
    let spill_paths: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    // Bounded so decompression can't run unboundedly ahead of the parsers.
    let (tx, rx) = mpsc::sync_channel::<(usize, Vec<u8>)>(jobs * 2);
    let rx = Mutex::new(rx);

    std::thread::scope(|scope| -> Result<()> {
        let workers: Vec<_> = (0..jobs)
            .map(|_| {
                scope.spawn(|| -> Result<()> {
                    let mut builder = BookBuilder::new();
                    loop {
                        // Hold the lock only to receive; parsing runs unlocked.
                        let received = rx.lock().unwrap().recv();
                        let Ok((file_idx, batch)) = received else { break };
                        let mut visitor =
                            BookVisitor::new(&mut builder, opts.max_ply, opts.any_ending);
                        let mut reader = BufferedReader::new(batch.as_slice());
                        // Reading from an in-memory batch cannot fail.
                        while reader.read_game(&mut visitor).expect("in-memory pgn").is_some() {}
                        let (kept, skipped) = &tallies[file_idx];
                        kept.fetch_add(visitor.games, Ordering::Relaxed);
                        skipped.fetch_add(visitor.skipped_games, Ordering::Relaxed);

                        if builder.position_count() as u64 * EST_BYTES_PER_POSITION
                            >= per_worker_budget
                        {
                            spill_builder(&mut builder, spill_base, &spill_counter, &spill_paths)?;
                        }
                    }
                    // Flush the tail to disk too, so the final merge reads every
                    // run as an mmap'd book and holds none in RAM. Returning these
                    // as in-memory runs made all workers materialize a sorted Vec
                    // beside their still-live HashMap at once — a multi-GB spike
                    // that the per-worker budget never accounted for.
                    spill_builder(&mut builder, spill_base, &spill_counter, &spill_paths)?;
                    Ok(())
                })
            })
            .collect();

        for (file_idx, (path, _)) in inputs.iter().enumerate() {
            dispatch_file(path, file_idx, &tx, opts.limit, &tallies[file_idx].0)
                .with_context(|| format!("processing {}", path.display()))?;
        }
        drop(tx); // close the channel so workers drain and spill their tails

        for w in workers {
            w.join().expect("parser thread panicked")?;
        }
        Ok(())
    })?;

    let tallies = tallies
        .iter()
        .map(|(kept, skipped)| (kept.load(Ordering::Relaxed), skipped.load(Ordering::Relaxed)))
        .collect();
    Ok((spill_paths.into_inner().unwrap(), tallies))
}

/// Drain a worker's builder to an on-disk book (so its RAM is freed) and record
/// the path for the final merge. The builder is left empty, ready to refill.
fn spill_builder(
    builder: &mut BookBuilder,
    base: &Path,
    counter: &AtomicU64,
    paths: &Mutex<Vec<PathBuf>>,
) -> Result<()> {
    let sorted = std::mem::take(builder).into_sorted();
    if sorted.is_empty() {
        return Ok(());
    }
    let n = counter.fetch_add(1, Ordering::Relaxed);
    let path = {
        let mut name = base.as_os_str().to_os_string();
        name.push(format!(".spill{n}"));
        PathBuf::from(name)
    };
    let mut file = io::BufWriter::new(
        File::create(&path).with_context(|| format!("creating spill {}", path.display()))?,
    );
    write_merged(None::<&Book<Mmap>>, &sorted, &mut file)
        .with_context(|| format!("writing spill {}", path.display()))?;
    tracing::info!(path = %path.display(), positions = sorted.len(), "spilled tally to disk");
    paths.lock().unwrap().push(path);
    Ok(())
}

/// Split one dump into batches of whole games and queue them for the parser
/// threads. Batches are only ever cut immediately before a `[Event ` header
/// line, so a game never spans two batches (lichess movetext never begins a
/// line with `[Event `). Respects `limit` by not dispatching further games.
fn dispatch_file(
    path: &Path,
    file_idx: usize,
    tx: &mpsc::SyncSender<(usize, Vec<u8>)>,
    limit: u64,
    kept: &AtomicU64,
) -> Result<()> {
    let (reader, progress) = open_input(path)?;
    let mut reader = BufReader::with_capacity(1 << 20, reader);
    let mut batch: Vec<u8> = Vec::with_capacity(BATCH_BYTES + (4 << 10));
    let mut line: Vec<u8> = Vec::with_capacity(1 << 10);
    let mut games: u64 = 0;

    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if line.starts_with(b"[Event ") {
            if limit != 0 && games >= limit {
                break;
            }
            games += 1;
            if batch.len() >= BATCH_BYTES {
                if tx.send((file_idx, std::mem::take(&mut batch))).is_err() {
                    bail!("parser threads exited early");
                }
                progress.set_message(format!("{} games kept", kept.load(Ordering::Relaxed)));
            }
        }
        batch.extend_from_slice(&line);
    }
    if !batch.is_empty() && tx.send((file_idx, batch)).is_err() {
        bail!("parser threads exited early");
    }
    progress.finish_and_clear();
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

/// Bar for the final streaming merge, counting positions written into the new
/// book. `total` is the upper-bound estimate from [`write_merged_many_progress`]
/// (shared positions collapse), so it may finish a touch shy of full.
fn position_bar(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "writing book [{bar:30}] {human_pos}/{human_len} positions (eta {eta})",
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
    /// (position hash, canonical legal-move index) for each ply so far.
    pending: Vec<(u64, u8)>,
    games: u64,
    skipped_games: u64,
}

impl<'a> BookVisitor<'a> {
    fn new(builder: &'a mut BookBuilder, max_ply: usize, any_ending: bool) -> Self {
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
        // Skip non-standard variants and games that start from a custom FEN.
        // Our book assumes the standard initial position.
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
            // `canonical_index` is Some for every legal move; the guard is
            // pure defensiveness.
            Ok(mv) => match canonical_index(&self.pos, &mv) {
                Some(index) => {
                    let hash = position_hash(&self.pos);
                    self.pending.push((hash, index));
                    self.pos.play_unchecked(&mv);
                    self.ply += 1;
                }
                None => {
                    self.skip_current = true;
                }
            },
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
            for (hash, index) in self.pending.drain(..) {
                self.builder.record(hash, index);
            }
            self.games += 1;
        } else {
            self.skipped_games += 1;
        }
        self.pending.clear();
    }
}
