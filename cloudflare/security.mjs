// APIs that native apps also call (cloud saves): an Origin header must name a
// game origin, while requests without one (desktop/Android HTTP clients, curl)
// pass. Opaque `Origin: null` and cross-site fetch metadata are browsers from
// elsewhere and are refused.
export function browserOriginAllowed(request, allowed) {
  const origin = request.headers.get('Origin');
  if (origin !== null) return allowed.includes(origin);
  return request.headers.get('Sec-Fetch-Site') !== 'cross-site';
}

// Multiplayer is the standalone game server (research/GAME_SERVER.md): pages
// reach its HTTP routes and room sockets cross-origin.
export const GAME_SERVER_ORIGINS = Object.freeze(['https://mp.furyremastered.com', 'wss://mp.furyremastered.com']);

export function secureResponse(response, request, { webAnalytics = false } = {}) {
  // Reconstructing an upgrade response would discard its accepted WebSocket.
  if (response.status === 101) return response;
  const secured = new Response(response.body, response);
  // Cloudflare Web Analytics loads its beacon and reports to these origins only when enabled.
  const beaconScript = webAnalytics ? ' https://static.cloudflareinsights.com' : '', beaconConnect = webAnalytics ? ' https://cloudflareinsights.com' : '';
  secured.headers.set('Content-Security-Policy', `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'${beaconScript}; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; media-src 'self' blob:; connect-src 'self' ${GAME_SERVER_ORIGINS.join(' ')}${beaconConnect}; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`);
  secured.headers.set('X-Content-Type-Options', 'nosniff');
  secured.headers.set('X-Frame-Options', 'DENY');
  secured.headers.set('Referrer-Policy', 'no-referrer');
  secured.headers.set('Permissions-Policy', 'camera=(), microphone=(), geolocation=(), payment=(), usb=()');
  if (new URL(request.url).protocol === 'https:') secured.headers.set('Strict-Transport-Security', 'max-age=31536000');
  return secured;
}
