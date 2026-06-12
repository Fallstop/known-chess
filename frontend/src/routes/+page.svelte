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
	<meta name="description" content="Chess where every move needs a precedent: it counts only if it was played from your exact position in a real game. Play until a single game remains, and watch its real ending." />
	<link rel="canonical" href="https://precedent.jmw.nz/" />

	<meta property="og:type" content="website" />
	<meta property="og:site_name" content="Precedent Chess" />
	<meta property="og:title" content="Precedent Chess: every move once lived" />
	<meta property="og:description" content="Chess where every move needs a precedent: it counts only if it was played from your exact position in a real game. Play until a single game remains, and watch its real ending." />
	<meta property="og:url" content="https://precedent.jmw.nz/" />
	<meta property="og:image" content="https://precedent.jmw.nz/og.png" />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="og:image:alt" content="Precedent Chess: a chessboard where each square shows how many real games played that move" />

	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:title" content="Precedent Chess: every move once lived" />
	<meta name="twitter:description" content="Chess where every move needs a precedent: it counts only if it was played from your exact position in a real game." />
	<meta name="twitter:image" content="https://precedent.jmw.nz/og.png" />

	{@html `<script type="application/ld+json">${JSON.stringify({
		'@context': 'https://schema.org',
		'@type': 'WebApplication',
		name: 'Precedent Chess',
		url: 'https://precedent.jmw.nz/',
		description:
			'Chess where every move needs a precedent: it counts only if it was played from your exact position in a real Lichess game. Play until a single game remains, and watch its real ending.',
		applicationCategory: 'GameApplication',
		genre: 'Chess',
		operatingSystem: 'Web',
		browserRequirements: 'Requires JavaScript',
		offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
		image: 'https://precedent.jmw.nz/og.png',
		author: { '@type': 'Person', name: 'Jasper M-W', url: 'https://jmw.nz' },
		isBasedOn: 'https://database.lichess.org/'
	})}</scr${''}ipt>`}
</svelte:head>

<div class="page">
	<header>
		<h1>Precedent<span class="amp">/</span>Chess</h1>
		<p class="lede">
			Chess where a move is legal only if it has precedent: someone, somewhere, played it from
			this exact position in a real Lichess game. The deeper you go, the fewer games ever got
			there, until a single game remains and plays out its real ending.
		</p>
		{#if archivePositions !== null}
			<p class="archive-size">{archivePositions.toLocaleString()} positions on record</p>
		{/if}
	</header>

	<section class="how" aria-label="How it works">
		<div class="how-step">
			<span class="how-num" aria-hidden="true">01</span>
			<h2>Pick a move</h2>
			<p>Only moves actually played from this position are open to you. The small count on each square is how many games chose it.</p>
		</div>
		<div class="how-step">
			<span class="how-num" aria-hidden="true">02</span>
			<h2>Positions get rarer</h2>
			<p>Each position is looked up in the record on its own, whatever the move order. The deeper you go, the fewer real games ever reached it.</p>
		</div>
		<div class="how-step">
			<span class="how-num" aria-hidden="true">03</span>
			<h2>One game remains</h2>
			<p>Once a single game ever reached your position, precedent takes over and plays out its real ending, which you can open on Lichess.</p>
		</div>
	</section>

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
					onPeek={(uci) => game.warm(uci)}
				/>
				<GameVeil {game} />
			</div>
			<PlayerPlate {game} color={bottomColor} />
		</section>

		<aside class="ledger" aria-label="Game ledger">
			<RemainCounter total={game.total} />
			<StatePanel {game} />
			<Continuations {game} />
			<MoveHistory history={game.history} fen={game.fen} />

			<div class="controls" role="group" aria-label="Game controls">
				<button class="ctl" onclick={() => game.newGame()}>New game</button>
				<button
					class="ctl"
					onclick={() => game.takeback()}
					disabled={game.history.length === 0 || game.phase === 'loading' || game.phase === 'forced'}
					title="Rewind to your last real choice"
				>
					Take back
				</button>
				<button class="ctl" onclick={() => (flipped = !flipped)} title="View the board from the other side">
					Flip board
				</button>
				<button class="ctl" class:off={!game.sound} onclick={() => (game.sound = !game.sound)} aria-pressed={game.sound}>
					Sound {game.sound ? 'on' : 'off'}
				</button>
			</div>
		</aside>
	</main>

	<footer>
		<p class="foot-note">
			Every count is real games that reached your exact position. Moves played for you by
			precedent appear <span class="auto-demo">dimmed</span>.
		</p>
		<nav class="colophon" aria-label="Credits">
			<span class="colophon-item">
				<a href="https://database.lichess.org/" target="_blank" rel="noopener">Lichess open database</a>
				<a class="lic" href="https://creativecommons.org/publicdomain/zero/1.0/" target="_blank" rel="noopener">CC0</a>
			</span>
			<span class="dot" aria-hidden="true">·</span>
			<span class="colophon-item">
				<a href="https://jmw.nz" target="_blank" rel="noopener">Jasper M-W</a>
			</span>
			<span class="dot" aria-hidden="true">·</span>
			<span class="colophon-item">
				<a href="https://github.com/Fallstop/known-chess" target="_blank" rel="noopener">Source</a>
				<a class="lic" href="https://github.com/Fallstop/known-chess/blob/main/LICENSE" target="_blank" rel="noopener">MIT</a>
			</span>
		</nav>
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

	/* ——— how it works ——— */
	.how {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 1.6rem;
		margin-bottom: 2.2rem;
		padding: 1.1rem 0 1.3rem;
		border-top: 1px solid var(--panel-edge);
		border-bottom: 1px solid var(--panel-edge);
	}
	.how-step {
		display: grid;
		grid-template-columns: auto 1fr;
		grid-template-rows: auto auto;
		column-gap: 0.7rem;
		row-gap: 0.25rem;
		align-items: baseline;
	}
	.how-num {
		grid-row: span 2;
		align-self: start;
		font-family: var(--mono);
		font-size: 0.78rem;
		font-weight: 600;
		letter-spacing: 0.08em;
		color: var(--brass);
		border: 1px solid var(--brass-soft);
		border-radius: 999px;
		padding: 0.28rem 0.45rem;
		line-height: 1;
	}
	.how-step h2 {
		margin: 0;
		font-size: 1.02rem;
		font-weight: 700;
		letter-spacing: -0.01em;
	}
	.how-step p {
		margin: 0;
		color: var(--ink-dim);
		font-size: 0.85rem;
		line-height: 1.5;
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
			margin-bottom: 1.4rem;
		}
		h1 {
			grid-row: auto;
		}
		.how {
			grid-template-columns: 1fr;
			gap: 1rem;
			margin-bottom: 1.6rem;
		}
	}

	/* ——— phones ——— */
	@media (max-width: 560px) {
		.page {
			padding: 1.6rem 1rem 2.2rem;
		}
		header {
			gap: 0.5rem 0;
			margin-bottom: 1.2rem;
		}
		.lede {
			font-size: 0.92rem;
		}
		.how {
			padding: 0.9rem 0 1rem;
			margin-bottom: 1.2rem;
		}
		.how-step p {
			font-size: 0.82rem;
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
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: baseline;
		gap: 0.8rem 3rem;
		margin-top: 2.5rem;
		border-top: 1px solid var(--panel-edge);
		padding-top: 1.1rem;
	}
	.foot-note {
		margin: 0;
		color: var(--ink-faint);
		font-size: 0.82rem;
		line-height: 1.5;
		max-width: 52ch;
	}
	.auto-demo {
		font-style: italic;
		color: var(--ink-dim);
	}
	.colophon {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.3rem 0.7rem;
		margin-left: auto;
		font-family: var(--mono);
		font-size: 0.7rem;
		letter-spacing: 0.05em;
		white-space: nowrap;
	}
	.colophon-item {
		display: inline-flex;
		align-items: baseline;
		gap: 0.4rem;
	}
	.colophon .dot {
		color: var(--ink-faint);
	}
	.colophon a {
		color: var(--ink-dim);
		text-decoration: none;
		transition: color 0.15s;
	}
	.colophon a:hover {
		color: var(--brass-bright);
	}
	.colophon a:focus-visible {
		outline: 2px solid var(--brass-bright);
		outline-offset: 2px;
		border-radius: 2px;
	}
	/* License tags: tiny pills instead of parenthesized links. */
	.colophon a.lic {
		font-size: 0.6rem;
		letter-spacing: 0.08em;
		color: var(--ink-faint);
		border: 1px solid var(--panel-edge);
		border-radius: 999px;
		padding: 0.1rem 0.38rem;
		line-height: 1.3;
		transition: color 0.15s, border-color 0.15s;
	}
	.colophon a.lic:hover {
		color: var(--brass-bright);
		border-color: var(--brass-soft);
	}
	/* Stacked under the note on phones. */
	@media (max-width: 560px) {
		.colophon {
			margin-left: 0;
		}
	}
</style>
