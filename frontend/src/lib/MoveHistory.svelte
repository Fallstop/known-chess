<script lang="ts">
	import type { HistEntry } from '$lib/game.svelte';
	import { fmtCompact } from '$lib/format';
	import posthog from 'posthog-js';

	let { history, fen }: { history: HistEntry[]; fen: string } = $props();

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

	let copied = $state(false);
	let copyTimer: ReturnType<typeof setTimeout>;

	async function copyFen() {
		try {
			await navigator.clipboard.writeText(fen);
		} catch {
			// Clipboard API needs a secure context; fall back to a hidden textarea.
			const ta = document.createElement('textarea');
			ta.value = fen;
			ta.style.position = 'fixed';
			ta.style.opacity = '0';
			document.body.appendChild(ta);
			ta.select();
			try {
				document.execCommand('copy');
			} finally {
				ta.remove();
			}
		}
		posthog.capture('fen_copied', { move_count: history.length });
		copied = true;
		clearTimeout(copyTimer);
		copyTimer = setTimeout(() => (copied = false), 1400);
	}
</script>

<div class="ledger-history">
	<div class="hist-head">
		<span class="hist-label">moves</span>
		<button class="copy-fen" class:done={copied} onclick={copyFen} title="Copy current position as FEN">
			{copied ? 'Copied ✓' : 'Copy FEN'}
		</button>
	</div>
	<div class="history" bind:this={el}>
		{#if history.length === 0}
			<p class="hist-empty">The opening ledger is blank. Make a move.</p>
		{:else}
			<table>
				<caption class="sr-only">
					Moves played so far. Next to each move: how many recorded games played it from that
					position.
				</caption>
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
</div>

<style>
	.ledger-history {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.hist-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.6rem;
	}
	.hist-label {
		font-family: var(--mono);
		font-size: 0.68rem;
		letter-spacing: 0.18em;
		text-transform: uppercase;
		color: var(--ink-dim);
	}
	.copy-fen {
		font-family: var(--mono);
		font-size: 0.64rem;
		font-weight: 500;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--ink-dim);
		background: none;
		border: 1px solid var(--panel-edge);
		border-radius: 999px;
		padding: 0.25rem 0.7rem;
		cursor: pointer;
		transition: color 0.15s, border-color 0.15s;
	}
	.copy-fen:hover,
	.copy-fen:focus-visible {
		color: var(--brass-bright);
		border-color: var(--brass-soft);
	}
	.copy-fen:focus-visible {
		outline: 2px solid var(--brass-bright);
		outline-offset: 2px;
	}
	.copy-fen.done {
		color: var(--brass-bright);
		border-color: var(--brass-soft);
	}
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
