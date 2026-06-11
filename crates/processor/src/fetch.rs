//! The `list` and `get` subcommands: browse and download lichess dumps.
//!
//! lichess publishes one zstd-compressed PGN per month, indexed at
//! `[source].list_url`. `list` shows every month and whether we've downloaded
//! and/or processed it; `get` downloads the dumps for the requested tags into
//! `[storage].downloads`, up to [`MAX_DOWNLOAD_JOBS`] in parallel.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use shared::Config;

/// Hard cap on concurrent downloads, so a broad tag (a whole year, say) can't
/// hammer lichess with one connection per month.
pub const MAX_DOWNLOAD_JOBS: usize = 5;

/// One entry from the dump list: a month tag, its download URL, and filename.
#[derive(Debug, Clone)]
pub struct Entry {
    pub tag: String,
    pub url: String,
    pub filename: String,
}

/// Fetch and parse the dump list, newest-first as lichess publishes it.
pub fn fetch_list(list_url: &str) -> Result<Vec<Entry>> {
    let body = ureq::get(list_url)
        .call()
        .with_context(|| format!("fetching {list_url}"))?
        .into_string()
        .context("reading dump list")?;

    let mut entries = Vec::new();
    for line in body.lines() {
        let url = line.trim();
        if url.is_empty() {
            continue;
        }
        let filename = url.rsplit('/').next().unwrap_or(url).to_string();
        if let Some(tag) = extract_tag(&filename) {
            entries.push(Entry {
                tag,
                url: url.to_string(),
                filename,
            });
        }
    }
    Ok(entries)
}

/// Pull a `YYYY-MM` tag out of a dump filename, if present.
fn extract_tag(filename: &str) -> Option<String> {
    let bytes = filename.as_bytes();
    // Scan for the first `dddd-dd` run.
    for i in 0..bytes.len().saturating_sub(6) {
        let w = &bytes[i..i + 7];
        if w[0..4].iter().all(u8::is_ascii_digit)
            && w[4] == b'-'
            && w[5].is_ascii_digit()
            && w[6].is_ascii_digit()
        {
            return Some(String::from_utf8_lossy(w).into_owned());
        }
    }
    None
}

/// Entries whose tag matches `query` (substring), or the newest for `latest`.
pub fn match_tag<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a Entry> {
    if query.eq_ignore_ascii_case("latest") {
        return entries.iter().take(1).collect();
    }
    let q = query.to_lowercase();
    entries
        .iter()
        .filter(|e| e.tag.to_lowercase().contains(&q))
        .collect()
}

pub fn cmd_list(cfg: &Config, query: Option<&str>) -> Result<()> {
    let entries = fetch_list(&cfg.source.list_url)?;
    let filtered: Vec<&Entry> = match query {
        Some(q) => entries.iter().filter(|e| e.tag.contains(q)).collect(),
        None => entries.iter().collect(),
    };
    if filtered.is_empty() {
        println!("no months match {:?}", query.unwrap_or(""));
        return Ok(());
    }

    // "Processed" = folded into the combined book, per its .sources manifest.
    let merged = cfg.read_manifest();

    println!("{:<9}  {:<11}  {:<10}  {}", "MONTH", "DOWNLOADED", "PROCESSED", "SIZE");
    let (mut n_dl, mut n_proc) = (0u32, 0u32);
    for e in &filtered {
        let dump = cfg.download_path(&e.filename);
        let downloaded = dump.is_file();
        let processed = merged.contains(&e.filename);
        let (dl_cell, size_cell) = if downloaded {
            n_dl += 1;
            let size = dump.metadata().map(|m| m.len()).unwrap_or(0);
            ("yes", human_size(size))
        } else {
            ("—", "—".to_string())
        };
        let proc_cell = if processed {
            n_proc += 1;
            "yes"
        } else if downloaded {
            "pending"
        } else {
            "—"
        };
        println!("{:<9}  {:<11}  {:<10}  {:>7}", e.tag, dl_cell, proc_cell, size_cell);
    }
    println!(
        "{} months · {} downloaded · {} processed",
        filtered.len(),
        n_dl,
        n_proc
    );
    Ok(())
}

pub fn cmd_get(cfg: &Config, tags: &[String], jobs: usize) -> Result<()> {
    let entries = fetch_list(&cfg.source.list_url)?;

    // Resolve tags to a de-duplicated, newest-first set of entries. A tag can
    // match many months (`2014` selects the whole year).
    let mut selected: Vec<Entry> = Vec::new();
    for tag in tags {
        let matches = match_tag(&entries, tag);
        if matches.is_empty() {
            tracing::warn!(tag, "no month matches — skipping");
        }
        for e in matches {
            if !selected.iter().any(|s| s.tag == e.tag) {
                selected.push(e.clone());
            }
        }
    }
    if selected.is_empty() {
        bail!("nothing to download");
    }

    std::fs::create_dir_all(&cfg.storage.downloads)
        .with_context(|| format!("creating {}", cfg.storage.downloads.display()))?;

    let pending: Vec<Entry> = selected
        .into_iter()
        .filter(|e| {
            let dest = cfg.download_path(&e.filename);
            if dest.is_file() {
                let size = dest.metadata().map(|m| m.len()).unwrap_or(0);
                tracing::info!(tag = %e.tag, size = %human_size(size), "already downloaded");
                return false;
            }
            true
        })
        .collect();
    if pending.is_empty() {
        println!("everything requested is already downloaded");
        return Ok(());
    }

    let jobs = jobs.clamp(1, MAX_DOWNLOAD_JOBS).min(pending.len());
    tracing::info!(count = pending.len(), jobs, "downloading");

    // A fixed pool of worker threads pulls dumps off a shared cursor; each
    // in-flight download owns one bar in the MultiProgress.
    let progress = MultiProgress::new();
    let next = AtomicUsize::new(0);
    let failed = Mutex::new(Vec::<String>::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(e) = pending.get(i) else { break };
                let dest = cfg.download_path(&e.filename);
                match download(&e.url, &dest, &progress) {
                    Ok(()) => {
                        let size = dest.metadata().map(|m| m.len()).unwrap_or(0);
                        let _ = progress.println(format!("{} saved ({})", e.tag, human_size(size)));
                    }
                    Err(err) => {
                        let _ = progress.println(format!("{} failed: {err:#}", e.tag));
                        failed.lock().unwrap().push(e.tag.clone());
                    }
                }
            });
        }
    });

    let failed = failed.into_inner().unwrap();
    if !failed.is_empty() {
        bail!(
            "{} of {} downloads failed ({}) — rerun `kc-process get` to retry",
            failed.len(),
            pending.len(),
            failed.join(", ")
        );
    }
    println!("done. build with: kc-process build");
    Ok(())
}

/// Stream `url` to `dest` via a `.part` file (renamed on success), showing a
/// byte progress bar driven by the response's Content-Length when available.
fn download(url: &str, dest: &Path, progress: &MultiProgress) -> Result<()> {
    let resp = ureq::get(url).call().context("HTTP request failed")?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let pb = progress.add(ProgressBar::new(total));
    pb.set_style(
        ProgressStyle::with_template(
            "{prefix} [{bar:30}] {bytes}/{total_bytes} ({bytes_per_sec}, eta {eta})",
        )
        .unwrap()
        .progress_chars("=> "),
    );
    pb.set_prefix(dest.file_name().and_then(|f| f.to_str()).unwrap_or("download").to_string());

    let part = dest.with_extension("part");
    let result = (|| {
        let mut reader = pb.wrap_read(resp.into_reader());
        let mut out = File::create(&part)
            .with_context(|| format!("creating {}", part.display()))?;
        io::copy(&mut reader, &mut out).context("writing download")?;
        out.flush().ok();
        std::fs::rename(&part, dest)
            .with_context(|| format!("moving {} into place", part.display()))
    })();
    pb.finish_and_clear();
    progress.remove(&pb);
    if result.is_err() {
        std::fs::remove_file(&part).ok();
    }
    result
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{}{}", bytes, UNITS[unit])
    } else {
        format!("{:.1}{}", size, UNITS[unit])
    }
}
