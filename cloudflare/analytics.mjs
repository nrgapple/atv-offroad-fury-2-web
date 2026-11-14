// Anonymous gameplay events for Workers Analytics Engine. Never stores IPs,
// identifiers or request text; the IP keys only the in-memory rate limiter
// and the excluded-address check.
import { MAX_EVENT_BYTES, dataPoint, normalizeEvent } from '../web/analytics.js';

const accepted = () => new Response(null, { status: 204, headers: { 'Cache-Control': 'no-store' } });

// An address as [value, bits] (BigInt, 32 or 128), or null: IPv4 dotted
// quads and IPv6 with :: and an embedded IPv4 tail. An IPv4-mapped IPv6
// address compares as IPv4, with its prefix 96 bits shorter.
function address(text) {
  const v4 = groups => (groups.length === 4 && groups.every(g => /^\d{1,3}$/.test(g) && Number(g) < 256)
    ? groups.reduce((n, g) => (n << 8n) | BigInt(g), 0n) : null);
  if (!text.includes(':')) { const n = v4(text.split('.')); return n === null ? null : [n, 32, 0]; }
  const last = text.lastIndexOf(':');
  let tail = [];
  if (text.includes('.', last)) {
    const n = v4(text.slice(last + 1).split('.'));
    if (n === null) return null;
    tail = [(n >> 16n).toString(16), (n & 0xffffn).toString(16)];
    text = text.slice(0, last + 1) + tail.join(':');
  }
  const halves = text.split('::');
  if (halves.length > 2) return null;
  const [head, rest = null] = halves.map(half => (half ? half.split(':') : []));
  const groups = rest === null ? head : [...head, ...Array(8 - head.length - rest.length).fill('0'), ...rest];
  if (groups.length !== 8 || (rest !== null && head.length + rest.length > 7) || !groups.every(g => /^[0-9a-f]{1,4}$/i.test(g))) return null;
  const n = groups.reduce((sum, g) => (sum << 16n) | BigInt(`0x${g}`), 0n);
  return n >> 32n === 0xffffn ? [n & 0xffffffffn, 32, 96] : [n, 128, 0];
}

// ANALYTICS_EXCLUDED_IPS: comma-separated addresses or CIDR blocks (IPv4 or
// IPv6, e.g. an IPv6 /64) whose events are dropped: the owner's test machines.
// Invalid entries are ignored. The parsed list is kept per isolate.
let excludedText = null, excludedNets = [];
function networks(list) {
  return list.split(',').map(entry => entry.trim()).filter(Boolean).flatMap(entry => {
    const [host, prefix, extra] = entry.split('/');
    const net = address(host);
    if (!net || extra !== undefined || (prefix !== undefined && !/^\d{1,3}$/.test(prefix))) return [];
    const width = prefix === undefined ? net[1] : Number(prefix) - net[2];
    return width >= 0 && width <= net[1] ? [{ bits: net[1], shift: BigInt(net[1] - width), value: net[0] >> BigInt(net[1] - width) }] : [];
  });
}

export function excludedAddress(ip, list) {
  if (!ip || !list) return false;
  if (list !== excludedText) { excludedText = list; excludedNets = networks(list); }
  const own = address(ip.trim());
  return Boolean(own) && excludedNets.some(net => net.bits === own[1] && own[0] >> net.shift === net.value);
}

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
  // Excluded addresses are compared here, in memory only, and never written.
  if (excludedAddress(ip, env.ANALYTICS_EXCLUDED_IPS)) return accepted();
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
