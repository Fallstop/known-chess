#!/usr/bin/env bash
# Run kc-server (Rust API) and the SvelteKit Node server together in one
# container. If either exits, bring the whole container down so the orchestrator
# (CapRover) restarts it.
set -euo pipefail

if [[ ! -f "${KC_BOOK_PATH:-/data/book.book}" ]]; then
	echo "start.sh: book not found at '${KC_BOOK_PATH:-/data/book.book}'." >&2
	echo "  Mount a volume and put the book there, or set KC_BOOK_PATH." >&2
	exit 1
fi

kc-server &
server_pid=$!

node /app/frontend/build &
frontend_pid=$!

# Exit as soon as either process dies, propagating its status.
wait -n
status=$?
kill "$server_pid" "$frontend_pid" 2>/dev/null || true
exit "$status"
