<script lang="ts">
	import type { SourcePlayer } from '$lib/api';
	import { fmtMonth } from '$lib/format';
	import type { KnownGame } from '$lib/game.svelte';
	import posthog from 'posthog-js';

	/** Overlay covering the board when the game is over, off the record, or errored. */
	let { game }: { game: KnownGame } = $props();

	const fmtName = (p: SourcePlayer) => p.name || 'Anonymous';
</script>

{#snippet crown()}
	<!-- Lucide "crown", inlined — no icon library in this project. -->
	<svg
		class="crown"
		role="img"
		aria-label="Winner"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="2"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path
			d="M11.562 3.266a.5.5 0 0 1 .876 0L15.39 8.87a1 1 0 0 0 1.516.294L21.183 5.5a.5.5 0 0 1 .798.519l-2.834 10.246a1 1 0 0 1-.956.734H5.81a1 1 0 0 1-.957-.734L2.02 6.02a.5.5 0 0 1 .798-.519l4.276 3.664a1 1 0 0 0 1.516-.294z"
		/>
		<path d="M5 21h14" />
	</svg>
{/snippet}

{#if game.phase === 'over' && game.result}
	<div class="veil">
		<p class="veil-title">{game.result.title}</p>
		<p class="veil-detail">{game.result.detail}</p>
		{#if game.lastEntry}
			<p class="veil-note">
				{game.lastEntry.count.toLocaleString()}
				{game.lastEntry.count === 1 ? 'game' : 'games'} ended exactly here.
			</p>
		{/if}

		{#if game.history.length}
			<div class="trace">
				{#if game.sourceState === 'searching'}
					<div class="card skeleton" aria-busy="true">
						<span class="kicker">
							<span class="spinner"></span> Tracing this game on Lichess…
						</span>
						<span class="sk-line w" style="width: 62%"></span>
						<span class="sk-line b" style="width: 48%"></span>
					</div>
				{:else if game.sourceState === 'found' && game.sourceGame}
					{@const src = game.sourceGame}
					<div class="card found">
						<span class="kicker">{src.exact ? 'You replayed this game' : 'Closest match on record'}</span>

						<span class="side" class:won={src.winner === 'white'}>
							<span class="disc white" aria-hidden="true"></span>
							<span class="pname">{fmtName(src.white)}</span>
							{#if src.white.rating}<span class="elo">{src.white.rating}</span>{/if}
							{#if src.winner === 'white'}{@render crown()}{/if}
						</span>
						<span class="side" class:won={src.winner === 'black'}>
							<span class="disc black" aria-hidden="true"></span>
							<span class="pname">{fmtName(src.black)}</span>
							{#if src.black.rating}<span class="elo">{src.black.rating}</span>{/if}
							{#if src.winner === 'black'}{@render crown()}{/if}
						</span>
						{#if src.winner === null}
							<span class="side draw"><span class="disc half" aria-hidden="true"></span>Drawn</span>
						{/if}

						<div class="cta">
							<span class="cta-meta">
								{src.speed ?? 'Game'}{src.month ? ` · ${fmtMonth(src.month)}` : ''}
							</span>
							<a
								class="cta-btn"
								href="https://lichess.org/{src.id}#{game.history.length}"
								target="_blank"
								rel="noopener"
								onclick={() => posthog.capture('lichess_link_opened', { lichess_id: src.id, exact_match: src.exact })}
							>
								Open on Lichess <span class="arr">↗</span>
							</a>
						</div>
					</div>
				{:else if game.sourceState === 'none'}
					<p class="trace-none">This exact game isn't indexed on Lichess.</p>
				{/if}
			</div>
		{/if}

		<button class="ctl primary" onclick={() => game.newGame()}>Play again</button>
	</div>
{:else if game.phase === 'dry'}
	<div class="veil">
		<p class="veil-title">Off the record.</p>
		<p class="veil-note">No recorded game ever continued from this position.</p>
		<div class="veil-actions">
			<button class="ctl" onclick={() => game.takeback()}>Take back</button>
			<button class="ctl primary" onclick={() => game.newGame()}>New game</button>
		</div>
	</div>
{:else if game.phase === 'error'}
	<div class="veil">
		<p class="veil-title">Lost the thread.</p>
		<p class="veil-note">The archive didn't answer. Check your connection and try again.</p>
		{#if game.errorMsg}
			<p class="veil-err">{game.errorMsg}</p>
		{/if}
		<button class="ctl primary" onclick={() => game.retry()}>Retry</button>
	</div>
{/if}

<style>
	.veil {
		position: absolute;
		inset: 0;
		z-index: 10;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.35rem;
		text-align: center;
		padding: 1rem;
		background: rgba(13, 17, 14, 0.86);
		backdrop-filter: blur(3px);
		border-radius: 8px;
		animation: veil-in 0.45s cubic-bezier(0.2, 0.9, 0.3, 1) backwards;
		animation-delay: 0.35s;
	}
	@keyframes veil-in {
		from {
			opacity: 0;
		}
	}
	.veil-title {
		margin: 0;
		font-size: clamp(2rem, 5vw, 3.1rem);
		font-weight: 850;
		font-style: italic;
		font-variation-settings: 'opsz' 144;
		letter-spacing: -0.02em;
		color: var(--brass-bright);
	}
	.veil-detail {
		margin: 0;
		font-size: 1.2rem;
		font-weight: 600;
	}
	.veil-note {
		margin: 0 0 0.6rem;
		color: var(--ink-dim);
		font-size: 0.92rem;
		max-width: 34ch;
	}
	.veil-err {
		margin: 0 0 0.6rem;
		font-family: var(--mono);
		font-size: 0.66rem;
		letter-spacing: 0.04em;
		color: var(--ink-faint);
		max-width: 40ch;
	}
	.veil-actions {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
		justify-content: center;
	}

	/* ——— lichess trace card ——— */
	.trace {
		width: min(20rem, 86%);
		margin: 0.1rem 0 1rem;
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 0.85rem 0.95rem 0.7rem;
		background: linear-gradient(180deg, rgba(37, 50, 43, 0.55), rgba(21, 30, 25, 0.55));
		border: 1px solid var(--panel-edge);
		border-radius: 12px;
		text-align: left;
		text-decoration: none;
		color: var(--ink);
	}
	.kicker {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		font-family: var(--mono);
		font-size: 0.62rem;
		font-weight: 500;
		letter-spacing: 0.16em;
		text-transform: uppercase;
		color: var(--brass);
	}

	/* sides */
	.side {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 1rem;
		font-weight: 600;
		color: var(--ink-dim);
	}
	.side.won {
		color: var(--ink);
	}
	.disc {
		width: 15px;
		height: 15px;
		border-radius: 50%;
		flex: none;
	}
	.disc.white {
		background: var(--sq-light);
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.3);
	}
	.disc.black {
		background: #1a1410;
		box-shadow: inset 0 0 0 1.5px var(--ink-faint);
	}
	.disc.half {
		background: linear-gradient(90deg, var(--sq-light) 50%, #1a1410 50%);
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.3);
	}
	.pname {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.elo {
		font-family: var(--mono);
		font-size: 0.72rem;
		color: var(--ink-faint);
	}
	/* Plain glyph, no pill — only the Lichess link should read as clickable. */
	.crown {
		margin-left: auto;
		width: 16px;
		height: 16px;
		flex: none;
		color: var(--brass-bright);
	}
	.side.draw {
		font-weight: 500;
		color: var(--ink-dim);
	}

	/* call to action */
	.cta {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.6rem;
		margin-top: 0.2rem;
		padding-top: 0.6rem;
		border-top: 1px solid var(--panel-edge);
	}
	.cta-meta {
		font-family: var(--mono);
		font-size: 0.66rem;
		letter-spacing: 0.06em;
		color: var(--ink-faint);
		text-transform: capitalize;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 0;
	}
	/* A quiet, secondary link — the brass "Play again" stays the lone primary action. */
	.cta-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-family: var(--mono);
		font-size: 0.66rem;
		font-weight: 600;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		text-decoration: none;
		color: var(--brass-bright);
		background: none;
		border: 1px solid var(--brass-soft);
		border-radius: 999px;
		padding: 0.36rem 0.7rem;
		flex: none;
		transition: background 0.15s, border-color 0.15s;
	}
	.cta-btn:hover {
		background: var(--brass-tint);
		border-color: var(--brass);
	}
	.cta-btn:focus-visible {
		outline: 2px solid var(--brass-bright);
		outline-offset: 2px;
	}
	.arr {
		font-size: 0.8rem;
		transition: transform 0.2s;
	}
	.cta-btn:hover .arr {
		transform: translate(2px, -2px);
	}

	/* The card itself is just a panel — names stay selectable, not a click target. */
	.card.found {
		animation: card-in 0.4s cubic-bezier(0.2, 0.9, 0.3, 1) backwards;
	}
	@keyframes card-in {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}

	/* searching skeleton */
	.spinner {
		width: 11px;
		height: 11px;
		border-radius: 50%;
		border: 1.5px solid var(--brass-soft);
		border-top-color: var(--brass-bright);
		animation: spin 0.7s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	.sk-line {
		height: 14px;
		border-radius: 5px;
		background: linear-gradient(90deg, var(--panel-edge) 25%, rgba(201, 160, 78, 0.12) 50%, var(--panel-edge) 75%);
		background-size: 200% 100%;
		animation: shimmer 1.3s ease-in-out infinite;
	}
	.sk-line.b {
		opacity: 0.7;
	}
	@keyframes shimmer {
		to {
			background-position: -200% 0;
		}
	}
	.trace-none {
		margin: 0;
		font-family: var(--mono);
		font-size: 0.68rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--ink-faint);
	}

	@media (prefers-reduced-motion: reduce) {
		.spinner,
		.sk-line {
			animation: none;
		}
		.card.found,
		.veil {
			animation: none;
		}
	}
</style>
