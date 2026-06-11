<script lang="ts">
	import { Chess } from 'chess.js';
	import { onMount, untrack } from 'svelte';
	import '@fontsource-variable/fraunces';
	import '@fontsource/ibm-plex-mono/400.css';
	import '@fontsource/ibm-plex-mono/500.css';
	import '@fontsource/ibm-plex-mono/600.css';
	import Chessboard from '$lib/Chessboard.svelte';
	import Odometer from '$lib/Odometer.svelte';
	import { lookup, meta, type KnownMove } from '$lib/api';
	import { fmtCompact } from '$lib/format';
	import { moveSound, forcedSound, endSound } from '$lib/audio';

	const START = new Chess().fen();

	type Phase = 'loading' | 'choose' | 'forced' | 'over' | 'dry' | 'error';

	interface HistEntry {
		san: string;
		mover: 'w' | 'b';
		/** Games that played this move from the position it was made in. */
		count: number;
		/** How many known moves existed when it was made (1 = autoplayed). */
		choices: number;
		/** FEN before the move, for takebacks. */
		preFen: string;
		from: string;
		to: string;
	}

	let fen = $state(START);
	let known = $state<KnownMove[]>([]);
	let total = $state(0);
	let phase = $state<Phase>('loading');
	let errorMsg = $state('');
	let history = $state<HistEntry[]>([]);
	let flipped = $state(false);
	let sound = $state(true);
	let archivePositions = $state<number | null>(null);
	let previewUci = $state<string | null>(null);
	let names = $state({ w: '', b: '' });
	/** Bumped on new-game/takeback so in-flight lookups and autoplay timers abort. */
	let gen = 0;

	const game = $derived(new Chess(fen));
	const turn = $derived(game.turn());
	const interactive = $derived(phase === 'choose');
	const lastMove = $derived(history.length ? { from: history[history.length - 1].from, to: history[history.length - 1].to } : null);
	const lastEntry = $derived(history.length ? history[history.length - 1] : null);

	const checkSquare = $derived.by(() => {
		if (!game.inCheck()) return null;
		for (const row of game.board()) {
			for (const p of row) {
				if (p && p.type === 'k' && p.color === turn) return p.square;
			}
		}
		return null;
	});

	// ——— captured material ———————————————————————————————————————————————

	const VALUES: Record<string, number> = { p: 1, n: 3, b: 3, r: 5, q: 9 };
	const START_COUNTS: Record<string, number> = { p: 8, n: 2, b: 2, r: 2, q: 1 };
	const ORDER = ['q', 'r', 'b', 'n', 'p'];

	const material = $derived.by(() => {
		const present = {
			w: { p: 0, n: 0, b: 0, r: 0, q: 0 } as Record<string, number>,
			b: { p: 0, n: 0, b: 0, r: 0, q: 0 } as Record<string, number>
		};
		for (const row of game.board()) {
			for (const p of row) {
				if (p && p.type !== 'k') present[p.color][p.type]++;
			}
		}
		// Pieces each side has taken (= the opponent's missing pieces).
		const taken = (victim: 'w' | 'b') =>
			ORDER.flatMap((t) => Array(Math.max(0, START_COUNTS[t] - present[victim][t])).fill(t) as string[]);
		const caps = { w: taken('b'), b: taken('w') };
		const score = (list: string[]) => list.reduce((s, t) => s + VALUES[t], 0);
		const diff = score(caps.w) - score(caps.b);
		return { caps, diff };
	});

	// ——— game loop ———————————————————————————————————————————————————————

	const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

	async function step(g: number): Promise<void> {
		const pos = new Chess(fen);
		if (pos.isGameOver()) {
			phase = 'over';
			known = [];
			// The hero counter lands on the games that ended exactly here.
			if (lastEntry) total = lastEntry.count;
			if (sound) endSound(pos.isCheckmate());
			return;
		}
		phase = 'loading';
		let resp;
		try {
			resp = await lookup(fen);
		} catch (e) {
			if (g !== gen) return;
			phase = 'error';
			errorMsg = e instanceof Error ? e.message : String(e);
			return;
		}
		if (g !== gen) return;
		known = resp.moves;
		total = resp.total;

		if (known.length === 0) {
			phase = 'dry';
			return;
		}
		if (known.length === 1) {
			// Locked in: every remaining game continued the same way, so the
			// archive plays the move itself after a beat.
			phase = 'forced';
			await delay(800);
			if (g !== gen) return;
			applyMove(known[0], true);
			return step(g);
		}
		phase = 'choose';
	}

	function applyMove(mv: KnownMove, auto = false) {
		const pos = new Chess(fen);
		const preFen = fen;
		const mover = pos.turn();
		const choices = known.length;
		const res = pos.move({
			from: mv.uci.slice(0, 2),
			to: mv.uci.slice(2, 4),
			promotion: mv.uci.slice(4, 5) || undefined
		});
		history = [...history, { san: res.san, mover, count: mv.count, choices, preFen, from: res.from, to: res.to }];
		fen = pos.fen();
		if (sound) {
			const capture = res.captured !== undefined;
			if (auto) forcedSound(capture);
			else moveSound(capture);
		}
	}

	function choose(mv: KnownMove) {
		if (phase !== 'choose') return;
		previewUci = null;
		applyMove(mv);
		void step(gen);
	}

	function onBoardMove(uci: string) {
		const mv = known.find((m) => m.uci === uci);
		if (mv) choose(mv);
	}

	function newGame() {
		gen++;
		fen = START;
		history = [];
		known = [];
		total = 0;
		previewUci = null;
		phase = 'loading';
		void step(gen);
	}

	/** Rewind to the most recent position where there was a genuine choice. */
	function takeback() {
		if (!history.length || phase === 'loading') return;
		gen++;
		const h = [...history];
		let entry = h.pop()!;
		while (h.length && entry.choices <= 1) entry = h.pop()!;
		history = h;
		fen = entry.preFen;
		previewUci = null;
		void step(gen);
	}

	function retry() {
		gen++;
		void step(gen);
	}

	onMount(() => {
		newGame();
		meta()
			.then((m) => (archivePositions = m.positions))
			.catch(() => {});
	});

	// ——— narrowing delta chip ————————————————————————————————————————————

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

	// ——— presentation helpers ————————————————————————————————————————————

	const result = $derived.by(() => {
		if (phase !== 'over') return null;
		if (game.isCheckmate()) {
			const winner = turn === 'w' ? 'b' : 'w';
			const name = (winner === 'w' ? names.w : names.b) || (winner === 'w' ? 'White' : 'Black');
			return { title: 'Checkmate.', detail: `${name} wins` };
		}
		if (game.isStalemate()) return { title: 'Stalemate.', detail: 'Drawn — no legal reply' };
		if (game.isInsufficientMaterial()) return { title: 'Dead position.', detail: 'Drawn — bare kings' };
		return { title: 'Draw.', detail: 'Game drawn' };
	});

	const moveRows = $derived.by(() => {
		const out: { n: number; w?: HistEntry; b?: HistEntry }[] = [];
		for (const e of history) {
			if (e.mover === 'w' || out.length === 0) out.push({ n: out.length + 1, w: e.mover === 'w' ? e : undefined, b: e.mover === 'b' ? e : undefined });
			else out[out.length - 1].b = e;
		}
		return out;
	});

	let historyEl = $state<HTMLElement | null>(null);
	$effect(() => {
		void history.length;
		if (historyEl) historyEl.scrollTop = historyEl.scrollHeight;
	});

	const toMoveName = $derived((turn === 'w' ? names.w : names.b) || (turn === 'w' ? 'White' : 'Black'));
	const topColor = $derived<'w' | 'b'>(flipped ? 'w' : 'b');
	const bottomColor = $derived<'w' | 'b'>(flipped ? 'b' : 'w');
</script>

<svelte:head>
	<title>Known Chess — every move once lived</title>
	<meta name="description" content="Chess where only moves played in real games are legal. Narrow eight million games down to one." />
</svelte:head>

{#snippet plate(color: 'w' | 'b')}
	<div class="plate" class:active={turn === color && phase !== 'over' && phase !== 'dry'}>
		<span class="lamp" class:white={color === 'w'}></span>
		<input
			class="pname"
			placeholder={color === 'w' ? 'White' : 'Black'}
			maxlength="20"
			spellcheck="false"
			bind:value={names[color]}
		/>
		<span class="caps">
			{#each material.caps[color] as t, i (i)}
				<img src="/pieces/{color === 'w' ? 'b' : 'w'}{t.toUpperCase()}.svg" alt="captured {t}" />
			{/each}
			{#if (color === 'w' ? material.diff : -material.diff) > 0}
				<span class="mdiff">+{color === 'w' ? material.diff : -material.diff}</span>
			{/if}
		</span>
	</div>
{/snippet}

<div class="page">
	<header>
		<h1>Known<span class="amp">/</span>Chess</h1>
		<p class="lede">
			A move is legal only if someone, somewhere, actually played it. Every game here is drawn from real
			Lichess games that ended over the board — and as you play, the archive narrows. When one path
			remains, history finishes the game for you.
		</p>
		{#if archivePositions !== null}
			<p class="archive-size">{archivePositions.toLocaleString()} positions remembered</p>
		{/if}
	</header>

	<main>
		<section class="arena">
			{@render plate(topColor)}
			<div class="board-frame">
				<Chessboard
					{fen}
					allowed={interactive ? known : []}
					{interactive}
					{flipped}
					{lastMove}
					{checkSquare}
					{previewUci}
					onMove={onBoardMove}
				/>
				{#if phase === 'over' && result}
					<div class="veil">
						<p class="veil-title">{result.title}</p>
						<p class="veil-detail">{result.detail}</p>
						{#if lastEntry}
							<p class="veil-note">
								{lastEntry.count.toLocaleString()}
								{lastEntry.count === 1 ? 'game' : 'games'} ended exactly here.
							</p>
						{/if}
						<button class="ctl primary" onclick={newGame}>Play again</button>
					</div>
				{:else if phase === 'dry'}
					<div class="veil">
						<p class="veil-title">The archive runs dry.</p>
						<p class="veil-note">No recorded game continued from this position.</p>
						<button class="ctl primary" onclick={newGame}>Play again</button>
					</div>
				{:else if phase === 'error'}
					<div class="veil">
						<p class="veil-title">Lost the thread.</p>
						<p class="veil-note">{errorMsg}</p>
						<button class="ctl primary" onclick={retry}>Retry</button>
					</div>
				{/if}
			</div>
			{@render plate(bottomColor)}
		</section>

		<aside class="ledger">
			<div class="counter">
				<span class="label">games remain</span>
				<div class="counter-row">
					<Odometer value={total} />
					{#if delta}
						{#key delta.key}
							<span class="delta">−{fmtCompact(delta.amount)}</span>
						{/key}
					{/if}
				</div>
			</div>

			<div class="state" class:forced={phase === 'forced'}>
				{#if phase === 'loading'}
					<span class="state-main dim">Consulting the archive<span class="ellip"></span></span>
				{:else if phase === 'forced'}
					<span class="state-main">Locked in</span>
					<span class="state-sub">Every remaining game agrees — the line plays itself out.</span>
				{:else if phase === 'choose'}
					<span class="state-main">{toMoveName} to move</span>
					<span class="state-sub">{known.length} known paths diverge here</span>
				{:else if phase === 'over' && result}
					<span class="state-main">{result.title.replace(/\.$/, '')}</span>
					<span class="state-sub">{result.detail}</span>
				{:else if phase === 'dry'}
					<span class="state-main">Off the record</span>
				{:else}
					<span class="state-main dim">—</span>
				{/if}
			</div>

			{#if phase === 'choose'}
				<ol class="paths">
					{#each known as mv (mv.uci)}
						<li>
							<button
								class="path"
								onclick={() => choose(mv)}
								onmouseenter={() => (previewUci = mv.uci)}
								onmouseleave={() => (previewUci = null)}
								onfocus={() => (previewUci = mv.uci)}
								onblur={() => (previewUci = null)}
							>
								<span class="psan">{mv.san}</span>
								<span class="pbar"><span style="width:{(mv.count / known[0].count) * 100}%"></span></span>
								<span class="pcount">{fmtCompact(mv.count)}</span>
								<span class="ppct">{total ? (mv.count / total >= 0.01 ? Math.round((mv.count / total) * 100) + '%' : '<1%') : ''}</span>
							</button>
						</li>
					{/each}
				</ol>
			{/if}

			<div class="history" bind:this={historyEl}>
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

			<div class="controls">
				<button class="ctl" onclick={newGame}>New game</button>
				<button class="ctl" onclick={takeback} disabled={history.length === 0 || phase === 'loading' || phase === 'forced'}>
					Takeback
				</button>
				<button class="ctl" onclick={() => (flipped = !flipped)}>Flip</button>
				<button class="ctl" class:off={!sound} onclick={() => (sound = !sound)} aria-pressed={sound}>
					Sound {sound ? 'on' : 'off'}
				</button>
			</div>
		</aside>
	</main>

	<footer>
		<p>
			Moves are weighted by how often they were played; small print on each target square is how many
			games went that way. Autoplayed moves appear <span class="auto-demo">dimmed</span> in the ledger.
		</p>
	</footer>
</div>

<style>
	:global(:root) {
		--bg: #0f1613;
		--panel: #151e19;
		--panel-edge: #25322b;
		--ink: #ece5d3;
		--ink-dim: #9aa394;
		--ink-faint: #5d685f;
		--brass: #c9a04e;
		--brass-bright: #e8c87c;
		--brass-soft: rgba(201, 160, 78, 0.45);
		--brass-tint: rgba(201, 160, 78, 0.14);
		--brass-tint-strong: rgba(201, 160, 78, 0.32);
		--brass-glow: rgba(232, 200, 124, 0.5);
		--sq-light: #e6d7b4;
		--sq-dark: #8a6246;
		--serif: 'Fraunces Variable', Georgia, 'Times New Roman', serif;
		--mono: 'IBM Plex Mono', ui-monospace, 'Cascadia Mono', monospace;
	}

	:global(body) {
		margin: 0;
		background:
			radial-gradient(1100px 700px at 75% -10%, #1b2922 0%, transparent 60%),
			radial-gradient(900px 600px at -10% 110%, #141f18 0%, transparent 55%),
			var(--bg);
		color: var(--ink);
		font-family: var(--serif);
		font-optical-sizing: auto;
		-webkit-font-smoothing: antialiased;
	}
	/* film grain */
	:global(body)::after {
		content: '';
		position: fixed;
		inset: 0;
		pointer-events: none;
		z-index: 100;
		opacity: 0.05;
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
	}

	.page {
		max-width: 1180px;
		margin: 0 auto;
		padding: 2.5rem 1.5rem 3rem;
	}

	/* ——— header ——— */
	header {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.4rem 2.5rem;
		align-items: end;
		margin-bottom: 2.2rem;
	}
	h1 {
		grid-row: span 2;
		margin: 0;
		font-size: clamp(2.6rem, 6vw, 4.2rem);
		font-weight: 850;
		font-variation-settings: 'opsz' 144;
		letter-spacing: -0.03em;
		line-height: 0.95;
	}
	h1 .amp {
		color: var(--brass);
		font-style: italic;
		font-weight: 400;
		margin: 0 0.05em;
	}
	.lede {
		margin: 0;
		max-width: 58ch;
		color: var(--ink-dim);
		font-size: 0.98rem;
		line-height: 1.55;
		align-self: end;
	}
	.archive-size {
		margin: 0;
		font-family: var(--mono);
		font-size: 0.72rem;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--brass);
	}

	/* ——— layout ——— */
	main {
		display: grid;
		grid-template-columns: minmax(0, 1.35fr) minmax(300px, 1fr);
		gap: 2.2rem;
		align-items: start;
	}
	@media (max-width: 900px) {
		main {
			grid-template-columns: 1fr;
		}
		header {
			grid-template-columns: 1fr;
			align-items: start;
		}
		h1 {
			grid-row: auto;
		}
	}

	/* ——— arena ——— */
	.arena {
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
	}
	.board-frame {
		position: relative;
	}

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

	/* ——— end-of-game veil ——— */
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

	/* ——— ledger ——— */
	.ledger {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		min-width: 0;
	}

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

	.state {
		padding: 0.7rem 1.2rem;
		background: var(--panel);
		border: 1px solid var(--panel-edge);
		border-radius: 10px;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
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

	/* ——— continuations ——— */
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
		transition: border-color 0.15s, background 0.15s, transform 0.15s;
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

	/* ——— history ——— */
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
	.history table {
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

	/* ——— controls ——— */
	.controls {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}
	.ctl {
		font-family: var(--mono);
		font-size: 0.72rem;
		font-weight: 500;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--ink-dim);
		background: none;
		border: 1px solid var(--panel-edge);
		border-radius: 999px;
		padding: 0.45rem 1rem;
		cursor: pointer;
		transition: color 0.15s, border-color 0.15s, background 0.15s;
	}
	.ctl:hover:not(:disabled) {
		color: var(--brass-bright);
		border-color: var(--brass-soft);
	}
	.ctl:disabled {
		opacity: 0.35;
		cursor: default;
	}
	.ctl.off {
		text-decoration: line-through;
	}
	.ctl.primary {
		color: #14110a;
		background: var(--brass);
		border-color: var(--brass);
		font-weight: 600;
	}
	.ctl.primary:hover {
		background: var(--brass-bright);
		border-color: var(--brass-bright);
		color: #14110a;
	}

	footer {
		margin-top: 2.5rem;
		border-top: 1px solid var(--panel-edge);
		padding-top: 1rem;
	}
	footer p {
		margin: 0;
		color: var(--ink-faint);
		font-size: 0.82rem;
		max-width: 72ch;
	}
	.auto-demo {
		font-style: italic;
		color: var(--ink-dim);
	}
</style>
