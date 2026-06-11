<script lang="ts">
	import type { HistEntry } from '$lib/game.svelte';
	import { fmtCompact } from '$lib/format';

	let { history }: { history: HistEntry[] } = $props();

	const moveRows = $derived.by(() => {
		const out: { n: number; w?: HistEntry; b?: HistEntry }[] = [];
		for (const e of history) {
			if (e.mover === 'w' || out.length === 0) out.push({ n: out.length + 1, w: e.mover === 'w' ? e : undefined, b: e.mover === 'b' ? e : undefined });
			else out[out.length - 1].b = e;
		}
		return out;
	});

	let el = $state<HTMLElement | null>(null);
	$effect(() => {
		void history.length;
		if (el) el.scrollTop = el.scrollHeight;
	});
</script>

<div class="history" bind:this={el}>
	{#if history.length === 0}
		<p class="hist-empty">The opening ledger is blank — make a move.</p>
	{:else}
		<table>
			<tbody>
				{#each moveRows as row (row.n)}
					<tr>
						<td class="hnum">{row.n}.</td>
						{#each [row.w, row.b] as e, i (i)}
							<td class="hply" class:auto={e && e.choices <= 1}>
								{#if e}
									<span class="hsan">{e.san}</span>
									<span class="hcount">{fmtCompact(e.count)}</span>
								{/if}
							</td>
						{/each}
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</div>

<style>
	.history {
		background: var(--panel);
		border: 1px solid var(--panel-edge);
		border-radius: 10px;
		padding: 0.6rem 0.9rem;
		max-height: 200px;
		overflow-y: auto;
		scrollbar-width: thin;
		scrollbar-color: var(--panel-edge) transparent;
	}
	.hist-empty {
		margin: 0.2rem 0;
		color: var(--ink-faint);
		font-style: italic;
		font-size: 0.9rem;
	}
	table {
		width: 100%;
		border-collapse: collapse;
	}
	.hnum {
		font-family: var(--mono);
		font-size: 0.72rem;
		color: var(--ink-faint);
		width: 2.2rem;
		padding: 0.18rem 0;
	}
	.hply {
		padding: 0.18rem 0.3rem;
		width: 45%;
	}
	.hsan {
		font-weight: 600;
		font-size: 0.95rem;
	}
	.hply.auto .hsan {
		color: var(--ink-dim);
		font-weight: 400;
		font-style: italic;
	}
	.hcount {
		font-family: var(--mono);
		font-size: 0.66rem;
		color: var(--ink-faint);
		margin-left: 0.45rem;
	}
</style>
