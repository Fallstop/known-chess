//! `kc-server` — serves known-chess move lookups out of an mmap'd book.
//!
//! The frontend owns the game state (it has a full chess engine in the
//! browser). For each position it asks this server "which moves have been played
//! from here, and how often?" via [`POST /api/lookup`]. The server hashes the
//! position, binary-searches the book, and returns the legal continuations.
//!
//! The book to load comes from `config.toml` (see [`shared::config`]): the
//! single combined book at `[storage].book` (or `[server].book`). The bind
//! address comes from `[server].bind`, overridable with `KC_BIND`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, routing::post, Json, Router};
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use shakmaty::fen::Fen;
use shakmaty::san::SanPlus;
use shakmaty::{CastlingMode, Chess};
use shared::{canonical_legal, position_hash, Book, Config};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

type SharedBook = Arc<Book<Mmap>>;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let cfg = Config::load(std::env::var_os("KC_CONFIG").map(PathBuf::from).as_deref())?;
    let book_path = cfg.server_book_path();
    let book =
        open_book(&book_path).with_context(|| format!("opening book {}", book_path.display()))?;
    tracing::info!(positions = book.position_count(), path = %book_path.display(), "book loaded");
    let book: SharedBook = Arc::new(book);

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/meta", get(meta))
        .route("/api/lookup", post(lookup))
        .with_state(book)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // KC_BIND overrides the configured address, e.g. for local dev.
    let bind = std::env::var("KC_BIND").unwrap_or(cfg.server.bind);
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

#[derive(Serialize)]
struct MetaResponse {
    /// Number of distinct positions in the book.
    positions: u64,
}

/// Static facts about the loaded book, for the frontend's header.
async fn meta(State(book): State<SharedBook>) -> Json<MetaResponse> {
    Json(MetaResponse {
        positions: book.position_count(),
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
    State(book): State<SharedBook>,
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
    let stats = book.lookup(hash);

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
