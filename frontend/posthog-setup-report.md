# PostHog post-wizard report

The wizard has completed a deep integration of PostHog analytics into Precedent Chess. Here is a summary of all changes made:

- **`src/hooks.client.ts`** (new file) — Initialises PostHog via the `init()` SvelteKit hook using environment variables. Also wires up `handleError` to capture client-side exceptions automatically.
- **`src/hooks.server.ts`** — Extended to include an `/ingest` reverse proxy so PostHog requests route through your own domain and bypass ad blockers. A `handleError` hook was added to catch server-side errors.
- **`svelte.config.js`** — Added `paths.relative = false`, required for session replay to work correctly under SSR.
- **`.env`** — Created with `PUBLIC_POSTHOG_PROJECT_TOKEN` and `PUBLIC_POSTHOG_HOST`.
- **`src/lib/game.svelte.ts`** — Six capture calls added to the `KnownGame` class covering the full game lifecycle.
- **`src/lib/GameVeil.svelte`** — Capture on the "Open on Lichess" link click.
- **`src/lib/MoveHistory.svelte`** — Capture when the FEN is copied to clipboard.

## Events instrumented

| Event | Description | File |
|---|---|---|
| `game_started` | Player starts a new game (onMount and "New game" / "Play again" buttons) | `src/lib/game.svelte.ts` |
| `move_chosen` | Player actively picks a move with precedent (not a forced autoplay). Properties: `san`, `uci`, `game_count`, `choices_available`, `move_number` | `src/lib/game.svelte.ts` |
| `game_completed` | Game reaches a terminal state (checkmate, stalemate, etc). Properties: `result_type`, `moves_played`, `matching_games` | `src/lib/game.svelte.ts` |
| `position_off_record` | The chosen line has no continuations in the archive. Properties: `moves_played` | `src/lib/game.svelte.ts` |
| `source_game_found` | Server identified the real Lichess game replayed. Properties: `exact_match`, `lichess_id`, `speed` | `src/lib/game.svelte.ts` |
| `takeback_used` | Player rewound to their last genuine choice. Properties: `moves_rewound` | `src/lib/game.svelte.ts` |
| `lichess_link_opened` | Player clicked "Open on Lichess". Properties: `lichess_id`, `exact_match` | `src/lib/GameVeil.svelte` |
| `fen_copied` | Player copied the current position FEN. Properties: `move_count` | `src/lib/MoveHistory.svelte` |

## Next steps

We've built some insights and a dashboard for you to keep an eye on user behaviour, based on the events we just instrumented:

- [Analytics basics (wizard) — Dashboard](https://us.posthog.com/project/468099/dashboard/1706850)
- [Games started over time](https://us.posthog.com/project/468099/insights/P0LBza4R) — daily trend of new games
- [Game outcomes by result type](https://us.posthog.com/project/468099/insights/UchbkaB0) — breakdown of checkmate vs stalemate vs draw
- [Average moves per completed game](https://us.posthog.com/project/468099/insights/0RNNuMBg) — engagement depth over time
- [Off-record rate: games started vs. off-record positions](https://us.posthog.com/project/468099/insights/sTwHfHR7) — how often players venture beyond the archive
- [Lichess link engagement](https://us.posthog.com/project/468099/insights/tdnk3o4W) — source-game identification vs click-through rate

### Agent skill

We've left an agent skill folder in your project. You can use this context for further agent development when using Claude Code. This will help ensure the model provides the most up-to-date approaches for integrating PostHog.
