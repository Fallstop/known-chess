import { PUBLIC_KC_API_URL } from '$env/static/public';

/**
 * Where the Rust server lives, baked in at build time. Empty means
 * same-origin: in dev, Vite proxies /api to it (see vite.config.ts). In the
 * Cloudflare Pages deployment, set PUBLIC_KC_API_URL in the dashboard's
 * BUILD environment to the backend's public origin, e.g.
 * "https://api.example.com" (no trailing slash); the server sends permissive
 * CORS headers, so cross-origin is fine.
 */
const API_BASE = PUBLIC_KC_API_URL;

/** A move known to the book, as returned by the server. */
export interface KnownMove {
	/** Long algebraic, e.g. "e2e4" or "e7e8q". Feed straight into chess.js. */
	uci: string;
	/** Standard algebraic, e.g. "Nf3", for display. */
	san: string;
	/** Number of games that played this move from the queried position. */
	count: number;
}

export interface LookupResponse {
	fen: string;
	/** Total games that continued from this position. */
	total: number;
	/** Known continuations, most popular first. */
	moves: KnownMove[];
}

export interface MetaResponse {
	/** Number of distinct positions in the book. */
	positions: number;
}

class HttpError extends Error {
	constructor(
		path: string,
		readonly status: number
	) {
		super(`${path} failed: ${status}`);
	}
}

const ATTEMPT_TIMEOUT_MS = 8000;
const RETRIES = 2;

/** Worth retrying: network blips, timeouts, server hiccups — not 4xx. */
const transient = (e: unknown) => !(e instanceof HttpError) || e.status >= 500 || e.status === 429;

const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/**
 * Fetch with a per-attempt timeout and a couple of retries on transient
 * failures. The caller's signal aborts the in-flight request and stops
 * retrying (e.g. when the game it belonged to was reset).
 */
async function fetchJSON<T>(path: string, init: RequestInit, signal?: AbortSignal): Promise<T> {
	for (let attempt = 0; ; attempt++) {
		try {
			const timeout = AbortSignal.timeout(ATTEMPT_TIMEOUT_MS);
			const res = await fetch(API_BASE + path, {
				...init,
				signal: signal ? AbortSignal.any([signal, timeout]) : timeout
			});
			if (!res.ok) throw new HttpError(path, res.status);
			return (await res.json()) as T;
		} catch (e) {
			if (signal?.aborted || attempt >= RETRIES || !transient(e)) throw e;
			await wait(300 * (attempt + 1));
		}
	}
}

function postJSON<T>(path: string, body: unknown, signal?: AbortSignal): Promise<T> {
	return fetchJSON<T>(
		path,
		{
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(body)
		},
		signal
	);
}

/**
 * Ask the server which moves have been played from a given position.
 *
 * Sent as a GET so it's a CORS "simple request" — no OPTIONS preflight per
 * lookup — and so the browser HTTP cache can serve repeated positions
 * (the server marks lookups cacheable for a day).
 *
 * In dev, `/api` is proxied to the Rust server (see vite.config.ts).
 */
export function lookup(fen: string, signal?: AbortSignal): Promise<LookupResponse> {
	return fetchJSON<LookupResponse>(`/api/lookup?fen=${encodeURIComponent(fen)}`, {}, signal);
}

/**
 * The set of move UCIs the book has already seen from this position, for
 * Unprecedented mode. A 4xx means the server rejected the FEN as an illegal
 * position — which is exactly a position no real game ever reached, so it has
 * no precedents: an empty set, not an error.
 */
export async function forbidden(fen: string, signal?: AbortSignal): Promise<Set<string>> {
	try {
		const resp = await lookup(fen, signal);
		return new Set(resp.moves.map((m) => m.uci));
	} catch (e) {
		if (e instanceof HttpError && e.status >= 400 && e.status < 500) return new Set();
		throw e;
	}
}

export interface SourcePlayer {
	name: string;
	rating: number | null;
}

/** The real lichess game behind a fully played-out line. */
export interface SourceGame {
	/** Lichess game id; the game lives at https://lichess.org/{id}. */
	id: string;
	white: SourcePlayer;
	black: SourcePlayer;
	winner: 'white' | 'black' | null;
	speed: string | null;
	/** Month the game was played, e.g. "2012-12". */
	month: string | null;
	/** False when the line outran the explorer's index depth and this is the
	 * best result-matching candidate rather than a certainty. */
	exact: boolean;
}

/**
 * Ask the server which real lichess game a finished line replayed. Returns
 * null when it can't be pinned down (no token configured, explorer hiccup,
 * or several games shared the line as deep as the explorer indexes).
 */
export async function identify(ucis: string[], signal?: AbortSignal): Promise<SourceGame | null> {
	const resp = await postJSON<{ game: SourceGame | null }>('/api/identify', { ucis }, signal);
	return resp.game;
}

/** Static facts about the loaded book (archive size, for the header). */
export async function meta(): Promise<MetaResponse> {
	const res = await fetch(API_BASE + '/api/meta');
	if (!res.ok) {
		throw new Error(`meta failed: ${res.status}`);
	}
	return res.json();
}
