import type { Handle } from '@sveltejs/kit';

// Baseline response hardening. Deliberately no script-src CSP: app.html carries an inline theme
// bootstrap script that a strict one would block. What this does cover is the abuse that matters for
// a page showing a deposit address - being framed by another site (clickjacking) - plus MIME
// sniffing and referrer leakage (order ids in URLs are effectively credentials).
// Add `Strict-Transport-Security` at the TLS-terminating proxy, where the scheme is known.
export const handle: Handle = async ({ event, resolve }) => {
	const response = await resolve(event);
	response.headers.set(
		'Content-Security-Policy',
		"frame-ancestors 'none'; base-uri 'self'; object-src 'none'; form-action 'self'"
	);
	response.headers.set('X-Frame-Options', 'DENY');
	response.headers.set('X-Content-Type-Options', 'nosniff');
	response.headers.set('Referrer-Policy', 'no-referrer');
	response.headers.set('Permissions-Policy', 'camera=(), microphone=(), geolocation=()');
	return response;
};
