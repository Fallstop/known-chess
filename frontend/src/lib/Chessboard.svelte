<script lang="ts">
	import { Chess } from 'chess.js';
	import { untrack } from 'svelte';
	import type { KnownMove } from '$lib/api';
	import { fmtCompact } from '$lib/format';

	interface Props {
		/** Current position as FEN. */
		fen: string;
		/** Moves the player may make right now (empty while locked). */
		allowed?: KnownMove[];
		/** Whether the board accepts input. */
		interactive?: boolean;
		/** Render from Black's point of view. */
		flipped?: boolean;
		/** The move that produced this position, for the from/to tint. */
		lastMove?: { from: string; to: string } | null;
		/** Square of a king currently in check, if any. */
		checkSquare?: string | null;
		/** A move (UCI) to spotlight, e.g. while hovering the continuations list. */
		previewUci?: string | null;
		/** Called with the chosen move in UCI when the player completes a move. */
		onMove: (uci: string) => void;
	}

	let {
		fen,
		allowed = [],
		interactive = false,
		flipped = false,
		lastMove = null,
		checkSquare = null,
		previewUci = null,
		onMove
	}: Props = $props();

	const FILES = 'abcdefgh';

	interface Cell {
		sq: string;
		f: number;
		r: number;
		dark: boolean;
	}
	const CELLS: Cell[] = [];
	for (let r = 0; r < 8; r++) {
		for (let f = 0; f < 8; f++) {
			CELLS.push({ sq: FILES[f] + (r + 1), f, r, dark: (f + r) % 2 === 0 });
		}
	}

	const fileOf = (sq: string) => sq.charCodeAt(0) - 97;
	const rankOf = (sq: string) => sq.charCodeAt(1) - 49;
	const xOf = (f: number) => (flipped ? 7 - f : f);
	const yOf = (r: number) => (flipped ? r : 7 - r);

	// ——— Piece tracking ————————————————————————————————————————————————
	// Pieces carry stable ids across FEN changes so they glide between squares.
	// On each new FEN we match old pieces to new squares: exact square first,
	// then nearest-of-same-kind (which handles moves, castling, en passant).
	// Leftovers fade out as ghosts; brand-new squares pop a fresh piece in.

	interface Piece {
		id: number;
		color: 'w' | 'b';
		type: string;
		square: string;
		fresh: boolean;
		delay: number;
	}
	let pieces = $state<Piece[]>([]);
	let ghosts = $state<Piece[]>([]);
	let nextId = 1;

	function boardMap(f: string): Map<string, { color: 'w' | 'b'; type: string }> {
		const m = new Map<string, { color: 'w' | 'b'; type: string }>();
		for (const row of new Chess(f).board()) {
			for (const p of row) {
				if (p) m.set(p.square, { color: p.color, type: p.type });
			}
		}
		return m;
	}

	function dist(a: string, b: string): number {
		const df = a.charCodeAt(0) - b.charCodeAt(0);
		const dr = a.charCodeAt(1) - b.charCodeAt(1);
		return df * df + dr * dr;
	}

	function sync(target: Map<string, { color: 'w' | 'b'; type: string }>) {
		const setup = pieces.length === 0;
		const remaining = new Map(target);
		const keep: Piece[] = [];
		const homeless: Piece[] = [];

		for (const p of pieces) {
			const t = remaining.get(p.square);
			if (t && t.color === p.color && t.type === p.type) {
				keep.push({ ...p, fresh: false, delay: 0 });
				remaining.delete(p.square);
			} else {
				homeless.push(p);
			}
		}

		const pairs: { p: Piece; sq: string; d: number }[] = [];
		for (const p of homeless) {
			for (const [sq, t] of remaining) {
				if (t.color === p.color && t.type === p.type) pairs.push({ p, sq, d: dist(p.square, sq) });
			}
		}
		pairs.sort((a, b) => a.d - b.d);
		const usedP = new Set<number>();
		const usedSq = new Set<string>();
		for (const { p, sq } of pairs) {
			if (usedP.has(p.id) || usedSq.has(sq)) continue;
			usedP.add(p.id);
			usedSq.add(sq);
			remaining.delete(sq);
			keep.push({ ...p, square: sq, fresh: false, delay: 0 });
		}

		for (const p of homeless) {
			if (usedP.has(p.id)) continue;
			const g = { ...p };
			ghosts = [...ghosts, g];
			setTimeout(() => (ghosts = ghosts.filter((x) => x.id !== g.id)), 350);
		}

		for (const [sq, t] of remaining) {
			keep.push({
				id: nextId++,
				color: t.color,
				type: t.type,
				square: sq,
				fresh: true,
				delay: setup ? (fileOf(sq) + rankOf(sq)) * 35 : 0
			});
		}

		pieces = keep;
	}

	$effect(() => {
		const m = boardMap(fen);
		untrack(() => sync(m));
	});

	const occupied = $derived(new Set(pieces.map((p) => p.square)));
	const src = (p: { color: string; type: string }) => `/pieces/${p.color}${p.type.toUpperCase()}.svg`;

	// ——— Selection & targets ————————————————————————————————————————————

	let selected = $state<string | null>(null);
	let hovered = $state<string | null>(null);
	let promo = $state<{ from: string; to: string; options: KnownMove[] } | null>(null);

	// from-square -> (to-square -> the allowed moves there; >1 means promotion choice)
	const byFrom = $derived.by(() => {
		const m = new Map<string, Map<string, KnownMove[]>>();
		for (const mv of allowed) {
			const from = mv.uci.slice(0, 2);
			const to = mv.uci.slice(2, 4);
			if (!m.has(from)) m.set(from, new Map());
			const tos = m.get(from)!;
			if (!tos.has(to)) tos.set(to, []);
			tos.get(to)!.push(mv);
		}
		return m;
	});

	const targets = $derived(
		selected ? (byFrom.get(selected) ?? new Map<string, KnownMove[]>()) : new Map<string, KnownMove[]>()
	);

	// A new move set means a move was played (or the game reset) — drop stale UI.
	$effect(() => {
		void allowed;
		selected = null;
		promo = null;
	});

	const previewFrom = $derived(previewUci ? previewUci.slice(0, 2) : null);
	const previewTo = $derived(previewUci ? previewUci.slice(2, 4) : null);

	function clickSquare(sq: string) {
		if (!interactive) return;
		promo = null;
		if (selected && targets.has(sq)) {
			const opts = targets.get(sq)!;
			if (opts.length === 1) {
				onMove(opts[0].uci);
				selected = null;
			} else {
				promo = { from: selected, to: sq, options: [...opts].sort((a, b) => b.count - a.count) };
			}
			return;
		}
		selected = byFrom.has(sq) && sq !== selected ? sq : null;
	}

	const sumCount = (opts: KnownMove[]) => opts.reduce((s, o) => s + o.count, 0);
</script>

<div class="board" class:locked={!interactive}>
	<!-- squares -->
	{#each CELLS as c (c.sq)}
		<div
			class="sq"
			class:dark={c.dark}
			class:sel={selected === c.sq}
			class:last={lastMove !== null && (lastMove.from === c.sq || lastMove.to === c.sq)}
			class:preview={previewFrom === c.sq || previewTo === c.sq}
			class:check={checkSquare === c.sq}
			class:movable={interactive && byFrom.has(c.sq)}
			class:hot={hovered === c.sq && interactive && byFrom.has(c.sq)}
			style="--x:{xOf(c.f)};--y:{yOf(c.r)}"
		>
			{#if yOf(c.r) === 7}<span class="coord file">{FILES[c.f]}</span>{/if}
			{#if xOf(c.f) === 0}<span class="coord rank">{c.r + 1}</span>{/if}
		</div>
	{/each}

	<!-- pieces -->
	{#each pieces as p (p.id)}
		<div
			class="piece"
			class:fresh={p.fresh}
			class:lift={hovered === p.square && interactive && byFrom.has(p.square)}
			style="--x:{xOf(fileOf(p.square))};--y:{yOf(rankOf(p.square))};--d:{p.delay}ms"
		>
			<img src={src(p)} alt="" draggable="false" />
		</div>
	{/each}
	{#each ghosts as g (g.id)}
		<div class="piece ghost" style="--x:{xOf(fileOf(g.square))};--y:{yOf(rankOf(g.square))}">
			<img src={src(g)} alt="" draggable="false" />
		</div>
	{/each}

	<!-- move targets -->
	{#each [...targets] as [sq, opts] (sq)}
		<div class="mark" class:ring={occupied.has(sq)} style="--x:{xOf(fileOf(sq))};--y:{yOf(rankOf(sq))}">
			{#if !occupied.has(sq)}<span class="dot"></span>{/if}
			<span class="badge">{fmtCompact(sumCount(opts))}</span>
		</div>
	{/each}

	<!-- hit layer -->
	{#each CELLS as c (c.sq)}
		<button
			class="hit"
			class:pointer={interactive && (byFrom.has(c.sq) || targets.has(c.sq))}
			style="--x:{xOf(c.f)};--y:{yOf(c.r)}"
			aria-label={c.sq}
			onclick={() => clickSquare(c.sq)}
			onmouseenter={() => (hovered = c.sq)}
			onmouseleave={() => (hovered = null)}
		></button>
	{/each}

	<!-- promotion picker -->
	{#if promo}
		<button class="promo-veil" aria-label="Cancel promotion" onclick={() => (promo = null)}></button>
		<div
			class="promo"
			class:from-bottom={yOf(rankOf(promo.to)) !== 0}
			style="--x:{xOf(fileOf(promo.to))}"
		>
			{#each promo.options as opt (opt.uci)}
				<button
					class="promo-opt"
					onclick={() => {
						onMove(opt.uci);
						promo = null;
						selected = null;
					}}
				>
					<img src={`/pieces/${opt.uci.slice(0, 2)[1] === '7' ? 'w' : 'b'}${opt.uci.slice(4).toUpperCase()}.svg`} alt={opt.san} draggable="false" />
					<span>{fmtCompact(opt.count)}</span>
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.board {
		position: relative;
		width: 100%;
		aspect-ratio: 1;
		user-select: none;
		border-radius: 8px;
		overflow: hidden;
		box-shadow:
			0 1px 0 rgba(255, 255, 255, 0.06) inset,
			0 24px 60px -18px rgba(0, 0, 0, 0.65),
			0 0 0 1px rgba(0, 0, 0, 0.5);
	}
	.board.locked {
		cursor: default;
	}

	.sq,
	.piece,
	.mark,
	.hit {
		position: absolute;
		top: 0;
		left: 0;
		width: 12.5%;
		height: 12.5%;
		transform: translate(calc(var(--x) * 100%), calc(var(--y) * 100%));
	}

	/* ——— squares ——— */
	.sq {
		background: var(--sq-light);
	}
	.sq.dark {
		background: var(--sq-dark);
	}
	.sq::before {
		content: '';
		position: absolute;
		inset: 0;
		pointer-events: none;
	}
	.sq.last::before {
		background: var(--brass-tint-strong);
	}
	.sq.movable::after {
		content: '';
		position: absolute;
		inset: 0;
		background: radial-gradient(circle at 50% 58%, var(--brass-glow) 0%, transparent 62%);
		opacity: 0.55;
		transition: opacity 0.15s;
	}
	.sq.hot::after {
		opacity: 1;
	}
	.sq.sel::before {
		background: var(--brass-tint-strong);
		box-shadow: inset 0 0 0 3px var(--brass);
	}
	.sq.preview::before {
		box-shadow: inset 0 0 0 3px var(--brass);
		background: var(--brass-tint);
	}
	.sq.check::before {
		background: radial-gradient(circle, rgba(190, 49, 38, 0.75) 0%, rgba(190, 49, 38, 0.22) 55%, transparent 75%);
	}

	.coord {
		position: absolute;
		font-family: var(--mono);
		font-size: clamp(8px, 1.4vw, 11px);
		font-weight: 600;
		opacity: 0.65;
		pointer-events: none;
	}
	.coord.file {
		right: 5%;
		bottom: 3%;
	}
	.coord.rank {
		left: 5%;
		top: 4%;
	}
	.sq .coord {
		color: var(--sq-dark);
	}
	.sq.dark .coord {
		color: var(--sq-light);
	}

	/* ——— pieces ——— */
	.piece {
		z-index: 2;
		pointer-events: none;
		transition: transform 0.28s cubic-bezier(0.2, 0.85, 0.3, 1);
	}
	.piece img {
		width: 100%;
		height: 100%;
		display: block;
		filter: drop-shadow(0 2px 2px rgba(20, 12, 5, 0.4));
		transition: transform 0.15s ease;
	}
	.piece.lift img {
		transform: translateY(-4%) scale(1.05);
	}
	.piece.fresh img {
		animation: pop 0.32s cubic-bezier(0.2, 0.9, 0.35, 1.3) backwards;
		animation-delay: var(--d, 0ms);
	}
	@keyframes pop {
		from {
			opacity: 0;
			transform: scale(0.35);
		}
	}
	.piece.ghost {
		z-index: 1;
		animation: ghost-fade 0.3s forwards;
	}
	@keyframes ghost-fade {
		to {
			opacity: 0;
		}
	}
	.piece.ghost img {
		animation: ghost-shrink 0.3s forwards;
	}
	@keyframes ghost-shrink {
		to {
			transform: scale(0.65);
		}
	}

	/* ——— targets ——— */
	.mark {
		z-index: 3;
		pointer-events: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.mark .dot {
		width: 30%;
		height: 30%;
		border-radius: 50%;
		background: rgba(15, 12, 6, 0.28);
		box-shadow: inset 0 0 0 2px var(--brass-soft);
	}
	.mark.ring::before {
		content: '';
		position: absolute;
		inset: 3%;
		border-radius: 50%;
		border: 3px solid var(--brass);
		opacity: 0.85;
	}
	.badge {
		position: absolute;
		top: 4%;
		right: 4%;
		font-family: var(--mono);
		font-size: clamp(8px, 1.5vw, 11px);
		font-weight: 600;
		line-height: 1;
		color: var(--brass-bright);
		background: rgba(12, 16, 13, 0.85);
		border: 1px solid rgba(201, 160, 78, 0.35);
		padding: 2px 5px;
		border-radius: 999px;
		letter-spacing: 0.02em;
	}

	/* ——— hits ——— */
	.hit {
		z-index: 4;
		background: none;
		border: 0;
		padding: 0;
		appearance: none;
		cursor: default;
	}
	.hit.pointer {
		cursor: pointer;
	}
	.hit:focus-visible {
		outline: 2px solid var(--brass);
		outline-offset: -2px;
	}

	/* ——— promotion ——— */
	.promo-veil {
		position: absolute;
		inset: 0;
		z-index: 5;
		background: rgba(10, 12, 10, 0.55);
		border: 0;
		padding: 0;
		appearance: none;
		cursor: pointer;
	}
	.promo {
		position: absolute;
		z-index: 6;
		left: calc(var(--x) * 12.5%);
		top: 0;
		width: 12.5%;
		display: flex;
		flex-direction: column;
		background: var(--panel);
		border: 1px solid var(--brass-soft);
		border-radius: 6px;
		overflow: hidden;
		box-shadow: 0 12px 32px rgba(0, 0, 0, 0.6);
	}
	.promo.from-bottom {
		top: auto;
		bottom: 0;
		flex-direction: column-reverse;
	}
	.promo-opt {
		background: none;
		border: 0;
		padding: 6% 8% 10%;
		cursor: pointer;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 2px;
	}
	.promo-opt:hover {
		background: var(--brass-tint);
	}
	.promo-opt img {
		width: 100%;
		display: block;
	}
	.promo-opt span {
		font-family: var(--mono);
		font-size: 10px;
		color: var(--ink-dim);
	}
</style>
