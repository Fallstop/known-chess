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
	<title>Precedent Chess: every move once lived</title>
	<meta name="description" content="Chess where every move needs a precedent. It counts only if it has been played in a real game before. Narrow millions of games down to one." />
</svelte:head>

<div class="page">
	<header>
		<h1>Precedent<span class="amp">/</span>Chess</h1>
		<p class="lede">
			In Precedent Chess, a move is legal only if it has precedent. Someone, somewhere, must have
			played it in a real game. Every game here comes from real Lichess games that ended over the
			board, and with each move the set of games matching yours gets smaller. When a single game is
			left, precedent plays it out to the end.
		</p>
		{#if archivePositions !== null}
			<p class="archive-size">{archivePositions.toLocaleString()} positions on record</p>
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
			<MoveHistory history={game.history} fen={game.fen} />

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
	/* ——— tablet & below: stack to one column ——— */
	@media (max-width: 900px) {
		main {
			grid-template-columns: 1fr;
			gap: 1.6rem;
			justify-items: center;
		}
		.arena,
		.ledger {
			width: 100%;
			max-width: 34rem;
		}
		/* Bound the board by viewport height too, so it never runs off the
		   bottom on short or landscape screens. */
		.arena {
			max-width: min(34rem, 82vh);
		}
		header {
			grid-template-columns: 1fr;
			align-items: start;
			margin-bottom: 1.8rem;
		}
		h1 {
			grid-row: auto;
		}
	}

	/* ——— phones ——— */
	@media (max-width: 560px) {
		.page {
			padding: 1.6rem 1rem 2.2rem;
		}
		header {
			gap: 0.5rem 0;
			margin-bottom: 1.4rem;
		}
		.lede {
			font-size: 0.92rem;
		}
		main {
			gap: 1.2rem;
		}
		.arena {
			gap: 0.5rem;
		}
		footer {
			margin-top: 1.8rem;
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
