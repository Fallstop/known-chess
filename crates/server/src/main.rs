//! `kc-server` serves known-chess move lookups out of an mmap'd book.
//!
//! The frontend owns the game state (it has a full chess engine in the
//! browser). For each position it asks this server "which moves have been played
//! from here, and how often?" via [`POST /api/lookup`]. The server hashes the
//! position, binary-searches the book, and returns the legal continuations.
//!
//! The book to load comes from `KC_BOOK_PATH` if set (the deployment path,
//! see the root `Dockerfile`), otherwise from `config.toml` (see
//! [`shared::config`]): the single combined book at `[storage].book` (or
//! `[server].book`). When `KC_BOOK_PATH` is set no `config.toml` is required, so
//! the server runs from environment variables alone. The bind address comes from
//! `KC_BIND`, falling back to `[server].bind` (or `0.0.0.0:8080`).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
    routing::post,
    Json, Router,
};
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use shakmaty::fen::Fen;
use shakmaty::san::SanPlus;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position};
use shared::{canonical_legal, position_hash, Book, Config};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

type SharedBook = Arc<Book<Mmap>>;

#[derive(Clone)]
struct AppState {
    book: SharedBook,
    /// Bearer token for explorer.lichess.ovh (it requires authentication).
    /// `None` disables `/api/identify`: it then always answers `game: null`.
    lichess_token: Option<Arc<str>>,
    /// Public URL of the frontend; `GET /` redirects there when set.
    site_url: Option<Arc<str>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    // `KC_BOOK_PATH` (set by the deployment image) points straight at the book
    // and lets the server run with no `config.toml`. Only fall back to loading
    // config (and only require it to exist) when the env var is absent.
    let (book_path, cfg_bind, cfg_token, cfg_site) = match std::env::var_os("KC_BOOK_PATH") {
        Some(p) => (PathBuf::from(p), None, None, None),
        None => {
            let cfg = Config::load(std::env::var_os("KC_CONFIG").map(PathBuf::from).as_deref())?;
            (
                cfg.server_book_path(),
                Some(cfg.server.bind),
                cfg.server.lichess_token,
                cfg.server.site_url,
            )
        }
    };
    let book =
        open_book(&book_path).with_context(|| format!("opening book {}", book_path.display()))?;
    tracing::info!(positions = book.position_count(), path = %book_path.display(), "book loaded");

    let lichess_token: Option<Arc<str>> = std::env::var("KC_LICHESS_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or(cfg_token)
        .map(Arc::from);
    tracing::info!(enabled = lichess_token.is_some(), "lichess game identification");
    let site_url: Option<Arc<str>> = std::env::var("KC_SITE_URL")
        .ok()
        .filter(|u| !u.is_empty())
        .or(cfg_site)
        .map(Arc::from);
    let state = AppState { book: Arc::new(book), lichess_token, site_url };

    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(|| async { "ok" }))
        .route("/api/meta", get(meta))
        .route("/api/lookup", post(lookup))
        .route("/api/identify", post(identify))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // KC_BIND overrides the configured address, e.g. for local dev.
    let bind = std::env::var("KC_BIND")
        .ok()
        .or(cfg_bind)
        .unwrap_or_else(|| "0.0.0.0:8080".to_string());
    let addr: SocketAddr = bind.parse().with_context(|| format!("parsing bind address {bind:?}"))?;
    tracing::info!(%addr, "listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn open_book(path: &Path) -> Result<Book<Mmap>> {
    let file = std::fs::File::open(path)?;
    // SAFETY: the book file is read-only and not mutated while mapped.
    let mmap = unsafe { Mmap::map(&file)? };
    Ok(Book::open(mmap)?)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}

/// The API has no UI; send people who land on the bare origin to the site.
/// Temporary (307) so a stale URL isn't cached forever by browsers.
async fn root(State(state): State<AppState>) -> Response {
    match state.site_url.as_deref() {
        Some(url) => Redirect::temporary(url).into_response(),
        None => "known-chess API — see /api/lookup, /api/meta".into_response(),
    }
}

#[derive(Serialize)]
struct MetaResponse {
    /// Number of distinct positions in the book.
    positions: u64,
}

/// Static facts about the loaded book, for the frontend's header.
async fn meta(State(state): State<AppState>) -> Json<MetaResponse> {
    Json(MetaResponse {
        positions: state.book.position_count(),
    })
}

#[derive(Deserialize)]
struct LookupRequest {
    /// The position to look up, as a FEN string.
    fen: String,
}

#[derive(Serialize)]
struct KnownMove {
    uci: String,
    san: String,
    count: u64,
}

#[derive(Serialize)]
struct LookupResponse {
    /// Echoed back so the client can correlate async responses.
    fen: String,
    /// Total games that continued from this position (sum of move counts).
    total: u64,
    /// Known continuations, most popular first.
    moves: Vec<KnownMove>,
}

async fn lookup(
    State(state): State<AppState>,
    Json(req): Json<LookupRequest>,
) -> Result<Json<LookupResponse>, AppError> {
    let fen: Fen = req
        .fen
        .parse()
        .map_err(|_| AppError::bad_request("invalid FEN"))?;
    let pos: Chess = fen
        .into_position(CastlingMode::Standard)
        .map_err(|_| AppError::bad_request("illegal position"))?;

    let hash = position_hash(&pos);
    let stats = state.book.lookup(hash);

    // The book stores each move as an index into the canonical legal-move
    // ordering; resolve against the live position to emit both UCI (for the
    // client engine) and SAN (for display).
    let legal = canonical_legal(&pos);
    let mut moves = Vec::with_capacity(stats.len());
    let mut total: u64 = 0;
    for stat in stats {
        total += stat.count;
        if let Some(mv) = legal.get(stat.index as usize) {
            let uci = mv.to_uci(CastlingMode::Standard).to_string();
            let san = SanPlus::from_move(pos.clone(), mv).to_string();
            moves.push(KnownMove {
                uci,
                san,
                count: stat.count,
            });
        }
    }

    Ok(Json(LookupResponse {
        fen: req.fen,
        total,
        moves,
    }))
}

/// Deepest position depth (in plies) the lichess opening explorer indexes;
/// positions after more than this many plies always come back empty.
const EXPLORER_MAX_PLY: usize = 49;

#[derive(Deserialize)]
struct IdentifyRequest {
    /// Every move of the finished game, in order, as UCI.
    ucis: Vec<String>,
}

#[derive(Serialize, Clone)]
struct SourcePlayer {
    name: String,
    rating: Option<u32>,
}

#[derive(Serialize, Clone)]
struct SourceGame {
    /// Lichess game id. The game lives at `https://lichess.org/{id}`.
    id: String,
    white: SourcePlayer,
    black: SourcePlayer,
    /// "white", "black", or null for a draw.
    winner: Option<String>,
    speed: Option<String>,
    /// Month the game was played, e.g. "2012-12".
    month: Option<String>,
    /// False when several games shared the line as deep as the explorer
    /// indexes, so this is the best (result-matching) candidate, not a proof.
    exact: bool,
}

#[derive(Serialize)]
struct IdentifyResponse {
    game: Option<SourceGame>,
}

/// Find the real lichess game behind a fully played-out line.
///
/// The book stores no game ids, so this asks the lichess opening explorer
/// instead: replay the line to the deepest position the explorer still indexes
/// (≤ [`EXPLORER_MAX_PLY`] plies), then ask which games played our next move
/// from there. Once the line is down to a single game, which is exactly the
/// situation after a known-chess playout, the explorer attaches the game id to
/// the move.
async fn identify(
    State(state): State<AppState>,
    Json(req): Json<IdentifyRequest>,
) -> Result<Json<IdentifyResponse>, AppError> {
    let Some(token) = state.lichess_token.clone() else {
        return Ok(Json(IdentifyResponse { game: None }));
    };
    if req.ucis.is_empty() || req.ucis.len() > 1024 {
        return Err(AppError::bad_request("expected 1..=1024 moves"));
    }

    // Replay to the anchor position: just before the last move, or just before
    // the deepest move the explorer can still see, whichever comes first.
    let anchor = (req.ucis.len() - 1).min(EXPLORER_MAX_PLY);
    let mut pos = Chess::default();
    for u in &req.ucis[..anchor] {
        pos = play_uci(pos, u).ok_or_else(|| AppError::bad_request("illegal move in line"))?;
    }
    let target = req.ucis[anchor].clone();
    // The winner according to the played-out line, for tie-breaking when the
    // explorer has several games matching the anchor move.
    let winner = line_winner(pos.clone(), &req.ucis[anchor..])
        .ok_or_else(|| AppError::bad_request("illegal move in line"))?;
    let fen = Fen::from_position(pos, EnPassantMode::Legal).to_string();

    let resp = tokio::task::spawn_blocking(move || explorer_lookup(&token, &fen))
        .await
        .map_err(|_| AppError::bad_request("lookup task failed"))?;
    let body = match resp {
        Ok(body) => body,
        Err(e) => {
            tracing::warn!(error = %e, "explorer lookup failed");
            return Ok(Json(IdentifyResponse { game: None }));
        }
    };

    Ok(Json(IdentifyResponse {
        game: match_game(&body, &target, winner.as_deref()),
    }))
}

fn play_uci(pos: Chess, uci: &str) -> Option<Chess> {
    let mv = uci.parse::<UciMove>().ok()?.to_move(&pos).ok()?;
    pos.play(&mv).ok()
}

/// Play out the tail of the line and report the winner: `Some(Some("white"))`
/// / `Some(Some("black"))` for mate, `Some(None)` for a drawn ending, and
/// `None` if a move is illegal.
fn line_winner(mut pos: Chess, tail: &[String]) -> Option<Option<String>> {
    for u in tail {
        pos = play_uci(pos, u)?;
    }
    if pos.is_checkmate() {
        let winner = if pos.turn().is_white() { "black" } else { "white" };
        Some(Some(winner.to_string()))
    } else {
        Some(None)
    }
}

fn explorer_lookup(token: &str, fen: &str) -> Result<serde_json::Value, ureq::Error> {
    let body = ureq::get("https://explorer.lichess.ovh/lichess")
        .query("variant", "standard")
        .query("fen", fen)
        .query("speeds", "ultraBullet,bullet,blitz,rapid,classical,correspondence")
        .query("ratings", "400,1000,1200,1400,1600,1800,2000,2200,2500")
        .query("modes", "rated")
        .query("recentGames", "15")
        .set("Authorization", &format!("Bearer {token}"))
        .timeout(std::time::Duration::from_secs(8))
        .call()?
        .into_json()?;
    Ok(body)
}

/// Pick the source game out of an explorer response: the move entry matching
/// `target` carries the game outright when it's unique; otherwise fall back to
/// the recent/top game samples that played `target`, tie-breaking on result.
fn match_game(body: &serde_json::Value, target: &str, winner: Option<&str>) -> Option<SourceGame> {
    let move_entry = body["moves"]
        .as_array()?
        .iter()
        .find(|m| m["uci"].as_str() == Some(target))?;
    let move_count = ["white", "draws", "black"]
        .iter()
        .filter_map(|k| move_entry[k].as_u64())
        .sum::<u64>();
    if let Some(game) = parse_game(&move_entry["game"], move_count == 1) {
        return Some(game);
    }

    let mut candidates: Vec<SourceGame> = ["recentGames", "topGames"]
        .iter()
        .filter_map(|k| body[k].as_array())
        .flatten()
        .filter(|g| g["uci"].as_str() == Some(target))
        .filter_map(|g| parse_game(g, move_count == 1))
        .collect();
    // Our game ended in the line's result; a candidate that didn't can't be it.
    candidates.retain(|g| g.winner.as_deref() == winner);
    match candidates.len() {
        1 => candidates.pop(),
        _ => None,
    }
}

fn parse_game(g: &serde_json::Value, exact: bool) -> Option<SourceGame> {
    let player = |v: &serde_json::Value| -> Option<SourcePlayer> {
        Some(SourcePlayer {
            name: v["name"].as_str()?.to_string(),
            rating: v["rating"].as_u64().map(|r| r as u32),
        })
    };
    Some(SourceGame {
        id: g["id"].as_str()?.to_string(),
        white: player(&g["white"])?,
        black: player(&g["black"])?,
        winner: g["winner"].as_str().map(str::to_string),
        speed: g["speed"].as_str().map(str::to_string),
        month: g["month"].as_str().map(str::to_string),
        exact,
    })
}

struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn bad_request(msg: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(serde_json::json!({ "error": self.message }))).into_response()
    }
}
