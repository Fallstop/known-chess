/**
 * Build-time mode flag. The same codebase ships two games off one backend:
 * "precedent" (only moves with a real precedent) and "unprecedented" (only
 * moves never played, with friendly fire). The second Cloudflare Pages project
 * sets PUBLIC_KC_MODE=unprecedented; everything mode-specific is gathered here.
 */
import { PUBLIC_KC_MODE } from '$env/static/public';

export type Mode = 'precedent' | 'unprecedented';
export const MODE: Mode = PUBLIC_KC_MODE === 'unprecedented' ? 'unprecedented' : 'precedent';

interface Step {
	title: string;
	body: string;
}

export interface Copy {
	/** Wordmark halves, joined by a slash glyph in the header. */
	brandA: string;
	brandB: string;
	title: string;
	description: string;
	canonical: string;
	ogImage: string;
	lede: string;
	steps: [Step, Step, Step];
	footNote: string;
	/** Ledger pieces that only make sense with the book behind them. */
	showCounts: boolean;
	showRemainCounter: boolean;
	showContinuations: boolean;
}

const PRECEDENT: Copy = {
	brandA: 'Precedent',
	brandB: 'Chess',
	title: 'Precedent Chess: every move once lived',
	description:
		'Chess where every move needs a precedent: it counts only if it was played from your exact position in a real game. Play until a single game remains, and watch its real ending.',
	canonical: 'https://precedent.jmw.nz/',
	ogImage: 'https://precedent.jmw.nz/og.png',
	lede: 'Chess where a move is legal only if it has precedent: someone, somewhere, played it from this exact position in a real Lichess game. The deeper you go, the fewer games ever got there, until a single game remains and plays out its real ending.',
	steps: [
		{ title: 'Pick a move', body: 'Only moves actually played from this position are open to you. The small count on each square is how many games chose it.' },
		{ title: 'Positions get rarer', body: 'Each position is looked up in the record on its own, whatever the move order. The deeper you go, the fewer real games ever reached it.' },
		{ title: 'One game remains', body: 'Once a single game ever reached your position, precedent takes over and plays out its real ending, which you can open on Lichess.' }
	],
	footNote: 'Every count is real games that reached your exact position. Moves played for you by precedent appear dimmed.',
	showCounts: true,
	showRemainCounter: true,
	showContinuations: true
};

const UNPRECEDENTED: Copy = {
	brandA: 'Unprecedented',
	brandB: 'Chess',
	title: 'Unprecedented Chess: only moves no one has ever played',
	description:
		'Chess where every move must be one never played from your position. Pieces move and block as normal, except you can capture your own pieces. You cannot take a king. Checkmate to win.',
	canonical: 'https://unprecedented.jmw.nz/',
	ogImage: 'https://unprecedented.jmw.nz/og-unprecedented.png',
	lede: 'Chess where a move is legal only if it has never been played from this exact position in a real Lichess game. Pieces still move and block normally, but you can capture your own pieces, so the queen can come out from behind the pawns. You still cannot take a king. Checkmate to win.',
	steps: [
		{ title: 'Make history', body: 'You may only play a move never recorded from this position. Every real first move is already taken, so you begin by capturing one of your own pieces.' },
		{ title: 'Friendly fire', body: 'Pieces move by their normal shape and are still blocked, with no jumping. The twist: you can capture your own pieces to clear a path.' },
		{ title: 'Checkmate to win', body: 'Neither king can ever be captured. Trap the enemy king with no unplayed escape and it is mate.' }
	],
	footNote: 'A move counts only if no real Lichess game ever played it from your exact position. Kings can never be captured, so win by checkmate.',
	showCounts: false,
	showRemainCounter: false,
	showContinuations: false
};

export const copy: Copy = MODE === 'unprecedented' ? UNPRECEDENTED : PRECEDENT;
