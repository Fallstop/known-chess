<script lang="ts">
	import { Chess, type Square } from 'chess.js';

	interface Props {
		/** Current position as FEN. */
		fen: string;
		/** UCI strings of the moves the player is allowed to make right now. */
		allowed: string[];
		/** Whether the board accepts input (false while the engine/auto-play moves). */
		interactive?: boolean;
		/** Called with the chosen move in UCI when the player completes a move. */
		onMove: (uci: string) => void;
	}

	let { fen, allowed, interactive = true, onMove }: Props = $props();

	const FILES = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
	const RANKS = [8, 7, 6, 5, 4, 3, 2, 1];

	const GLYPHS: Record<string, string> = {
		wp: '♙', wn: '♘', wb: '♗', wr: '♖', wq: '♕', wk: '♔',
		bp: '♟', bn: '♞', bb: '♝', br: '♜', bq: '♛', bk: '♚'
	};

	const game = $derived(new Chess(fen));
	let selected = $state<Square | null>(null);

	// Map of from-square -> { toSquare -> uci } for the allowed moves only.
	const allowedBySquare = $derived.by(() => {
		const map = new Map<string, Map<string, string>>();
		for (const uci of allowed) {
			const from = uci.slice(0, 2);
			const to = uci.slice(2, 4);
			if (!map.has(from)) map.set(from, new Map());
			map.get(from)!.set(to, uci);
		}
		return map;
	});

	function pieceAt(sq: string): string | null {
		const p = game.get(sq as Square);
		return p ? GLYPHS[p.color + p.type] : null;
	}

	function handleClick(sq: Square) {
		if (!interactive) return;

		// Completing a move onto a highlighted target.
		if (selected) {
			const uci = allowedBySquare.get(selected)?.get(sq);
			if (uci) {
				onMove(uci);
				selected = null;
				return;
			}
		}

		// Otherwise (re)select a square that has at least one allowed move.
		selected = allowedBySquare.has(sq) ? sq : null;
	}

	function isTarget(sq: string): boolean {
		return selected ? !!allowedBySquare.get(selected)?.has(sq) : false;
	}
</script>

<div class="board" class:locked={!interactive}>
	{#each RANKS as rank}
		{#each FILES as file}
			{@const sq = `${file}${rank}` as Square}
			{@const dark = (FILES.indexOf(file) + rank) % 2 === 0}
			<button
				class="sq"
				class:dark
				class:light={!dark}
				class:selected={selected === sq}
				class:target={isTarget(sq)}
				class:movable={allowedBySquare.has(sq)}
				onclick={() => handleClick(sq)}
				aria-label={sq}
			>
				{#if pieceAt(sq)}<span class="piece">{pieceAt(sq)}</span>{/if}
				{#if isTarget(sq)}<span class="dot"></span>{/if}
			</button>
		{/each}
	{/each}
</div>

<style>
	.board {
		display: grid;
		grid-template-columns: repeat(8, 1fr);
		width: min(80vw, 560px);
		aspect-ratio: 1;
		border: 3px solid #2a2118;
		border-radius: 4px;
		overflow: hidden;
		user-select: none;
	}
	.board.locked {
		cursor: progress;
	}
	.sq {
		position: relative;
		border: 0;
		padding: 0;
		font-size: clamp(1.5rem, 7vw, 3rem);
		line-height: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: default;
	}
	.sq.light {
		background: #ebecd0;
	}
	.sq.dark {
		background: #739552;
	}
	.sq.movable {
		cursor: pointer;
	}
	.sq.selected {
		background: #f6f669;
	}
	.sq.target {
		cursor: pointer;
	}
	.piece {
		z-index: 1;
		filter: drop-shadow(0 1px 1px rgba(0, 0, 0, 0.35));
	}
	.dot {
		position: absolute;
		width: 28%;
		height: 28%;
		border-radius: 50%;
		background: rgba(0, 0, 0, 0.2);
	}
</style>
