/**
 * A tiny chess engine for *Unprecedented* mode, where chess.js can't help:
 * pieces move by their normal geometry **with blocking** (no jumps), but you
 * may capture your OWN pieces (friendly fire). Neither king is ever capturable.
 * The resulting positions are routinely illegal by FIDE rules, so this board
 * validates nothing about plausibility — it only enforces the rules above plus
 * king-safety (you may not end your turn with your own king attacked).
 *
 * Squares are indexed 0..63 as rank*8 + file, a1 = 0, h8 = 63. Everything is
 * value-style: `apply` returns a fresh state, leaving the input untouched.
 */

export type Color = 'w' | 'b';
export type PieceType = 'p' | 'n' | 'b' | 'r' | 'q' | 'k';
export interface Piece {
	color: Color;
	type: PieceType;
}

export interface State {
	board: (Piece | null)[];
	turn: Color;
	/** Castling availability, keyed by side. */
	castling: { wk: boolean; wq: boolean; bk: boolean; bq: boolean };
	/** En-passant target square index (the empty square a pawn skipped), or null. */
	ep: number | null;
	halfmove: number;
	fullmove: number;
}

/** A generated move with the strings the UI and book filter need. */
export interface EngineMove {
	uci: string;
	san: string;
	from: number;
	to: number;
	promo?: PieceType;
}

const FILES = 'abcdefgh';
const fileOf = (sq: number) => sq & 7;
const rankOf = (sq: number) => sq >> 3;
const idx = (f: number, r: number) => r * 8 + f;
const onBoard = (f: number, r: number) => f >= 0 && f < 8 && r >= 0 && r < 8;
export const sqName = (sq: number) => FILES[fileOf(sq)] + (rankOf(sq) + 1);
const nameToSq = (s: string) => idx(s.charCodeAt(0) - 97, s.charCodeAt(1) - 49);
const other = (c: Color): Color => (c === 'w' ? 'b' : 'w');

// ——— FEN ————————————————————————————————————————————————————————————————

export const START_FEN = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1';

/** Parse a FEN with no legality checks; unknown/short fields get defaults. */
export function parseFen(fen: string): State {
	const [place, turn = 'w', castle = '-', ep = '-', half = '0', full = '1'] = fen.trim().split(/\s+/);
	const board: (Piece | null)[] = Array(64).fill(null);
	const rows = place.split('/'); // rows[0] is rank 8
	for (let r = 0; r < 8; r++) {
		const row = rows[7 - r] ?? '';
		let f = 0;
		for (const ch of row) {
			if (ch >= '1' && ch <= '8') f += Number(ch);
			else {
				const color: Color = ch === ch.toUpperCase() ? 'w' : 'b';
				board[idx(f, r)] = { color, type: ch.toLowerCase() as PieceType };
				f++;
			}
		}
	}
	return {
		board,
		turn: turn === 'b' ? 'b' : 'w',
		castling: {
			wk: castle.includes('K'),
			wq: castle.includes('Q'),
			bk: castle.includes('k'),
			bq: castle.includes('q')
		},
		ep: ep !== '-' ? nameToSq(ep) : null,
		halfmove: Number(half) || 0,
		fullmove: Number(full) || 1
	};
}

export function toFen(s: State): string {
	const rows: string[] = [];
	for (let r = 7; r >= 0; r--) {
		let row = '';
		let empty = 0;
		for (let f = 0; f < 8; f++) {
			const p = s.board[idx(f, r)];
			if (!p) empty++;
			else {
				if (empty) row += empty;
				empty = 0;
				row += p.color === 'w' ? p.type.toUpperCase() : p.type;
			}
		}
		if (empty) row += empty;
		rows.push(row);
	}
	const c =
		(s.castling.wk ? 'K' : '') +
		(s.castling.wq ? 'Q' : '') +
		(s.castling.bk ? 'k' : '') +
		(s.castling.bq ? 'q' : '') || '-';
	return `${rows.join('/')} ${s.turn} ${c} ${s.ep !== null ? sqName(s.ep) : '-'} ${s.halfmove} ${s.fullmove}`;
}

/** Square → piece map for rendering, tolerant of illegal positions. */
export function boardPieces(fen: string): Map<string, Piece> {
	const m = new Map<string, Piece>();
	const s = parseFen(fen);
	for (let sq = 0; sq < 64; sq++) {
		const p = s.board[sq];
		if (p) m.set(sqName(sq), p);
	}
	return m;
}

// ——— attack geometry ————————————————————————————————————————————————————

const KNIGHT: [number, number][] = [
	[1, 2], [2, 1], [2, -1], [1, -2], [-1, -2], [-2, -1], [-2, 1], [-1, 2]
];
const ROOK_DIRS: [number, number][] = [[1, 0], [-1, 0], [0, 1], [0, -1]];
const BISHOP_DIRS: [number, number][] = [[1, 1], [1, -1], [-1, 1], [-1, -1]];
const KING_DIRS = [...ROOK_DIRS, ...BISHOP_DIRS];

/**
 * Is `sq` attacked by a piece of `by`? Standard blocked-ray geometry: friendly
 * fire doesn't change what attacks a square (an enemy slider still attacks only
 * the first piece in its path), so this is ordinary chess attack detection.
 */
function attacked(s: State, by: Color, sq: number): boolean {
	const f = fileOf(sq);
	const r = rankOf(sq);
	// Pawns attack diagonally toward their advance direction.
	const pr = by === 'w' ? r - 1 : r + 1;
	for (const df of [-1, 1]) {
		if (onBoard(f + df, pr)) {
			const p = s.board[idx(f + df, pr)];
			if (p && p.color === by && p.type === 'p') return true;
		}
	}
	for (const [df, dr] of KNIGHT) {
		if (!onBoard(f + df, r + dr)) continue;
		const p = s.board[idx(f + df, r + dr)];
		if (p && p.color === by && p.type === 'n') return true;
	}
	for (const [df, dr] of KING_DIRS) {
		if (!onBoard(f + df, r + dr)) continue;
		const p = s.board[idx(f + df, r + dr)];
		if (p && p.color === by && p.type === 'k') return true;
	}
	const ray = (dirs: [number, number][], types: PieceType[]) => {
		for (const [df, dr] of dirs) {
			let nf = f + df;
			let nr = r + dr;
			while (onBoard(nf, nr)) {
				const p = s.board[idx(nf, nr)];
				if (p) {
					if (p.color === by && types.includes(p.type)) return true;
					break; // any piece (incl. a king) blocks the ray
				}
				nf += df;
				nr += dr;
			}
		}
		return false;
	};
	return ray(ROOK_DIRS, ['r', 'q']) || ray(BISHOP_DIRS, ['b', 'q']);
}

function kingSquare(s: State, color: Color): number {
	for (let sq = 0; sq < 64; sq++) {
		const p = s.board[sq];
		if (p && p.type === 'k' && p.color === color) return sq;
	}
	return -1; // a king is never capturable, so this shouldn't happen
}

export function inCheck(s: State, color: Color): boolean {
	const k = kingSquare(s, color);
	return k >= 0 && attacked(s, other(color), k);
}

// ——— move generation ————————————————————————————————————————————————————

interface Raw {
	from: number;
	to: number;
	promo?: PieceType;
	/** Castling side, so apply() can hop the rook. */
	castle?: 'K' | 'Q';
	/** En-passant capture: the pawn removed sits behind `to`. */
	ep?: boolean;
	/** Double pawn push, so apply() arms the next ep target. */
	double?: boolean;
}

/** A square is a capture target unless it's empty or holds a king. */
const capturable = (p: Piece | null): p is Piece => p !== null && p.type !== 'k';

function pseudoMoves(s: State): Raw[] {
	const out: Raw[] = [];
	const me = s.turn;
	for (let from = 0; from < 64; from++) {
		const p = s.board[from];
		if (!p || p.color !== me) continue;
		const f = fileOf(from);
		const r = rankOf(from);

		const step = (dirs: [number, number][]) => {
			for (const [df, dr] of dirs) {
				const nf = f + df;
				const nr = r + dr;
				if (!onBoard(nf, nr)) continue;
				const t = s.board[idx(nf, nr)];
				if (!t || capturable(t)) out.push({ from, to: idx(nf, nr) });
			}
		};
		const slide = (dirs: [number, number][]) => {
			for (const [df, dr] of dirs) {
				let nf = f + df;
				let nr = r + dr;
				while (onBoard(nf, nr)) {
					const t = s.board[idx(nf, nr)];
					if (!t) {
						out.push({ from, to: idx(nf, nr) });
					} else {
						if (capturable(t)) out.push({ from, to: idx(nf, nr) });
						break; // blocked (a king blocks too, since it can't be taken)
					}
					nf += df;
					nr += dr;
				}
			}
		};

		switch (p.type) {
			case 'n':
				step(KNIGHT);
				break;
			case 'k':
				step(KING_DIRS);
				castleMoves(s, me, out);
				break;
			case 'r':
				slide(ROOK_DIRS);
				break;
			case 'b':
				slide(BISHOP_DIRS);
				break;
			case 'q':
				slide(KING_DIRS);
				break;
			case 'p':
				pawnMoves(s, from, f, r, me, out);
				break;
		}
	}
	return out;
}

function pawnMoves(s: State, from: number, f: number, r: number, me: Color, out: Raw[]) {
	const dr = me === 'w' ? 1 : -1;
	const startRank = me === 'w' ? 1 : 6;
	const lastRank = me === 'w' ? 7 : 0;
	const push = (to: number, flags: Partial<Raw> = {}) => {
		if (rankOf(to) === lastRank) for (const promo of ['q', 'r', 'b', 'n'] as PieceType[]) out.push({ from, to, promo });
		else out.push({ from, to, ...flags });
	};
	// Forward (only onto empty squares — no jumping).
	if (onBoard(f, r + dr) && !s.board[idx(f, r + dr)]) {
		push(idx(f, r + dr));
		if (r === startRank && !s.board[idx(f, r + 2 * dr)]) out.push({ from, to: idx(f, r + 2 * dr), double: true });
	}
	// Diagonal captures — friend or foe, plus en passant against enemy pawns.
	for (const df of [-1, 1]) {
		const nf = f + df;
		const nr = r + dr;
		if (!onBoard(nf, nr)) continue;
		const to = idx(nf, nr);
		if (capturable(s.board[to])) push(to);
		else if (s.ep === to && !s.board[to]) out.push({ from, to, ep: true });
	}
}

function castleMoves(s: State, me: Color, out: Raw[]) {
	const r = me === 'w' ? 0 : 7;
	const home = idx(4, r);
	const k = s.board[home];
	if (!k || k.type !== 'k' || k.color !== me) return;
	if (attacked(s, other(me), home)) return; // can't castle out of check
	const can = me === 'w' ? { k: s.castling.wk, q: s.castling.wq } : { k: s.castling.bk, q: s.castling.bq };
	const rook = (f: number) => s.board[idx(f, r)];
	const empty = (...fs: number[]) => fs.every((f) => !s.board[idx(f, r)]);
	const safe = (...fs: number[]) => fs.every((f) => !attacked(s, other(me), idx(f, r)));
	if (can.k && rook(7)?.type === 'r' && rook(7)?.color === me && empty(5, 6) && safe(5, 6))
		out.push({ from: home, to: idx(6, r), castle: 'K' });
	if (can.q && rook(0)?.type === 'r' && rook(0)?.color === me && empty(1, 2, 3) && safe(2, 3))
		out.push({ from: home, to: idx(2, r), castle: 'Q' });
}

// ——— apply ——————————————————————————————————————————————————————————————

function applyRaw(s: State, m: Raw): State {
	const board = s.board.slice();
	const piece = board[m.from]!;
	const me = s.turn;
	const r = me === 'w' ? 0 : 7;
	const capture = board[m.to] !== null || m.ep;

	if (m.ep) board[idx(fileOf(m.to), rankOf(m.from))] = null; // remove the bypassed pawn
	board[m.to] = m.promo ? { color: me, type: m.promo } : piece;
	board[m.from] = null;
	if (m.castle === 'K') {
		board[idx(5, r)] = board[idx(7, r)];
		board[idx(7, r)] = null;
	} else if (m.castle === 'Q') {
		board[idx(3, r)] = board[idx(0, r)];
		board[idx(0, r)] = null;
	}

	// Recompute castling rights from the board: a right survives only while its
	// king and rook still sit home (covers moves, captures, and self-captures).
	const at = (f: number, rk: number, c: Color, t: PieceType) => {
		const p = board[idx(f, rk)];
		return !!p && p.color === c && p.type === t;
	};
	const castling = {
		wk: s.castling.wk && at(4, 0, 'w', 'k') && at(7, 0, 'w', 'r'),
		wq: s.castling.wq && at(4, 0, 'w', 'k') && at(0, 0, 'w', 'r'),
		bk: s.castling.bk && at(4, 7, 'b', 'k') && at(7, 7, 'b', 'r'),
		bq: s.castling.bq && at(4, 7, 'b', 'k') && at(0, 7, 'b', 'r')
	};

	return {
		board,
		turn: other(me),
		castling,
		ep: m.double ? idx(fileOf(m.from), rankOf(m.from) + (me === 'w' ? 1 : -1)) : null,
		halfmove: piece.type === 'p' || capture ? 0 : s.halfmove + 1,
		fullmove: s.fullmove + (me === 'b' ? 1 : 0)
	};
}

// ——— public API: legal moves + apply ————————————————————————————————————

function rawUci(m: Raw): string {
	return sqName(m.from) + sqName(m.to) + (m.promo ?? '');
}

function rawSan(s: State, m: Raw, all: Raw[]): string {
	if (m.castle === 'K') return withCheck(s, m, 'O-O');
	if (m.castle === 'Q') return withCheck(s, m, 'O-O-O');
	const p = s.board[m.from]!;
	const capture = m.ep || s.board[m.to] !== null;
	const dest = sqName(m.to);
	let san: string;
	if (p.type === 'p') {
		san = (capture ? FILES[fileOf(m.from)] + 'x' : '') + dest + (m.promo ? '=' + m.promo.toUpperCase() : '');
	} else {
		// Disambiguate against other same-type pieces that also reach `to`.
		const rivals = all.filter(
			(o) => o.to === m.to && o.from !== m.from && s.board[o.from]?.type === p.type
		);
		let dis = '';
		if (rivals.length) {
			const sameFile = rivals.some((o) => fileOf(o.from) === fileOf(m.from));
			const sameRank = rivals.some((o) => rankOf(o.from) === rankOf(m.from));
			dis = !sameFile ? FILES[fileOf(m.from)] : !sameRank ? String(rankOf(m.from) + 1) : sqName(m.from);
		}
		san = p.type.toUpperCase() + dis + (capture ? 'x' : '') + dest;
	}
	return withCheck(s, m, san);
}

/** Append '+' when the move leaves the opponent's king attacked. */
function withCheck(s: State, m: Raw, san: string): string {
	return inCheck(applyRaw(s, m), other(s.turn)) ? san + '+' : san;
}

/** King-safe moves for the side to move, with UCI + SAN attached. */
export function legalMoves(s: State): EngineMove[] {
	const pseudo = pseudoMoves(s);
	const legal = pseudo.filter((m) => !inCheck(applyRaw(s, m), s.turn));
	return legal.map((m) => ({ uci: rawUci(m), san: rawSan(s, m, legal), from: m.from, to: m.to, promo: m.promo }));
}

/** Apply a UCI move; returns the new state, or null if it isn't legal here. */
export function applyUci(s: State, uci: string): { state: State; san: string } | null {
	const from = nameToSq(uci.slice(0, 2));
	const to = nameToSq(uci.slice(2, 4));
	const promo = (uci.slice(4, 5) || undefined) as PieceType | undefined;
	const legal = pseudoMoves(s).filter((m) => !inCheck(applyRaw(s, m), s.turn));
	const m = legal.find((x) => x.from === from && x.to === to && x.promo === promo);
	if (!m) return null;
	return { state: applyRaw(s, m), san: rawSan(s, m, legal) };
}
