<script lang="ts">
	import { onMount } from 'svelte';
	import Chessboard from '$lib/Chessboard.svelte';
	import PlayerPlate from '$lib/PlayerPlate.svelte';
	import GameVeil from '$lib/GameVeil.svelte';
	import RemainCounter from '$lib/RemainCounter.svelte';
	import StatePanel from '$lib/StatePanel.svelte';
	import Continuations from '$lib/Continuations.svelte';
	import MoveHistory from '$lib/MoveHistory.svelte';
	import { KnownGame, type GameView } from '$lib/game.svelte';
	import { UnprecedentedGame } from '$lib/unprecedented.svelte';
	import { MODE, copy } from '$lib/mode';
	import { meta } from '$lib/api';

	const game: GameView = MODE === 'unprecedented' ? new UnprecedentedGame() : new KnownGame();

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
	<title>{copy.title}</title>
	<meta name="description" content={copy.description} />
	<link rel="canonical" href={copy.canonical} />

	<meta property="og:type" content="website" />
	<meta property="og:site_name" content="{copy.brandA}{copy.brandB}" />
	<meta property="og:title" content={copy.title} />
	<meta property="og:description" content={copy.description} />
	<meta property="og:url" content={copy.canonical} />
	<meta property="og:image" content={copy.ogImage} />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="og:image:alt" content={copy.title} />

	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:title" content={copy.title} />
	<meta name="twitter:description" content={copy.description} />
	<meta name="twitter:image" content={copy.ogImage} />

	{@html `<script type="application/ld+json">${JSON.stringify({
		'@context': 'https://schema.org',
		'@type': 'WebApplication',
		name: `${copy.brandA}${copy.brandB}`,
		url: copy.canonical,
		description: copy.description,
		applicationCategory: 'GameApplication',
		genre: 'Chess',
		operatingSystem: 'Web',
		browserRequirements: 'Requires JavaScript',
		offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
		image: copy.ogImage,
		author: { '@type': 'Person', name: 'Jasper M-W', url: 'https://jmw.nz' },
		isBasedOn: 'https://database.lichess.org/'
	})}</scr${''}ipt>`}
</svelte:head>

<div class="page">
	<header>
		<h1>{copy.brandA}<span class="amp">/</span>{copy.brandB}</h1>
		<p class="lede">{copy.lede}</p>
		{#if archivePositions !== null}
			<p class="archive-size">{archivePositions.toLocaleString()} positions on record</p>
		{/if}
	</header>

	<section class="how" aria-label="How it works">
		{#each copy.steps as step, i (i)}
			<div class="how-step">
				<span class="how-num" aria-hidden="true">0{i + 1}</span>
				<h2>{step.title}</h2>
				<p>{step.body}</p>
			</div>
		{/each}
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
					showCounts={copy.showCounts}
					onMove={(uci) => game.chooseUci(uci)}
					onPeek={(uci) => game.warm(uci)}
				/>
				<GameVeil {game} />
			</div>
			<PlayerPlate {game} color={bottomColor} />
		</section>

		<aside class="ledger" aria-label="Game ledger">
			{#if copy.showRemainCounter}
				<RemainCounter total={game.total} />
			{/if}
			<StatePanel {game} />
			{#if copy.showContinuations}
				<Continuations {game} />
			{/if}
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
		<p class="foot-note">{copy.footNote}</p>
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
