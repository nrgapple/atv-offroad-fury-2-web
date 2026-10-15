import { multiplayerRequest, secureResponse } from './security.mjs';
import { usageEvent, webAnalyticsEnabled, withWebAnalytics } from './analytics.mjs';
const BLOB = /^\/content\/[a-f0-9]{64}\.bin$/;
const IMAGE = /^\/devlog-images\/[a-z0-9-]+\.png$/;
const APP_ICON = /^\/app-icons\/[a-f0-9]{64}\.png$/;
const MANIFEST = /^\/content\/manifests\/([a-f0-9]{40})\.json$/;

function canonical(value) {
  if (Array.isArray(value)) return JSON.stringify(value.map(item => JSON.parse(canonical(item))));
  if (value && typeof value === 'object') return JSON.stringify(Object.fromEntries(Object.keys(value).sort().map(key => [key, JSON.parse(canonical(value[key]))])));
  return JSON.stringify(value);
}

async function gallery(request, env) {
  const response = await env.ASSETS.fetch(new Request(request, { method: 'GET' }));
  if (!response.ok) return response;
  const log = await response.json();
  const overlay = await env.CONTENT.get(env.RELEASE_COMMIT ? `releases/${env.RELEASE_COMMIT}/gallery.json` : 'devlog-gallery.json');
  if (overlay) {
    const approved = new Map((await overlay.json()).entries.map(entry => [entry.id, entry]));
    for (const entry of log.entries) {
      const local = approved.get(entry.id);
      if (!local) continue;
      const { image, caption, ...authored } = local;
      if (canonical(authored) === canonical(entry) && IMAGE.test(image)) {
        entry.image = image;
        if (caption) entry.caption = caption;
      }
    }
  }
  return new Response(request.method === 'HEAD' ? null : JSON.stringify(log), {
    headers: { 'Content-Type': 'application/json', 'Cache-Control': 'no-cache', 'X-Content-Type-Options': 'nosniff' },
  });
}

async function route(request, env) {
    const url = new URL(request.url);
    const path = url.pathname;
    if (env.CANONICAL_ORIGIN && url.hostname === `www.${new URL(env.CANONICAL_ORIGIN).hostname}`) {
      const target = new URL(env.CANONICAL_ORIGIN);
      target.pathname = path;
      target.search = url.search;
      return Response.redirect(target.href, 308);
    }
    const allowed = (env.GAME_ORIGINS ?? env.GAME_ORIGIN ?? url.origin).split(',').map(value => value.trim()).filter(Boolean);
    if (path === '/api/events') return usageEvent(request, env, allowed);
    if (path.startsWith('/multiplayer/')) {
      const origin = request.headers.get('Origin');
      if (!origin || !allowed.includes(origin)) return new Response('Origin denied', {status:403});
      if (request.method !== 'GET' && request.method !== 'POST') return new Response('Method not allowed', {status:405});
      if (!env.MULTIPLAYER) return new Response('Multiplayer unavailable', {status:503});
      return multiplayerRequest(request, env);
    }
    if (request.method !== 'GET' && request.method !== 'HEAD') return new Response('Method not allowed', { status: 405, headers: { Allow: 'GET, HEAD' } });
    if (path === '/') {
      const index = new URL(request.url);
      index.pathname = '/index.html';
      return env.ASSETS.fetch(new Request(index, request));
    }
    if (path === '/devlog.json') return gallery(request, env);
    const iconAliasKey = ['/apple-touch-icon.png', '/apple-touch-icon-precomposed.png'].includes(path) ? env.APPLE_TOUCH_ICON_KEY : path === '/app-icon-512.png' ? env.APP_ICON_512_KEY : null;
    if (/^fury-remastered-home-(180|512)-v1\.png$/.test(iconAliasKey ?? '')) {
      const iconUrl = new URL(request.url); iconUrl.pathname = '/' + iconAliasKey;
      return env.ASSETS.fetch(new Request(iconUrl, request));
    }
    const appleIcon = /^app-icons\/[a-f0-9]{64}\.png$/.test(iconAliasKey ?? '');
    if (path === '/content/manifest.json' || MANIFEST.test(path) || BLOB.test(path) || IMAGE.test(path) || APP_ICON.test(path) || appleIcon) {
      const pinned = path.match(MANIFEST);
      const key = appleIcon ? iconAliasKey : pinned ? `releases/${pinned[1]}/manifest.json` : path === '/content/manifest.json' && env.RELEASE_COMMIT ? `releases/${env.RELEASE_COMMIT}/manifest.json` : path.slice(1);
      const object = request.method === 'HEAD' ? await env.CONTENT.head(key) : await env.CONTENT.get(key);
      if (!object) return new Response('Not found', { status: 404 });
      const headers = new Headers();
      object.writeHttpMetadata(headers);
      headers.set('Content-Type', IMAGE.test(path) || APP_ICON.test(path) || appleIcon ? 'image/png' : path.endsWith('.json') ? 'application/json' : 'application/octet-stream');
      headers.set('ETag', object.httpEtag);
      headers.set('Content-Length', String(object.size));
      headers.set('Cache-Control', BLOB.test(path) || APP_ICON.test(path) ? 'public, max-age=31536000, immutable' : IMAGE.test(path) ? 'public, max-age=3600' : 'no-cache');
      headers.set('X-Content-Type-Options', 'nosniff');
      if (request.headers.get('If-None-Match') === object.httpEtag) return new Response(null, { status: 304, headers });
      return new Response(request.method === 'HEAD' ? null : object.body, { headers });
    }
    if (path.startsWith('/content/') || path.startsWith('/devlog-images/') || path.startsWith('/app-icons/')) return new Response('Not found', { status: 404 });
    return env.ASSETS.fetch(request);
}

export default { async fetch(request, env) { return secureResponse(await withWebAnalytics(await route(request, env), request, env), request, { webAnalytics: webAnalyticsEnabled(env) }); } };
