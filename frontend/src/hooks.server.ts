import type { Handle } from '@sveltejs/kit';

// Where the Rust `kc-server` listens. In the single-container deployment (see
// the root Dockerfile) both processes share localhost, so this defaults to the
// server's internal port. Override with KC_SERVER_URL if it lives elsewhere.
const KC_SERVER_URL = process.env.KC_SERVER_URL ?? 'http://localhost:8080';

/**
 * Proxy `/api/*` to the Rust server.
 *
 * In dev, Vite proxies `/api` for us (see vite.config.ts). The production Node
 * adapter has no such proxy, so the browser's same-origin `/api` calls would
 * otherwise 404. This handle forwards them to `kc-server` instead, keeping the
 * frontend the only externally exposed port.
 */
export const handle: Handle = async ({ event, resolve }) => {
	if (event.url.pathname.startsWith('/api/')) {
		const target = KC_SERVER_URL + event.url.pathname + event.url.search;
		const method = event.request.method;
		const init: RequestInit = {
			method,
			headers: { 'content-type': event.request.headers.get('content-type') ?? 'application/json' }
		};
		if (method !== 'GET' && method !== 'HEAD') {
			init.body = await event.request.arrayBuffer();
		}
		return fetch(target, init);
	}
	return resolve(event);
};
