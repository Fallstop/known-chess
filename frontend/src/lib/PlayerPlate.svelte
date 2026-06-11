<script lang="ts">
	import type { KnownGame } from '$lib/game.svelte';

	let { game, color }: { game: KnownGame; color: 'w' | 'b' } = $props();

	const active = $derived(game.turn === color && game.phase !== 'over' && game.phase !== 'dry');
	const diff = $derived(color === 'w' ? game.material.diff : -game.material.diff);
</script>

<div class="plate" class:active>
	<span class="lamp" class:white={color === 'w'}></span>
	<input
		class="pname"
		placeholder={color === 'w' ? 'White' : 'Black'}
		maxlength="20"
		spellcheck="false"
		bind:value={game.names[color]}
	/>
	<span class="caps">
		{#each game.material.caps[color] as t, i (i)}
			<img src="/pieces/{color === 'w' ? 'b' : 'w'}{t.toUpperCase()}.svg" alt="captured {t}" />
		{/each}
		{#if diff > 0}
			<span class="mdiff">+{diff}</span>
		{/if}
	</span>
</div>

<style>
	.plate {
		display: flex;
		align-items: center;
		gap: 0.7rem;
		padding: 0.45rem 0.8rem;
		background: var(--panel);
		border: 1px solid var(--panel-edge);
		border-radius: 8px;
		transition: border-color 0.25s, box-shadow 0.25s;
	}
	.plate.active {
		border-color: var(--brass-soft);
		box-shadow: 0 0 18px -6px var(--brass-glow);
	}
	.lamp {
		width: 11px;
		height: 11px;
		border-radius: 50%;
		background: #1c1c1c;
		box-shadow: inset 0 0 0 1.5px var(--ink-faint);
		flex: none;
	}
	.lamp.white {
		background: var(--ink);
	}
	.plate.active .lamp {
		box-shadow:
			inset 0 0 0 1.5px var(--brass),
			0 0 8px var(--brass-glow);
	}
	.pname {
		background: none;
		border: 0;
		outline: 0;
		color: var(--ink);
		font-family: var(--serif);
		font-size: 1.05rem;
		font-weight: 600;
		min-width: 0;
		flex: 1;
		padding: 0;
	}
	.pname::placeholder {
		color: var(--ink);
	}
	.pname:focus::placeholder {
		color: var(--ink-faint);
	}
	.caps {
		display: flex;
		align-items: center;
		gap: 0;
	}
	.caps img {
		width: 20px;
		height: 20px;
		margin-left: -7px;
		filter: drop-shadow(0 1px 1px rgba(0, 0, 0, 0.5));
	}
	.mdiff {
		font-family: var(--mono);
		font-size: 0.72rem;
		color: var(--brass-bright);
		margin-left: 0.45rem;
	}
</style>
