import { Chess } from 'chess.js';
import { identify, lookup, type KnownMove, type SourceGame } from '$lib/api';
import { moveSound, forcedSound, endSound } from '$lib/audio';

export const START = new Chess().fen();

export type Phase = 'loading' | 'choose' | 'forced' | 'over' | 'dry' | 'error';

export interface HistEntry {
	san: string;
	/** Long algebraic, e.g. "e7e8q", replayed server-side to identify the game. */
	uci: string;
	mover: 'w' | 'b';
	/** Games that played this move from the position it was made in. */
	count: number;
	/** How many known moves existed when it was made (1 = autoplayed). */
	choices: number;
	/** FEN before the move, for takebacks. */
	preFen: string;
	from: string;
	to: string;
}

// ——— captured material ———————————————————————————————————————————————

const VALUES: Record<string, number> = { p: 1, n: 3, b: 3, r: 5, q: 9 };
const START_COUNTS: Record<string, number> = { p: 8, n: 2, b: 2, r: 2, q: 1 };
const ORDER = ['q', 'r', 'b', 'n', 'p'];

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/**
 * The whole game in one reactive object: the position, what the archive
 * knows about it, and the phase machine that walks games forward
 * (lookup -> choose -> forced lines -> the end).
 */
export class KnownGame {
	fen = $state(START);
	known = $state<KnownMove[]>([]);
	/**
	 * The last set of real choices, kept through the brief lookup after a move
	 * so the continuations list doesn't unmount/remount (and shove the layout
	 * around) on every ply. Cleared when a forced line or the end takes over.
	 */
	lastChoices = $state<KnownMove[]>([]);
	total = $state(0);
	phase = $state<Phase>('loading');
	errorMsg = $state('');
	history = $state<HistEntry[]>([]);
	sound = $state(true);
	/** A move (UCI) to spotlight on the board, e.g. while hovering a continuation. */
	previewUci = $state<string | null>(null);
	/** The real lichess game a finished line replayed, once identified. */
	sourceGame = $state<SourceGame | null>(null);
	/** Lifecycle of the lichess trace, so the veil can show progress/outcome. */
	sourceState = $state<'idle' | 'searching' | 'found' | 'none'>('idle');
	/** Bumped on new-game/takeback so in-flight lookups and autoplay timers abort. */
	private gen = 0;

	chess = $derived(new Chess(this.fen));
	turn = $derived(this.chess.turn());
	interactive = $derived(this.phase === 'choose');
	lastEntry = $derived(this.history.length ? this.history[this.history.length - 1] : null);
	lastMove = $derived(this.lastEntry ? { from: this.lastEntry.from, to: this.lastEntry.to } : null);

	checkSquare = $derived.by(() => {
		if (!this.chess.inCheck()) return null;
		for (const row of this.chess.board()) {
			for (const p of row) {
				if (p && p.type === 'k' && p.color === this.turn) return p.square;
			}
		}
		return null;
	});

	material = $derived.by(() => {
		const present = {
			w: { p: 0, n: 0, b: 0, r: 0, q: 0 } as Record<string, number>,
			b: { p: 0, n: 0, b: 0, r: 0, q: 0 } as Record<string, number>
		};
		for (const row of this.chess.board()) {
			for (const p of row) {
				if (p && p.type !== 'k') present[p.color][p.type]++;
			}
		}
		// Pieces each side has taken (= the opponent's missing pieces).
		const taken = (victim: 'w' | 'b') =>
			ORDER.flatMap((t) => Array(Math.max(0, START_COUNTS[t] - present[victim][t])).fill(t) as string[]);
		const caps = { w: taken('b'), b: taken('w') };
		const score = (list: string[]) => list.reduce((s, t) => s + VALUES[t], 0);
		const diff = score(caps.w) - score(caps.b);
		return { caps, diff };
	});

	name(color: 'w' | 'b'): string {
		return color === 'w' ? 'White' : 'Black';
	}

	toMoveName = $derived.by(() => this.name(this.turn));

	result = $derived.by(() => {
		if (this.phase !== 'over') return null;
		if (this.chess.isCheckmate()) {
			const winner = this.turn === 'w' ? 'b' : 'w';
			return { title: 'Checkmate.', detail: `${this.name(winner)} wins` };
		}
		if (this.chess.isStalemate()) return { title: 'Stalemate.', detail: 'Drawn, no legal reply' };
		if (this.chess.isInsufficientMaterial()) return { title: 'Dead position.', detail: 'Drawn, bare kings' };
		return { title: 'Draw.', detail: 'Game drawn' };
	});

	// ——— game loop ———————————————————————————————————————————————————————

	private async step(g: number): Promise<void> {
		const pos = new Chess(this.fen);
		if (pos.isGameOver()) {
			this.phase = 'over';
			this.known = [];
			this.lastChoices = [];
			// The hero counter lands on the games that ended exactly here.
			if (this.lastEntry) this.total = this.lastEntry.count;
			if (this.sound) endSound(pos.isCheckmate());
			if (this.history.length) void this.findSource(g);
			return;
		}
		this.phase = 'loading';
		let resp;
		try {
			resp = await lookup(this.fen);
		} catch (e) {
			if (g !== this.gen) return;
			this.phase = 'error';
			this.errorMsg = e instanceof Error ? e.message : String(e);
			return;
		}
		if (g !== this.gen) return;
		this.known = resp.moves;
		this.total = resp.total;

		if (this.known.length === 0) {
			this.lastChoices = [];
			this.phase = 'dry';
			return;
		}
		if (this.known.length === 1) {
			// Locked in: every remaining game continued the same way, so the
			// archive plays the move itself after a beat.
			this.lastChoices = [];
			this.phase = 'forced';
			await delay(800);
			if (g !== this.gen) return;
			this.applyMove(this.known[0], true);
			return this.step(g);
		}
		this.lastChoices = this.known;
		this.phase = 'choose';
	}

	/** Look up which real game the finished line replayed; quietly optional. */
	private async findSource(g: number): Promise<void> {
		this.sourceState = 'searching';
		try {
			const game = await identify(this.history.map((h) => h.uci));
			if (g !== this.gen) return;
			this.sourceGame = game;
			this.sourceState = game ? 'found' : 'none';
		} catch {
			// The link is a bonus; the veil works fine without it.
			if (g !== this.gen) return;
			this.sourceState = 'none';
		}
	}

	private applyMove(mv: KnownMove, auto = false) {
		const pos = new Chess(this.fen);
		const preFen = this.fen;
		const mover = pos.turn();
		const choices = this.known.length;
		const res = pos.move({
			from: mv.uci.slice(0, 2),
			to: mv.uci.slice(2, 4),
			promotion: mv.uci.slice(4, 5) || undefined
		});
		this.history = [...this.history, { san: res.san, uci: mv.uci, mover, count: mv.count, choices, preFen, from: res.from, to: res.to }];
		this.fen = pos.fen();
		if (this.sound) {
			const capture = res.captured !== undefined;
			if (auto) forcedSound(capture);
			else moveSound(capture);
		}
	}

	choose(mv: KnownMove) {
		if (this.phase !== 'choose') return;
		this.previewUci = null;
		this.applyMove(mv);
		void this.step(this.gen);
	}

	chooseUci(uci: string) {
		const mv = this.known.find((m) => m.uci === uci);
		if (mv) this.choose(mv);
	}

	newGame() {
		this.gen++;
		this.fen = START;
		this.history = [];
		this.known = [];
		this.lastChoices = [];
		this.total = 0;
		this.previewUci = null;
		this.sourceGame = null;
		this.sourceState = 'idle';
		this.phase = 'loading';
		void this.step(this.gen);
	}

	/** Rewind to the most recent position where there was a genuine choice. */
	takeback() {
		if (!this.history.length || this.phase === 'loading') return;
		this.gen++;
		const h = [...this.history];
		let entry = h.pop()!;
		while (h.length && entry.choices <= 1) entry = h.pop()!;
		this.history = h;
		this.fen = entry.preFen;
		this.previewUci = null;
		this.sourceGame = null;
		this.sourceState = 'idle';
		void this.step(this.gen);
	}

	retry() {
		this.gen++;
		void this.step(this.gen);
	}
}
