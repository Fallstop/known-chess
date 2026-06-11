<script lang="ts">
	import type { KnownGame } from '$lib/game.svelte';

	/** Overlay covering the board when the game is over, off the record, or errored. */
	let { game }: { game: KnownGame } = $props();
</script>

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
		<button class="ctl primary" onclick={() => game.newGame()}>Play again</button>
	</div>
{:else if game.phase === 'dry'}
	<div class="veil">
		<p class="veil-title">The archive runs dry.</p>
		<p class="veil-note">No recorded game continued from this position.</p>
		<button class="ctl primary" onclick={() => game.newGame()}>Play again</button>
	</div>
{:else if game.phase === 'error'}
	<div class="veil">
		<p class="veil-title">Lost the thread.</p>
		<p class="veil-note">{game.errorMsg}</p>
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
		background: rgba(13, 17, 14, 0.82);
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
		margin: 0 0 0.8rem;
		color: var(--ink-dim);
		font-size: 0.92rem;
		max-width: 34ch;
	}
</style>
