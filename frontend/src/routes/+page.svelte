<script lang="ts">
	import { onMount } from 'svelte';
	import Chessboard from '$lib/Chessboard.svelte';
	import PlayerPlate from '$lib/PlayerPlate.svelte';
	import GameVeil from '$lib/GameVeil.svelte';
	import RemainCounter from '$lib/RemainCounter.svelte';
	import StatePanel from '$lib/StatePanel.svelte';
	import Continuations from '$lib/Continuations.svelte';
	import MoveHistory from '$lib/MoveHistory.svelte';
	import { KnownGame } from '$lib/game.svelte';
	import { meta } from '$lib/api';

	const game = new KnownGame();

	let flipped = $state(false);
	let archivePositions = $state<number | null>(null);

	const topColor = $derived<'w' | 'b'>(flipped ? 'w' : 'b');
	const bottomColor = $derived<'w' | 'b'>(flipped ? 'b' : 'w');

	onMount(() => {
		game.newGame();
		meta()
			.then((m) => (archivePositions = m.positions))
			.catch(() => {});
	});
</script>

<svelte:head>
	<title>Known Chess — every move once lived</title>
	<meta name="description" content="Chess where only moves played in real games are legal. Narrow eight million games down to one." />
</svelte:head>

<div class="page">
	<header>
		<h1>Known<span class="amp">/</span>Chess</h1>
		<p class="lede">
			A move is legal only if someone, somewhere, actually played it. Every game here is drawn from real
			Lichess games that ended over the board — and as you play, the archive narrows. When one path
			remains, history finishes the game for you.
		</p>
		{#if archivePositions !== null}
			<p class="archive-size">{archivePositions.toLocaleString()} positions remembered</p>
		{/if}
	</header>

	<main>
		<section class="arena">
			<PlayerPlate {game} color={topColor} />
			<div class="board-frame">
				<Chessboard
					fen={game.fen}
					allowed={game.interactive ? game.known : []}
					interactive={game.interactive}
					{flipped}
					lastMove={game.lastMove}
					checkSquare={game.checkSquare}
					previewUci={game.previewUci}
					dimInactive={game.phase === 'loading' || game.phase === 'forced'}
					onMove={(uci) => game.chooseUci(uci)}
				/>
				<GameVeil {game} />
			</div>
			<PlayerPlate {game} color={bottomColor} />
		</section>

		<aside class="ledger">
			<RemainCounter total={game.total} />
			<StatePanel {game} />
			<Continuations {game} />
			<MoveHistory history={game.history} />

			<div class="controls">
				<button class="ctl" onclick={() => game.newGame()}>New game</button>
				<button
					class="ctl"
					onclick={() => game.takeback()}
					disabled={game.history.length === 0 || game.phase === 'loading' || game.phase === 'forced'}
				>
					Takeback
				</button>
				<button class="ctl" onclick={() => (flipped = !flipped)}>Flip</button>
				<button class="ctl" class:off={!game.sound} onclick={() => (game.sound = !game.sound)} aria-pressed={game.sound}>
					Sound {game.sound ? 'on' : 'off'}
				</button>
			</div>
		</aside>
	</main>

	<footer>
		<p>
			Moves are weighted by how often they were played; small print on each target square is how many
			games went that way. Autoplayed moves appear <span class="auto-demo">dimmed</span> in the ledger.
		</p>
	</footer>
</div>

<style>
	.page {
		max-width: 1180px;
		margin: 0 auto;
		padding: 2.5rem 1.5rem 3rem;
	}

	/* ——— header ——— */
	header {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.4rem 2.5rem;
		align-items: end;
		margin-bottom: 2.2rem;
	}
	h1 {
		grid-row: span 2;
		margin: 0;
		font-size: clamp(2.6rem, 6vw, 4.2rem);
		font-weight: 850;
		font-variation-settings: 'opsz' 144;
		letter-spacing: -0.03em;
		line-height: 0.95;
	}
	h1 .amp {
		color: var(--brass);
		font-style: italic;
		font-weight: 400;
		margin: 0 0.05em;
	}
	.lede {
		margin: 0;
		max-width: 58ch;
		color: var(--ink-dim);
		font-size: 0.98rem;
		line-height: 1.55;
		align-self: end;
	}
	.archive-size {
		margin: 0;
		font-family: var(--mono);
		font-size: 0.72rem;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--brass);
	}

	/* ——— layout ——— */
	main {
		display: grid;
		grid-template-columns: minmax(0, 1.35fr) minmax(300px, 1fr);
		gap: 2.2rem;
		align-items: start;
	}
	@media (max-width: 900px) {
		main {
			grid-template-columns: 1fr;
		}
		header {
			grid-template-columns: 1fr;
			align-items: start;
		}
		h1 {
			grid-row: auto;
		}
	}

	/* ——— arena ——— */
	.arena {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
	}
	.board-frame {
		position: relative;
	}

	/* ——— ledger ——— */
	.ledger {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		min-width: 0;
	}

	.controls {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	footer {
		margin-top: 2.5rem;
		border-top: 1px solid var(--panel-edge);
		padding-top: 1rem;
	}
	footer p {
		margin: 0;
		color: var(--ink-faint);
		font-size: 0.82rem;
		max-width: 72ch;
	}
	.auto-demo {
		font-style: italic;
		color: var(--ink-dim);
	}
</style>
