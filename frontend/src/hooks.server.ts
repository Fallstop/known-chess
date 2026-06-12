import type { Handle, HandleServerError } from '@sveltejs/kit';

/**
 * Reverse proxy for PostHog — routes /ingest/* to PostHog's servers so the
 * analytics requests are same-origin and survive ad blockers. Runs in the
 * Cloudflare Pages worker.
 *
 * The Rust API is NOT proxied here: the browser calls it directly at
 * PUBLIC_KC_API_URL (see src/lib/api.ts); the server allows CORS. In dev,
 * Vite proxies /api instead (see vite.config.ts).
 */
export const handle: Handle = async ({ event, resolve }) => {
	const { pathname } = event.url;

	if (pathname.startsWith('/ingest')) {
		const useAssetHost =
			pathname.startsWith('/ingest/static/') || pathname.startsWith('/ingest/array/');
		const hostname = useAssetHost ? 'us-assets.i.posthog.com' : 'us.i.posthog.com';

		const url = new URL(event.request.url);
		url.protocol = 'https:';
		url.hostname = hostname;
		url.port = '443';
		url.pathname = pathname.replace(/^\/ingest/, '');

		const headers = new Headers(event.request.headers);
		headers.set('host', hostname);
		headers.set('accept-encoding', '');

		const clientIp =
			event.request.headers.get('x-forwarded-for') || event.getClientAddress();
		if (clientIp) headers.set('x-forwarded-for', clientIp);

		return fetch(url.toString(), {
			method: event.request.method,
			headers,
			body: event.request.body,
			// @ts-expect-error duplex required for streaming request bodies
			duplex: 'half'
		});
	}

	return resolve(event);
};

export const handleError: HandleServerError = async ({ error, message }) => {
	return { message };
};
