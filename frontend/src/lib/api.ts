/** A move known to the book, as returned by the server. */
export interface KnownMove {
	/** Long algebraic, e.g. "e2e4" or "e7e8q". Feed straight into chess.js. */
	uci: string;
	/** Standard algebraic, e.g. "Nf3" — for display. */
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
