<script lang="ts">
	import type { KnownGame } from '$lib/game.svelte';

	/** One-line phase readout: whose move, forced lines, lookups, the end. */
	let { game }: { game: KnownGame } = $props();
</script>

<div class="state" class:forced={game.phase === 'forced'}>
	{#if game.phase === 'loading'}
		<span class="state-main dim">Searching for precedent<span class="ellip"></span></span>
	{:else if game.phase === 'forced'}
		<span class="state-main">Locked in</span>
		<span class="state-sub">One precedent remains, so the line plays itself out.</span>
	{:else if game.phase === 'choose'}
		<span class="state-main">{game.toMoveName} to move</span>
		<span class="state-sub">{game.known.length} precedents diverge here</span>
	{:else if game.phase === 'over' && game.result}
		<span class="state-main">{game.result.title.replace(/\.$/, '')}</span>
		<span class="state-sub">{game.result.detail}</span>
	{:else if game.phase === 'dry'}
		<span class="state-main">Off the record</span>
	{:else}
		<span class="state-main dim">—</span>
	{/if}
</div>

<style>
	.state {
		padding: 0.7rem 1.2rem;
		background: var(--panel);
		border: 1px solid var(--panel-edge);
		border-radius: 10px;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 0.15rem;
		/* room for two lines in every phase, so one-liners don't shrink the box */
		min-height: 3.3rem;
		transition: border-color 0.3s;
	}
	.state.forced {
		border-color: var(--brass-soft);
		animation: forced-pulse 1.6s ease-in-out infinite;
	}
	@keyframes forced-pulse {
		50% {
			box-shadow: 0 0 22px -6px var(--brass-glow);
		}
	}
	.state-main {
		font-size: 1.15rem;
		font-weight: 700;
	}
	.state-main.dim {
		color: var(--ink-dim);
		font-weight: 500;
	}
	.state-sub {
		color: var(--ink-dim);
		font-size: 0.85rem;
	}
	.ellip::after {
		content: '…';
		display: inline-block;
		animation: ellip 1.2s steps(4) infinite;
		clip-path: inset(0 100% 0 0);
	}
	@keyframes ellip {
		to {
			clip-path: inset(0 -0.2em 0 0);
		}
	}
</style>
