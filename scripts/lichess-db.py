#!/usr/bin/python3
"""lichess-db — browse and download lichess standard game dumps.

The lichess database publishes one zstd-compressed PGN per month at
<https://database.lichess.org/standard/list.txt>. This script reads that list,
shows which months you've already downloaded and which have been turned into a
known-chess `.book` by `kc-process`, lets you search, and downloads the ones
you want into the shared location configured in config.toml.

Usage:
    lichess-db.py list [QUERY]      # show every month (optionally filtered)
    lichess-db.py search QUERY      # alias for `list QUERY`
    lichess-db.py get TAG [TAG...]  # download months (e.g. 2026-05, 2025, latest)
    lichess-db.py process TARGET... # run kc-process on a tag or a .pgn[.zst] file

A "month" is referred to by its tag, e.g. 2026-05. QUERY / TAG match anywhere in
the tag, so `2025` selects all of 2025 and `latest` grabs the newest month.

Run from anywhere; config.toml is found next to the repo root by default, or
point at one with --config.
"""

import argparse
import re
import shutil
import subprocess
import sys
import tomllib
import urllib.request
from pathlib import Path

LIST_FALLBACK = "https://database.lichess.org/standard/list.txt"
TAG_RE = re.compile(r"(\d{4}-\d{2})")

# ANSI colours, disabled when stdout isn't a tty.
if sys.stdout.isatty():
    DIM, GREEN, YELLOW, BOLD, RESET = "\033[2m", "\033[32m", "\033[33m", "\033[1m", "\033[0m"
else:
    DIM = GREEN = YELLOW = BOLD = RESET = ""


# How to invoke kc-process when [process].command isn't set in config.toml. We
# prefer a built/installed `kc-process` on PATH, else fall back to cargo.
DEFAULT_PROCESS_CMD = ["cargo", "run", "--release", "-p", "processor", "--"]


def load_config(explicit):
    """Find and parse config.toml, returning a Config namespace."""
    candidates = []
    if explicit:
        candidates.append(Path(explicit))
    else:
        here = Path(__file__).resolve().parent
        candidates += [here.parent / "config.toml", here / "config.toml", Path.cwd() / "config.toml"]

    cfg_path = next((c for c in candidates if c.is_file()), None)
    if cfg_path is None:
        sys.exit("error: no config.toml found (looked in {})".format(
            ", ".join(str(c) for c in candidates)))

    with cfg_path.open("rb") as fh:
        cfg = tomllib.load(fh)

    storage = cfg.get("storage", {})
    downloads = storage.get("downloads")
    if not downloads:
        sys.exit("error: [storage].downloads is required in {}".format(cfg_path))
    downloads = Path(downloads).expanduser()
    processed = Path(storage.get("processed") or downloads / "books").expanduser()
    list_url = cfg.get("source", {}).get("list_url", LIST_FALLBACK)

    # How to run kc-process. A string is split on whitespace; a list is used
    # as-is. When unset we use kc-process from PATH if present, else cargo.
    raw_cmd = cfg.get("process", {}).get("command")
    if isinstance(raw_cmd, str):
        process_cmd = raw_cmd.split()
    elif isinstance(raw_cmd, list):
        process_cmd = [str(x) for x in raw_cmd]
    elif shutil.which("kc-process"):
        process_cmd = ["kc-process"]
    else:
        process_cmd = DEFAULT_PROCESS_CMD

    return Config(downloads, processed, list_url, process_cmd, cfg_path)


class Config:
    def __init__(self, downloads, processed, list_url, process_cmd, path):
        self.downloads = downloads
        self.processed = processed
        self.list_url = list_url
        self.process_cmd = process_cmd
        self.path = path


def fetch_list(list_url):
    """Return [(tag, url, filename)] newest-first, as published by lichess."""
    try:
        with urllib.request.urlopen(list_url, timeout=30) as resp:
            text = resp.read().decode("utf-8", "replace")
    except Exception as exc:  # noqa: BLE001 — surface any network/parse failure plainly
        sys.exit("error: could not fetch {}: {}".format(list_url, exc))

    entries = []
    for line in text.splitlines():
        url = line.strip()
        if not url:
            continue
        filename = url.rsplit("/", 1)[-1]
        m = TAG_RE.search(filename)
        if m:
            entries.append((m.group(1), url, filename))
    return entries


def human_size(num):
    for unit in ("B", "K", "M", "G", "T"):
        if num < 1024 or unit == "T":
            return ("{:.0f}{}" if unit == "B" else "{:.1f}{}").format(num, unit)
        num /= 1024


def status_for(tag, filename, downloads, processed):
    """Return (downloaded_path_or_None, processed_bool, size_bytes)."""
    dl = downloads / filename
    book = processed / (filename[:-len(".pgn.zst")] + ".book") if filename.endswith(".pgn.zst") \
        else processed / (tag + ".book")
    size = dl.stat().st_size if dl.is_file() else 0
    return (dl if dl.is_file() else None), book.is_file(), size


def cmd_list(entries, query, downloads, processed):
    if query:
        entries = [e for e in entries if query.lower() in e[0].lower()]
        if not entries:
            print("no months match {!r}".format(query))
            return
    print("{}{:<9}  {:<11}  {:<10}  {}{}".format(BOLD, "MONTH", "DOWNLOADED", "PROCESSED", "SIZE", RESET))
    n_dl = n_proc = 0
    for tag, _url, filename in entries:
        dl_path, is_proc, size = status_for(tag, filename, downloads, processed)
        if dl_path:
            n_dl += 1
            dl_cell = GREEN + "yes" + RESET
            size_cell = human_size(size)
        else:
            dl_cell = DIM + "—" + RESET
            size_cell = DIM + "—" + RESET
        if is_proc:
            n_proc += 1
            proc_cell = GREEN + "yes" + RESET
        else:
            proc_cell = (YELLOW + "pending" + RESET) if dl_path else DIM + "—" + RESET
        # pad on the visible text, accounting for invisible ANSI codes
        print("{:<9}  {}  {}  {:>6}".format(
            tag, pad(dl_cell, 11), pad(proc_cell, 10), size_cell))
    print("{}{} months · {} downloaded · {} processed{}".format(
        DIM, len(entries), n_dl, n_proc, RESET))


def pad(cell, width):
    """Left-justify a possibly-coloured cell to `width` visible chars."""
    visible = re.sub(r"\033\[[0-9;]*m", "", cell)
    return cell + " " * max(0, width - len(visible))


def resolve_tags(tags, entries):
    """Map user TAGs (substrings, or 'latest') to concrete entries, newest-first."""
    selected = {}
    for tag in tags:
        if tag.lower() == "latest":
            matches = entries[:1]
        else:
            matches = [e for e in entries if tag.lower() in e[0].lower()]
        if not matches:
            print("warning: no month matches {!r}".format(tag), file=sys.stderr)
        for e in matches:
            selected[e[0]] = e
    return [selected[t] for t in sorted(selected, reverse=True)]


def cmd_get(tags, entries, downloads, processed):
    targets = resolve_tags(tags, entries)
    if not targets:
        sys.exit("nothing to download")

    downloads.mkdir(parents=True, exist_ok=True)
    have_curl = shutil.which("curl") is not None

    todo = []
    for tag, url, filename in targets:
        dest = downloads / filename
        if dest.is_file():
            print("{}· {} already downloaded ({}){}".format(
                DIM, tag, human_size(dest.stat().st_size), RESET))
        else:
            todo.append((tag, url, dest))

    if not todo:
        print("everything requested is already present.")
        return

    print("downloading {} month(s) to {}".format(len(todo), downloads))
    for tag, url, dest in todo:
        print("{}↓ {}{}  {}".format(BOLD, tag, RESET, url))
        ok = download(url, dest, have_curl)
        if not ok:
            sys.exit("error: download failed for {}".format(tag))
        print("  {}saved {} ({}){}".format(
            GREEN, dest.name, human_size(dest.stat().st_size), RESET))
    print("done. process with: {} process {}".format(
        Path(__file__).name, " ".join(t for t, _, _ in todo)))


def book_path_for(src, processed):
    """The .book output path for an input PGN, stripping .pgn / .pgn.zst."""
    name = src.name
    for suffix in (".pgn.zst", ".pgn"):
        if name.endswith(suffix):
            name = name[:-len(suffix)]
            break
    return processed / (name + ".book")


def resolve_inputs(targets, entries, downloads):
    """Turn each TARGET (a file path or a month tag) into an input PGN path."""
    inputs = []
    for target in targets:
        p = Path(target).expanduser()
        if p.is_file():
            inputs.append(p)
            continue
        # Not a file — treat it as a month tag and look it up in the downloads dir.
        matches = [e for e in entries if target.lower() in e[0].lower()]
        if target.lower() == "latest" and entries:
            matches = entries[:1]
        if not matches:
            print("warning: {!r} is neither a file nor a known month".format(target),
                  file=sys.stderr)
            continue
        for _tag, _url, filename in matches:
            dest = downloads / filename
            if dest.is_file():
                inputs.append(dest)
            else:
                print("warning: {} not downloaded yet (run: {} get {})".format(
                    _tag, Path(__file__).name, _tag), file=sys.stderr)
    # de-dup while preserving order
    seen, unique = set(), []
    for p in inputs:
        if p not in seen:
            seen.add(p)
            unique.append(p)
    return unique


def cmd_process(args, entries, cfg):
    inputs = resolve_inputs(args.targets, entries, cfg.downloads)
    if not inputs:
        sys.exit("nothing to process")

    cfg.processed.mkdir(parents=True, exist_ok=True)
    extra = []
    if args.max_ply is not None:
        extra += ["--max-ply", str(args.max_ply)]
    if args.limit is not None:
        extra += ["--limit", str(args.limit)]
    if args.any_ending:
        extra += ["--any-ending"]

    failures = 0
    for src in inputs:
        out = book_path_for(src, cfg.processed)
        cmd = cfg.process_cmd + ["--input", str(src), "--output", str(out)] + extra
        print("{}▶ {}{} → {}".format(BOLD, src.name, RESET, out))
        print("  {}{}{}".format(DIM, " ".join(cmd), RESET))
        rc = subprocess.call(cmd)
        if rc != 0:
            failures += 1
            print("  {}error: kc-process exited {}{}".format(YELLOW, rc, RESET), file=sys.stderr)
        else:
            print("  {}wrote {}{}".format(GREEN, out.name, RESET))
    if failures:
        sys.exit("{} of {} input(s) failed".format(failures, len(inputs)))


def download(url, dest, have_curl):
    """Download `url` to `dest`, resuming a partial file when possible."""
    part = dest.with_suffix(dest.suffix + ".part")
    if have_curl:
        # -C - resumes a partial .part file; --fail surfaces HTTP errors as non-zero.
        rc = subprocess.call(["curl", "-fL", "-C", "-", "-o", str(part), url])
        if rc != 0:
            return False
    else:
        try:
            with urllib.request.urlopen(url, timeout=60) as resp, part.open("wb") as out:
                shutil.copyfileobj(resp, out, length=1 << 20)
        except Exception as exc:  # noqa: BLE001
            print("  error: {}".format(exc), file=sys.stderr)
            return False
    part.replace(dest)
    return True


def main():
    parser = argparse.ArgumentParser(
        description="Browse and download lichess standard game dumps.")
    parser.add_argument("--config", help="path to config.toml")
    sub = parser.add_subparsers(dest="command")

    p_list = sub.add_parser("list", help="show months (optionally filtered by QUERY)")
    p_list.add_argument("query", nargs="?", help="substring filter, e.g. 2025 or 2026-05")

    p_search = sub.add_parser("search", help="alias for `list QUERY`")
    p_search.add_argument("query", help="substring filter, e.g. 2025 or 2026-05")

    p_get = sub.add_parser("get", help="download one or more months")
    p_get.add_argument("tags", nargs="+", help="tags like 2026-05, 2025, or latest")

    p_proc = sub.add_parser("process", help="run kc-process on a tag or a .pgn[.zst] file")
    p_proc.add_argument("targets", nargs="+",
                        help="month tags (2026-05) and/or paths to .pgn[.zst] files")
    p_proc.add_argument("--max-ply", type=int, help="forwarded to kc-process")
    p_proc.add_argument("--limit", type=int, help="forwarded to kc-process")
    p_proc.add_argument("--any-ending", action="store_true", help="forwarded to kc-process")

    args = parser.parse_args()
    cfg = load_config(args.config)
    entries = fetch_list(cfg.list_url)

    if args.command in (None, "list"):
        cmd_list(entries, getattr(args, "query", None), cfg.downloads, cfg.processed)
    elif args.command == "search":
        cmd_list(entries, args.query, cfg.downloads, cfg.processed)
    elif args.command == "get":
        cmd_get(args.tags, entries, cfg.downloads, cfg.processed)
    elif args.command == "process":
        cmd_process(args, entries, cfg)


if __name__ == "__main__":
    main()
