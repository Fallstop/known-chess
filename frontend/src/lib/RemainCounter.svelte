<script lang="ts">
	import { untrack } from 'svelte';
	import Odometer from '$lib/Odometer.svelte';
	import { fmtCompact } from '$lib/format';

	/** The hero "games remain" counter with its narrowing-delta chip. */
	let { total }: { total: number } = $props();

	let delta = $state<{ amount: number; key: number } | null>(null);
	let prevTotal = -1;
	$effect(() => {
		const t = total;
		untrack(() => {
			if (prevTotal > 0 && t > 0 && t < prevTotal) {
				delta = { amount: prevTotal - t, key: (delta?.key ?? 0) + 1 };
			}
			if (t > 0) prevTotal = t;
		});
	});
</script>

<div class="counter">
	<span class="label">games reached this position</span>
	<div class="counter-row">
		<Odometer value={total} />
		{#if delta}
			{#key delta.key}
				<span class="delta">−{fmtCompact(delta.amount)}</span>
			{/key}
		{/if}
	</div>
</div>

<style>
	.counter {
		background: var(--panel);
		border: 1px solid var(--panel-edge);
		border-radius: 10px;
		padding: 1rem 1.2rem 1.15rem;
	}
	.label {
		display: block;
		font-family: var(--mono);
		font-size: 0.68rem;
		letter-spacing: 0.18em;
		text-transform: uppercase;
		color: var(--ink-dim);
		margin-bottom: 0.45rem;
	}
	.counter-row {
		position: relative;
		display: flex;
		align-items: baseline;
		gap: 0.8rem;
		font-family: var(--mono);
		font-weight: 600;
		font-size: clamp(2rem, 4.5vw, 3rem);
		color: var(--brass-bright);
		text-shadow: 0 0 24px rgba(232, 200, 124, 0.25);
	}
	.delta {
		position: absolute;
		right: 0;
		top: -0.4em;
		font-size: 0.85rem;
		font-weight: 500;
		color: var(--ink-dim);
		animation: delta-rise 1.4s ease-out forwards;
		pointer-events: none;
	}
	@keyframes delta-rise {
		0% {
			opacity: 0;
			transform: translateY(0.6em);
		}
		15% {
			opacity: 1;
		}
		100% {
			opacity: 0;
			transform: translateY(-0.9em);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.delta {
			animation: delta-fade 1.4s ease-out forwards;
		}
		@keyframes delta-fade {
			0%,
			60% {
				opacity: 1;
			}
			100% {
				opacity: 0;
			}
		}
	}
</style>
