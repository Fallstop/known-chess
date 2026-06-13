import { Chess } from 'chess.js';
import { identify, lookup, type KnownMove, type LookupResponse, type SourceGame } from '$lib/api';
import { moveSound, forcedSound, endSound } from '$lib/audio';
import posthog from 'posthog-js';

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
	/** Piece type captured by this move, if any (Unprecedented material tally). */
	captured?: string;
}

/**
 * The reactive surface the board and ledger components read. Both game modes
 * (KnownGame and UnprecedentedGame) implement it, so the same components drive
 * either game. Fields that only make sense with the book behind them
 * (lastChoices, total, sourceGame…) stay present but inert in Unprecedented.
 */
export interface GameView {
	fen: string;
	known: KnownMove[];
	lastChoices: KnownMove[];
	total: number;
	phase: Phase;
	errorMsg: string;
	history: HistEntry[];
	sound: boolean;
	previewUci: string | null;
	sourceGame: SourceGame | null;
	sourceState: 'idle' | 'searching' | 'found' | 'none';
	readonly turn: 'w' | 'b';
	readonly interactive: boolean;
	readonly lastEntry: HistEntry | null;
	readonly lastMove: { from: string; to: string } | null;
	readonly checkSquare: string | null;
	readonly material: { caps: Record<'w' | 'b', string[]>; diff: number };
	readonly toMoveName: string;
	readonly result: { title: string; detail: string } | null;
	newGame(): void;
	takeback(): void;
	retry(): void;
	choose(mv: KnownMove): void;
	chooseUci(uci: string): void;
	warm(uci: string): void;
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
export class KnownGame implements GameView {
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
	/** Cancels requests belonging to an abandoned gen; replaced on each bump. */
	private aborter = new AbortController();
	/**
	 * Speculative lookups for moves the player is eyeing (hovered piece,
	 * previewed continuation), keyed by UCI. Only valid for the current
	 * position: cleared when a move lands or the game rewinds/resets.
	 */
	private speculative = new Map<string, Promise<LookupResponse> | undefined>();

	private bump() {
		this.gen++;
		this.aborter.abort();
		this.aborter = new AbortController();
		this.speculative.clear();
	}

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

	private async step(g: number, prefetched?: Promise<LookupResponse>): Promise<void> {
		const pos = new Chess(this.fen);
		if (pos.isGameOver()) {
			this.phase = 'over';
			this.known = [];
			this.lastChoices = [];
			// The hero counter lands on the games that ended exactly here.
			if (this.lastEntry) this.total = this.lastEntry.count;
			if (this.sound) endSound(pos.isCheckmate());
			if (this.history.length) void this.findSource(g);
			const resultType = pos.isCheckmate()
				? 'checkmate'
				: pos.isStalemate()
					? 'stalemate'
					: pos.isInsufficientMaterial()
						? 'insufficient_material'
						: 'draw';
			posthog.capture('game_completed', {
				result_type: resultType,
				moves_played: this.history.length,
				matching_games: this.lastEntry?.count ?? 0
			});
			return;
		}
		this.phase = 'loading';
		let resp;
		try {
			resp = await (prefetched ?? lookup(this.fen, this.aborter.signal));
		} catch (first) {
			if (g !== this.gen) return;
			try {
				// A dead prefetch shouldn't sink the game when a fresh lookup
				// of the same position would succeed; give it one shot.
				if (!prefetched) throw first;
				resp = await lookup(this.fen, this.aborter.signal);
			} catch (e) {
				if (g !== this.gen) return;
				this.phase = 'error';
				this.errorMsg = e instanceof Error ? e.message : String(e);
				return;
			}
		}
		if (g !== this.gen) return;
		this.known = resp.moves;
		this.total = resp.total;

		if (this.known.length === 0) {
			this.lastChoices = [];
			this.phase = 'dry';
			posthog.capture('position_off_record', {
				moves_played: this.history.length
			});
			return;
		}
		if (this.known.length === 1) {
			// Locked in: every remaining game continued the same way, so the
			// archive plays the move itself after a beat. Look up the position
			// after that move now, so the next ply is ready when the beat ends.
			this.lastChoices = [];
			this.phase = 'forced';
			const ahead = this.prefetch(this.known[0]);
			await delay(800);
			if (g !== this.gen) return;
			this.applyMove(this.known[0], true);
			return this.step(g, ahead);
		}
		this.lastChoices = this.known;
		this.phase = 'choose';
	}

	/**
	 * Start the lookup for the position a move leads to, so it can run while
	 * the move's animation/beat plays. Undefined when the move ends the game
	 * (step handles that without a lookup). Errors surface when awaited; the
	 * stray catch only stops an unhandled rejection if the run is abandoned.
	 */
	private prefetch(mv: KnownMove): Promise<LookupResponse> | undefined {
		const next = new Chess(this.fen);
		next.move({
			from: mv.uci.slice(0, 2),
			to: mv.uci.slice(2, 4),
			promotion: mv.uci.slice(4, 5) || undefined
		});
		if (next.isGameOver()) return undefined;
		const p = lookup(next.fen(), this.aborter.signal);
		p.catch(() => {});
		return p;
	}

	/**
	 * Warm the lookup for a candidate move the player seems interested in,
	 * so choosing it lands on an already-fetched answer.
	 */
	warm(uci: string) {
		if (this.phase !== 'choose' || this.speculative.has(uci)) return;
		const mv = this.known.find((m) => m.uci === uci);
		if (mv) this.speculative.set(uci, this.prefetch(mv));
	}

	/** Look up which real game the finished line replayed; quietly optional. */
	private async findSource(g: number): Promise<void> {
		this.sourceState = 'searching';
		try {
			const game = await identify(
				this.history.map((h) => h.uci),
				this.aborter.signal
			);
			if (g !== this.gen) return;
			this.sourceGame = game;
			this.sourceState = game ? 'found' : 'none';
			if (game) {
				posthog.capture('source_game_found', {
					exact_match: game.exact,
					lichess_id: game.id,
					speed: game.speed
				});
			}
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
		this.speculative.clear();
		if (this.sound) {
			const capture = res.captured !== undefined;
			if (auto) forcedSound(capture);
			else moveSound(capture);
		}
	}

	choose(mv: KnownMove) {
		if (this.phase !== 'choose') return;
		this.previewUci = null;
		posthog.capture('move_chosen', {
			san: mv.san,
			uci: mv.uci,
			game_count: mv.count,
			choices_available: this.known.length,
			move_number: this.history.length + 1
		});
		// Grab any hover-warmed lookup before applyMove clears the cache.
		const ahead = this.speculative.get(mv.uci);
		this.applyMove(mv);
		void this.step(this.gen, ahead);
	}

	chooseUci(uci: string) {
		const mv = this.known.find((m) => m.uci === uci);
		if (mv) this.choose(mv);
	}

	newGame() {
		this.bump();
		posthog.capture('game_started', {
			previous_moves: this.history.length
		});
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
		this.bump();
		const h = [...this.history];
		const movesBeforeTakeback = h.length;
		let entry = h.pop()!;
		while (h.length && entry.choices <= 1) entry = h.pop()!;
		posthog.capture('takeback_used', {
			moves_rewound: movesBeforeTakeback - h.length
		});
		this.history = h;
		this.fen = entry.preFen;
		this.previewUci = null;
		this.sourceGame = null;
		this.sourceState = 'idle';
		void this.step(this.gen);
	}

	retry() {
		this.bump();
		void this.step(this.gen);
	}
}
