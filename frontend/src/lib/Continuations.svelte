<script lang="ts">
	import type { KnownGame } from '$lib/game.svelte';
	import { fmtCompact } from '$lib/format';

	/**
	 * The clickable list of known continuations. Stays mounted (greyed) through
	 * the brief lookup after a move so the layout doesn't jump every ply.
	 */
	let { game }: { game: KnownGame } = $props();

	const visible = $derived(
		(game.phase === 'choose' || game.phase === 'loading') && game.lastChoices.length > 0
	);
</script>

{#if visible}
	<ol class="paths" class:stale={game.phase === 'loading'}>
		{#each game.lastChoices as mv (mv.uci)}
			<li>
				<button
					class="path"
					onclick={() => game.choose(mv)}
					onmouseenter={() => (game.previewUci = mv.uci)}
					onmouseleave={() => (game.previewUci = null)}
					onfocus={() => (game.previewUci = mv.uci)}
					onblur={() => (game.previewUci = null)}
				>
					<span class="psan">{mv.san}</span>
					<span class="pbar"><span style="width:{(mv.count / game.lastChoices[0].count) * 100}%"></span></span>
					<span class="pcount">{fmtCompact(mv.count)}</span>
					<span class="ppct">{game.total ? (mv.count / game.total >= 0.01 ? Math.round((mv.count / game.total) * 100) + '%' : '<1%') : ''}</span>
				</button>
			</li>
		{/each}
	</ol>
{/if}

<style>
	.paths {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		max-height: 305px;
		overflow-y: auto;
		scrollbar-width: thin;
		scrollbar-color: var(--panel-edge) transparent;
	}
	.paths.stale {
		pointer-events: none;
	}
	.paths.stale .path {
		opacity: 0.55;
	}
	.path {
		width: 100%;
		display: grid;
		grid-template-columns: 3.4rem 1fr auto 2.6rem;
		align-items: center;
		gap: 0.7rem;
		background: var(--panel);
		border: 1px solid var(--panel-edge);
		border-radius: 8px;
		padding: 0.45rem 0.75rem;
		color: var(--ink);
		cursor: pointer;
		font: inherit;
		text-align: left;
		transition: border-color 0.15s, background 0.15s, transform 0.15s, opacity 0.15s;
	}
	.path:hover,
	.path:focus-visible {
		border-color: var(--brass-soft);
		background: #1a241e;
		transform: translateX(3px);
		outline: none;
	}
	.psan {
		font-weight: 700;
		font-size: 1.05rem;
	}
	.pbar {
		height: 5px;
		border-radius: 99px;
		background: rgba(255, 255, 255, 0.06);
		overflow: hidden;
	}
	.pbar span {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: linear-gradient(90deg, var(--brass), var(--brass-bright));
		transition: width 0.4s cubic-bezier(0.25, 0.9, 0.3, 1);
	}
	.pcount {
		font-family: var(--mono);
		font-size: 0.78rem;
		color: var(--brass-bright);
	}
	.ppct {
		font-family: var(--mono);
		font-size: 0.7rem;
		color: var(--ink-faint);
		text-align: right;
	}
</style>
