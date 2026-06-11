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

/**
 * Ask the server which moves have been played from a given position.
 *
 * In dev, `/api` is proxied to the Rust server (see vite.config.ts).
 */
export async function lookup(fen: string): Promise<LookupResponse> {
	const res = await fetch('/api/lookup', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ fen })
	});
	if (!res.ok) {
		throw new Error(`lookup failed: ${res.status}`);
	}
	return res.json();
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
export async function identify(ucis: string[]): Promise<SourceGame | null> {
	const res = await fetch('/api/identify', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ ucis })
	});
	if (!res.ok) {
		throw new Error(`identify failed: ${res.status}`);
	}
	return (await res.json()).game;
}

/** Static facts about the loaded book (archive size, for the header). */
export async function meta(): Promise<MetaResponse> {
	const res = await fetch('/api/meta');
	if (!res.ok) {
		throw new Error(`meta failed: ${res.status}`);
	}
	return res.json();
}
