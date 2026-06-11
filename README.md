# known-chess

Chess, but **you can only play moves that have actually been played in real
lichess games.**

Every position you reach is looked up in a "book" built from a lichess game
dump. You may only choose among the moves that real games played from that
position. As the game goes on, the set of games still matching your line shrinks.
Once a single game remains, there is only ever one legal move — so the line
**plays itself out to the end**, and you either win, lose, or draw.

Only games that ended in **checkmate or stalemate** are included, so every line
in the book reaches a real board result rather than fizzling out in a
resignation or a flag-fall.

## How it works

A chess position is identified by its 64-bit **Zobrist hash**. The book is a
flat, sorted table of `hash → [(move, game-count)]`, so a lookup is a single
binary search with no parsing or allocation — the server `mmap`s the file and
answers straight out of the page cache. Transpositions (the same position
reached by different move orders) collapse onto the same hash automatically.

```
 lichess .pgn.zst ──► kc-process ──► book.book ──► kc-server ──► frontend
   (raw games)        (Rust tool)    (shared fmt)  (Rust/axum)   (SvelteKit)
```

* **`crates/shared`** — the contract both Rust binaries share: Zobrist hashing,
  the 16-bit move encoding, and the binary book format (reader + writer).
* **`crates/processor`** (`kc-process`) — downloads lichess dumps, lists what's
  available/downloaded/processed, and folds each dump's `position → move` counts
  (for games ending in mate/stalemate) into one combined book.
* **`crates/server`** (`kc-server`) — `mmap`s the book and serves move lookups
  over HTTP. Dockerised.
* **`frontend`** — a SvelteKit app. It runs a full chess engine in the browser
  (`chess.js`), and for each position asks the server which moves are allowed.
  Forced (single-move) lines and the opponent's replies auto-play.

### The book format

All little-endian. See `crates/shared/src/book.rs` for the authoritative spec.

```
magic    [u8; 8]  = b"KNCHESS1"
version  u32
count    u32                       number of positions

entry table   (count × 16 bytes, sorted ascending by hash)
  hash       u64
  moves_off  u32                   offset into the moves blob
  moves_len  u16
  _pad       u16

moves blob    (6 bytes per record)
  mv         u16                   packed move: from | to<<6 | promo<<12
  count      u32                   games that played this move from here
```

A move is packed into 16 bits and resolved back to a concrete move by matching
it against the live position's legal moves — so castling and en-passant need no
special encoding.

## Quickstart

Storage locations live in **`config.toml`** at the repo root — where dumps are
downloaded, the combined book they're merged into, and the server's bind
address. Both binaries discover it automatically (an explicit `--config`, the
`KC_CONFIG` env var, or the nearest `config.toml` walking up from the cwd).

```toml
[storage]
downloads = "/path/to/Chess"            # where .pgn.zst dumps are saved
book      = "/path/to/Chess/known.book" # the single combined book

[source]
list_url = "https://database.lichess.org/standard/list.txt"

[server]
bind = "0.0.0.0:8080"
```

### 1. Get dumps and build the book

`kc-process` manages the whole dump → book pipeline against the lichess index:

```sh
KP="cargo run --release --bin kc-process --"

$KP list                 # every month, with downloaded / processed / size
$KP list 2024            # filter by substring (alias: search)
$KP get 2024-01 latest   # download dumps for those months (tags, or `latest`)
$KP build 2024-01        # fold a downloaded month into the combined book
$KP build                # catch-up: fold in every downloaded-but-unprocessed dump
$KP build dump.pgn.zst   # or point straight at a file
```

`build` is **incremental**: it folds the existing book back in first, so adding
a month doesn't reprocess the dumps already baked in (use `--fresh` to rebuild
from scratch). Dumps merged in are tracked in `<book>.sources`, which drives the
PROCESSED column in `list`.

Useful `build` flags: `--limit N` (stop after N qualifying games per dump),
`--max-ply N` (cap recorded depth), `--any-ending` (keep resignations/timeouts),
`--output PATH` (write somewhere other than `[storage].book`).

### 2. Run the server

The server reads the combined book named in `config.toml` — no path argument:

```sh
cargo run --release --bin kc-server
# → listening on 0.0.0.0:8080
```

```sh
curl -s localhost:8080/api/lookup -H 'content-type: application/json' \
  -d '{"fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"}'
# {"fen":"...","total":2,"moves":[{"uci":"e2e4","san":"e4","count":2}]}
```

| env | default | meaning |
|-----|---------|---------|
| `KC_CONFIG` | nearest `config.toml` | path to the config file |
| `KC_BIND` | `[server].bind` | listen address (overrides config) |
| `RUST_LOG` | `info` | log filter |

### 3. Run the frontend

```sh
cd frontend
npm install
npm run dev      # → http://localhost:5173, /api proxied to :8080
```

### With Docker

```sh
# Build a book into ./data first (step 1), then:
docker compose up --build
# server → :8080, frontend → :3000
```

## API

`POST /api/lookup` — body `{ "fen": "<position>" }`

```json
{
  "fen": "...",
  "total": 2,
  "moves": [{ "uci": "e2e4", "san": "e4", "count": 2 }]
}
```

`total` is the number of games that continued from this position. When it
reaches `1`, there is exactly one known move and the line is forced.

`GET /api/meta` — `{ "positions": <count> }`, the number of distinct positions
in the loaded book.

`GET /health` — returns `ok`.

## Layout

```
.
├── Cargo.toml                  # Rust workspace
├── config.toml                 # storage paths + server bind (both binaries)
├── crates/
│   ├── shared/                 # hashing + move encoding + book format + config
│   ├── processor/              # kc-process: get/list/build dumps → book
│   └── server/                 # kc-server: book → HTTP (+ Dockerfile)
├── frontend/                   # SvelteKit app (+ Dockerfile)
├── data/                       # books & dumps (gitignored)
└── docker-compose.yml
```

## Design notes

* **Transpositions merge.** Because positions are keyed by Zobrist hash, two
  different openings reaching the same position share an entry and pool their
  counts. This keeps the book compact at the cost of treating "one known game"
  as *position-based* rather than strict-move-order: a forced line is one where
  the position itself has a single recorded continuation.
* **Production routing.** In dev, Vite proxies `/api` to the server. In a real
  deployment, front both services with a reverse proxy (or rely on the server's
  permissive CORS) so the browser's `/api/lookup` reaches `kc-server`.
* **Scale.** Lichess monthly dumps are tens of GB compressed and hundreds of
  millions of games. Process a subset with `--limit` while iterating; the book
  itself stays small because it's deduplicated by position.
```
