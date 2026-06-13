import { forbidden, type KnownMove, type SourceGame } from '$lib/api';
import { moveSound, endSound } from '$lib/audio';
import {
	START_FEN,
	parseFen,
	toFen,
	legalMoves,
	applyUci,
	inCheck,
	type State
} from '$lib/freechess';
import type { GameView, HistEntry, Phase } from '$lib/game.svelte';
import posthog from 'posthog-js';

const VALUES: Record<string, number> = { p: 1, n: 3, b: 3, r: 5, q: 9 };
const ORDER = ['q', 'r', 'b', 'n', 'p'];

/**
 * Unprecedented mode: every move must be one no real game ever played from the
 * current position. Candidate moves come from the local engine (normal geometry
 * with blocking, but friendly fire allowed, no king capture); the book's known
 * moves are subtracted, and what's left is what you may play. One player steers
 * both sides, as in the precedent game.
 */
export class UnprecedentedGame implements GameView {
	fen = $state(START_FEN);
	known = $state<KnownMove[]>([]);
	total = $state(0);
	phase = $state<Phase>('loading');
	errorMsg = $state('');
	history = $state<HistEntry[]>([]);
	sound = $state(true);
	previewUci = $state<string | null>(null);

	// Precedent-only surface, kept inert so shared components compile.
	lastChoices = $state<KnownMove[]>([]);
	sourceGame = $state<SourceGame | null>(null);
	sourceState = $state<'idle' | 'searching' | 'found' | 'none'>('idle');

	/** Bumped on new-game/takeback so stale in-flight lookups abort. */
	private gen = 0;
	private aborter = new AbortController();

	private bump() {
		this.gen++;
		this.aborter.abort();
		this.aborter = new AbortController();
	}

	private state = $derived<State>(parseFen(this.fen));
	turn = $derived(this.state.turn);
	interactive = $derived(this.phase === 'choose');
	lastEntry = $derived(this.history.length ? this.history[this.history.length - 1] : null);
	lastMove = $derived(this.lastEntry ? { from: this.lastEntry.from, to: this.lastEntry.to } : null);
	checkSquare = $derived.by(() => {
		const s = this.state;
		if (!inCheck(s, s.turn)) return null;
		for (let sq = 0; sq < 64; sq++) {
			const p = s.board[sq];
			if (p && p.type === 'k' && p.color === s.turn)
				return 'abcdefgh'[sq & 7] + ((sq >> 3) + 1);
		}
		return null;
	});

	/**
	 * Captured material, tallied straight from the moves played and attributed to
	 * the mover. Board-diff (as the precedent game uses) would misread a friendly
	 * capture as the opponent's gain, so here history is the source of truth.
	 */
	material = $derived.by(() => {
		const caps: Record<'w' | 'b', string[]> = { w: [], b: [] };
		for (const h of this.history) if (h.captured) caps[h.mover].push(h.captured);
		const order = (list: string[]) => [...list].sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
		caps.w = order(caps.w);
		caps.b = order(caps.b);
		const score = (list: string[]) => list.reduce((s, t) => s + (VALUES[t] ?? 0), 0);
		return { caps, diff: score(caps.w) - score(caps.b) };
	});

	name(color: 'w' | 'b'): string {
		return color === 'w' ? 'White' : 'Black';
	}
	toMoveName = $derived(this.name(this.turn));

	result = $derived.by(() => {
		if (this.phase !== 'over') return null;
		const s = this.state;
		if (inCheck(s, s.turn)) {
			const winner = s.turn === 'w' ? 'b' : 'w';
			return { title: 'Checkmate.', detail: `${this.name(winner)} wins` };
		}
		return { title: 'Stuck.', detail: 'No unplayed move remains — drawn' };
	});

	// ——— game loop ————————————————————————————————————————————————————————

	private async step(g: number): Promise<void> {
		const s = parseFen(this.fen);
		const legal = legalMoves(s);
		// No king-safe move at all: mate if in check, else a stuck draw — decided
		// locally, no lookup needed.
		if (legal.length === 0) return this.end(g, s);

		this.phase = 'loading';
		let forbid: Set<string>;
		try {
			forbid = await forbidden(this.fen, this.aborter.signal);
		} catch (e) {
			if (g !== this.gen) return;
			this.phase = 'error';
			this.errorMsg = e instanceof Error ? e.message : String(e);
			return;
		}
		if (g !== this.gen) return;

		// Allowed = king-safe moves the book has never seen from here.
		const allowed = legal.filter((m) => !forbid.has(m.uci));
		this.known = allowed.map((m) => ({ uci: m.uci, san: m.san, count: 0 }));
		// Every escape is already on the record (or there are none): terminal.
		if (allowed.length === 0) return this.end(g, s);
		this.phase = 'choose';
	}

	private end(g: number, s: State) {
		if (g !== this.gen) return;
		this.known = [];
		this.phase = 'over';
		const mate = inCheck(s, s.turn);
		if (this.sound) endSound(mate);
		posthog.capture('game_completed', {
			result_type: mate ? 'checkmate' : 'stuck',
			moves_played: this.history.length,
			mode: 'unprecedented'
		});
	}

	private applyMove(mv: KnownMove) {
		const s = parseFen(this.fen);
		const res = applyUci(s, mv.uci);
		if (!res) return;
		const mover = s.turn;
		const victimSq = mv.uci.slice(2, 4);
		const victim = s.board['abcdefgh'.indexOf(victimSq[0]) + (Number(victimSq[1]) - 1) * 8];
		const captured = victim && victim.type !== 'k' ? victim.type : undefined;
		this.history = [
			...this.history,
			{
				san: res.san,
				uci: mv.uci,
				mover,
				count: 0,
				choices: this.known.length,
				preFen: this.fen,
				from: mv.uci.slice(0, 2),
				to: victimSq,
				captured
			}
		];
		this.fen = toFen(res.state);
		if (this.sound) moveSound(captured !== undefined);
	}

	choose(mv: KnownMove) {
		if (this.phase !== 'choose') return;
		this.previewUci = null;
		posthog.capture('move_chosen', {
			san: mv.san,
			uci: mv.uci,
			choices_available: this.known.length,
			move_number: this.history.length + 1,
			mode: 'unprecedented'
		});
		this.applyMove(mv);
		void this.step(this.gen);
	}

	chooseUci(uci: string) {
		const mv = this.known.find((m) => m.uci === uci);
		if (mv) this.choose(mv);
	}

	// Candidate moves are generated locally, so there's nothing to pre-warm.
	warm(_uci: string) {}

	newGame() {
		this.bump();
		posthog.capture('game_started', { previous_moves: this.history.length, mode: 'unprecedented' });
		this.fen = START_FEN;
		this.history = [];
		this.known = [];
		this.previewUci = null;
		this.phase = 'loading';
		void this.step(this.gen);
	}

	/** Rewind one move. Every move here is a genuine choice, so just step back. */
	takeback() {
		if (!this.history.length || this.phase === 'loading') return;
		this.bump();
		const h = [...this.history];
		const entry = h.pop()!;
		posthog.capture('takeback_used', { moves_rewound: 1, mode: 'unprecedented' });
		this.history = h;
		this.fen = entry.preFen;
		this.previewUci = null;
		void this.step(this.gen);
	}

	retry() {
		this.bump();
		void this.step(this.gen);
	}
}
