<script lang="ts">
	import { Chess } from 'chess.js';
	import Chessboard from '$lib/Chessboard.svelte';
	import { lookup, type KnownMove } from '$lib/api';

	const START = new Chess().fen();

	let fen = $state(START);
	let playerColor = $state<'w' | 'b'>('w');
	let known = $state<KnownMove[]>([]);
	let total = $state(0);
	let status = $state<'loading' | 'play' | 'offbook' | 'over' | 'error'>('loading');
	let resultText = $state('');
	let busy = $state(false);
	let history = $state<string[]>([]);

	const turn = $derived(new Chess(fen).turn());
	const playerToMove = $derived(turn === playerColor);
	// Player may click only on their own turn, when not mid-animation, and when
	// there's a genuine choice (single-move lines auto-advance).
	const interactive = $derived(status === 'play' && playerToMove && !busy && known.length > 1);
	const allowedUcis = $derived(playerToMove ? known.map((m) => m.uci) : []);

	const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

	function pickWeighted(moves: KnownMove[]): KnownMove {
		const sum = moves.reduce((s, m) => s + m.count, 0);
		let r = Math.random() * sum;
		for (const m of moves) {
			r -= m.count;
			if (r <= 0) return m;
		}
		return moves[0];
	}

	function describeResult(g: Chess): string {
		if (g.isCheckmate()) {
			// The side to move has been mated.
			const loser = g.turn();
			if (loser === playerColor) return 'Checkmate — you lose.';
			return 'Checkmate — you win!';
		}
		if (g.isStalemate()) return 'Stalemate — draw.';
		if (g.isInsufficientMaterial()) return 'Draw — insufficient material.';
		return 'Game over — draw.';
	}

	/** Apply a move (UCI) and continue the game loop. */
	async function play(uci: string) {
		const g = new Chess(fen);
		g.move({ from: uci.slice(0, 2), to: uci.slice(2, 4), promotion: uci.slice(4, 5) || undefined });
		history = [...history, g.history().slice(-1)[0]];
		fen = g.fen();
		await loop();
	}

	/** Look up the current position and either auto-advance or hand off to the player. */
	async function loop() {
		const g = new Chess(fen);
		if (g.isGameOver()) {
			status = 'over';
			resultText = describeResult(g);
			return;
		}

		busy = true;
		status = 'loading';
		try {
			const resp = await lookup(fen);
			known = resp.moves;
			total = resp.total;
		} catch (e) {
			status = 'error';
			resultText = String(e);
			busy = false;
			return;
		}

		if (known.length === 0) {
			status = 'offbook';
			resultText = 'This line left the known games — no recorded continuation.';
			busy = false;
			return;
		}

		status = 'play';
		const playerTurn = g.turn() === playerColor;

		// The opponent always auto-plays; the player auto-plays only when forced
		// (a single known move). This is how a narrowed-down "one known game"
		// plays itself out to the result.
		if (!playerTurn) {
			await delay(450);
			busy = false;
			await play(pickWeighted(known).uci);
		} else if (known.length === 1) {
			await delay(350);
			busy = false;
			await play(known[0].uci);
		} else {
			busy = false; // wait for the player to click
		}
	}

	function restart() {
		fen = START;
		history = [];
		known = [];
		total = 0;
		resultText = '';
		loop();
	}

	// Kick off (and restart whenever the player changes colour).
	$effect(() => {
		void playerColor;
		restart();
	});
</script>

<main>
	<header>
		<h1>known-chess</h1>
		<p class="tag">Only moves played in real lichess games are legal. When one game remains, it plays itself out.</p>
	</header>

	<div class="layout">
		<Chessboard {fen} allowed={allowedUcis} {interactive} onMove={play} />

		<aside class="panel">
			<div class="controls">
				<label>
					Play as
					<select bind:value={playerColor}>
						<option value="w">White</option>
						<option value="b">Black</option>
					</select>
				</label>
				<button onclick={restart}>Restart</button>
			</div>

			<div class="status">
				{#if status === 'loading'}
					<p>Thinking…</p>
				{:else if status === 'over' || status === 'offbook' || status === 'error'}
					<p class="result">{resultText}</p>
				{:else if !playerToMove}
					<p>Opponent to move…</p>
				{:else}
					<p><strong>Your move.</strong> {known.length} known continuation{known.length === 1 ? '' : 's'}.</p>
				{/if}
				<p class="games">{total.toLocaleString()} game{total === 1 ? '' : 's'} reached this position.</p>
			</div>

			{#if status === 'play' && playerToMove && known.length > 0}
				<ol class="moves">
					{#each known as m}
						<li>
							<button onclick={() => play(m.uci)} disabled={busy}>
								<span class="san">{m.san}</span>
								<span class="count">{m.count.toLocaleString()}</span>
							</button>
						</li>
					{/each}
				</ol>
			{/if}

			<div class="history">
				{#each history as san, i}
					{#if i % 2 === 0}<span class="num">{i / 2 + 1}.</span>{/if}
					<span class="ply">{san}</span>
				{/each}
			</div>
		</aside>
	</div>
</main>

<style>
	:global(body) {
		margin: 0;
		background: #312e2b;
		color: #e8e6e3;
		font-family: system-ui, sans-serif;
	}
	main {
		max-width: 980px;
		margin: 0 auto;
		padding: 1.5rem;
	}
	header h1 {
		margin: 0;
		font-weight: 800;
		letter-spacing: -0.02em;
	}
	.tag {
		margin: 0.25rem 0 1.5rem;
		color: #b8b4ae;
		max-width: 48ch;
	}
	.layout {
		display: flex;
		gap: 1.5rem;
		flex-wrap: wrap;
		align-items: flex-start;
	}
	.panel {
		flex: 1;
		min-width: 240px;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	.controls {
		display: flex;
		gap: 0.75rem;
		align-items: center;
	}
	select,
	button {
		font: inherit;
		background: #45413c;
		color: inherit;
		border: 1px solid #5c574f;
		border-radius: 6px;
		padding: 0.4rem 0.6rem;
		cursor: pointer;
	}
	.result {
		font-weight: 700;
		font-size: 1.1rem;
	}
	.games {
		color: #b8b4ae;
		font-size: 0.85rem;
	}
	.moves {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		max-height: 320px;
		overflow-y: auto;
	}
	.moves button {
		width: 100%;
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	.moves .san {
		font-weight: 700;
	}
	.moves .count {
		color: #b8b4ae;
		font-size: 0.85rem;
	}
	.history {
		font-size: 0.85rem;
		color: #cfccc7;
		line-height: 1.7;
		word-spacing: 0.15rem;
	}
	.history .num {
		color: #8a857d;
		margin-left: 0.4rem;
	}
</style>
