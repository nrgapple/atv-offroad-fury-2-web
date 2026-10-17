// Anonymous gameplay events for Workers Analytics Engine. Never stores IPs,
// identifiers or request text; the IP keys only the in-memory rate limiter.
import { MAX_EVENT_BYTES, dataPoint, normalizeEvent } from '../web/analytics.js';

const accepted = () => new Response(null, { status: 204, headers: { 'Cache-Control': 'no-store' } });

export async function usageEvent(request, env, allowed) {
  if (request.method !== 'POST') return new Response('Method not allowed', { status: 405, headers: { Allow: 'POST' } });
  // Beacons may omit Origin, and Safari sends `Origin: null` for no-cors beacons
  // under the site's no-referrer policy; fetch metadata still proves a
  // same-origin page sent them.
  const origin = request.headers.get('Origin');
  if (origin && origin !== 'null' ? !allowed.includes(origin) : request.headers.get('Sec-Fetch-Site') !== 'same-origin') return new Response('Origin denied', { status: 403 });
  const length = request.headers.get('Content-Length');
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > MAX_EVENT_BYTES)) return new Response('Event too large', { status: 413 });
  let event;
  try {
    const body = await request.text();
    if (body.length > MAX_EVENT_BYTES) return new Response('Event too large', { status: 413 });
    event = normalizeEvent(JSON.parse(body));
  } catch { event = null; }
  if (!event) return new Response('Invalid event', { status: 400 });
  // Analytics is best effort: missing bindings or limits drop silently so pages never retry.
  if (!env.USAGE) return accepted();
  const ip = request.headers.get('CF-Connecting-IP');
  if (env.EVENT_LIMIT && ip) {
    try { if (!(await env.EVENT_LIMIT.limit({ key: ip })).success) return accepted(); } catch { return accepted(); }
  }
  try { env.USAGE.writeDataPoint(dataPoint(event, { country: request.cf?.country ?? '', release: env.RELEASE_COMMIT ?? env.BUILD_COMMIT ?? '' })); } catch { /* Best effort. */ }
  return accepted();
}

// Cloudflare Web Analytics (page views, referrers, Core Web Vitals) is opt-in
// through the public WEB_ANALYTICS_TOKEN site tag; the CSP widens only then.
const SITE_TOKEN = /^[a-f0-9]{32}$/;
export const webAnalyticsEnabled = env => SITE_TOKEN.test(env?.WEB_ANALYTICS_TOKEN ?? '');

export async function withWebAnalytics(response, request, env) {
  if (!webAnalyticsEnabled(env) || request.method !== 'GET' || response.status !== 200 || !(response.headers.get('Content-Type') ?? '').startsWith('text/html')) return response;
  const html = await response.text();
  const tag = `<script defer src="https://static.cloudflareinsights.com/beacon.min.js" data-cf-beacon='{"token":"${env.WEB_ANALYTICS_TOKEN}"}'></script>`;
  const headers = new Headers(response.headers);
  headers.delete('Content-Length'); headers.delete('ETag');
  return new Response(html.includes('</body>') ? html.replace('</body>', `${tag}\n</body>`) : html + tag, { status: 200, headers });
}
