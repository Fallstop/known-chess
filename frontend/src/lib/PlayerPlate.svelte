<script lang="ts">
	import type { KnownGame } from '$lib/game.svelte';

	let { game, color }: { game: KnownGame; color: 'w' | 'b' } = $props();

	const active = $derived(game.turn === color && game.phase !== 'over' && game.phase !== 'dry');
	const diff = $derived(color === 'w' ? game.material.diff : -game.material.diff);
</script>

<div class="plate" class:active class:white={color === 'w'} class:black={color === 'b'}>
	<span class="token" class:white={color === 'w'}>
		<img src="/pieces/{color}K.svg" alt="" />
	</span>
	<span class="who">
		<span class="side-label">{color === 'w' ? 'White' : 'Black'}</span>
		<input
			class="pname"
			placeholder="Add a name"
			maxlength="20"
			spellcheck="false"
			bind:value={game.names[color]}
		/>
	</span>
	{#if active}
		<span class="to-move"><span class="pulse"></span>to move</span>
	{/if}
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
		padding: 0.4rem 0.7rem 0.4rem 0.45rem;
		background: var(--panel);
		border: 1px solid var(--panel-edge);
		border-radius: 10px;
		transition: border-color 0.25s, box-shadow 0.25s, background 0.25s;
	}
	/* A hairline of side colour down the leading edge: cream for white, ink for black. */
	.plate.white {
		border-left: 3px solid var(--sq-light);
	}
	.plate.black {
		border-left: 3px solid #0b0907;
		background: linear-gradient(90deg, rgba(0, 0, 0, 0.28), var(--panel) 22%);
	}
	.plate.active {
		border-color: var(--brass-soft);
		box-shadow: 0 0 18px -6px var(--brass-glow);
	}

	/* King token — the unmissable side tell. */
	.token {
		width: 34px;
		height: 34px;
		flex: none;
		display: grid;
		place-items: center;
		border-radius: 8px;
		background: #120f0b;
		box-shadow: inset 0 0 0 1px var(--panel-edge);
	}
	.token.white {
		background: var(--sq-light);
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.25);
	}
	.token img {
		width: 26px;
		height: 26px;
		filter: drop-shadow(0 1px 1px rgba(0, 0, 0, 0.45));
	}
	.plate.active .token {
		box-shadow:
			inset 0 0 0 1px var(--brass-soft),
			0 0 10px -2px var(--brass-glow);
	}

	.who {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
		line-height: 1.1;
	}
	.side-label {
		font-family: var(--mono);
		font-size: 0.58rem;
		font-weight: 600;
		letter-spacing: 0.18em;
		text-transform: uppercase;
		color: var(--ink-faint);
	}
	.plate.active .side-label {
		color: var(--brass);
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
		width: 100%;
		padding: 0;
	}
	.pname::placeholder {
		color: var(--ink-faint);
		font-style: italic;
		font-weight: 500;
	}

	.to-move {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-family: var(--mono);
		font-size: 0.58rem;
		font-weight: 600;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--brass-bright);
		flex: none;
	}
	.pulse {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--brass-bright);
		box-shadow: 0 0 0 0 var(--brass-glow);
		animation: pulse 1.6s ease-out infinite;
	}
	@keyframes pulse {
		0% {
			box-shadow: 0 0 0 0 var(--brass-glow);
		}
		70% {
			box-shadow: 0 0 0 6px rgba(232, 200, 124, 0);
		}
		100% {
			box-shadow: 0 0 0 0 rgba(232, 200, 124, 0);
		}
	}

	.caps {
		display: flex;
		align-items: center;
		gap: 0;
		flex: none;
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

	@media (prefers-reduced-motion: reduce) {
		.pulse {
			animation: none;
		}
	}
</style>
