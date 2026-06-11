<script lang="ts">
	import { fade } from 'svelte/transition';

	let { value }: { value: number } = $props();

	// Key each character by its distance from the right edge: comma grouping is
	// stable from the right, so when the number loses digits only the leftmost
	// columns unmount and every surviving digit keeps its rolling column.
	const chars = $derived.by(() => {
		const s = Math.max(0, Math.floor(value)).toLocaleString('en-US');
		return [...s].map((ch, i) => ({ ch, key: s.length - i }));
	});

	const DIGITS = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
</script>

<span class="odo">
	<span class="sr-only">{Math.max(0, Math.floor(value)).toLocaleString('en-US')}</span>
	{#each chars as c (c.key)}
		{#if c.ch >= '0' && c.ch <= '9'}
			<span class="col" transition:fade={{ duration: 200 }} aria-hidden="true">
				<span class="strip" style="transform: translateY({-Number(c.ch)}em)">
					{#each DIGITS as d}<span class="d">{d}</span>{/each}
				</span>
			</span>
		{:else}
			<span class="sep" transition:fade={{ duration: 200 }} aria-hidden="true">{c.ch}</span>
		{/if}
	{/each}
</span>

<style>
	.odo {
		display: inline-flex;
		align-items: baseline;
		font-variant-numeric: tabular-nums;
		line-height: 1;
	}
	.col {
		display: inline-block;
		height: 1em;
		overflow: hidden;
	}
	.strip {
		display: block;
		transition: transform 0.65s cubic-bezier(0.25, 0.9, 0.25, 1);
		will-change: transform;
	}
	.d {
		display: block;
		height: 1em;
		line-height: 1em;
	}
	.sep {
		display: inline-block;
		line-height: 1em;
	}
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}
</style>
