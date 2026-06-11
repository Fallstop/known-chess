import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		// Proxy API calls to the Rust server during development so the browser
		// talks to a same-origin /api and we avoid CORS faff.
		proxy: {
			'/api': {
				target: process.env.KC_SERVER_URL ?? 'http://localhost:8080',
				changeOrigin: true
			}
		}
	}
});
