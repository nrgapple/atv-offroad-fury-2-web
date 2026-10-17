// Registers the offline cache (sw.js) for packaged releases. See research/OFFLINE.md.
// Unversioned development builds and the local browser-test server skip it: their
// app files have no per-deploy cache key, so a cache-first shell would go stale.
const RELEASE = /^\?v=[a-f0-9]{40}$/;
const CONTENT = /^\/content\/(?:[a-f0-9]{64}\.bin|manifest\.json|manifests\/[a-f0-9]{40}\.json)$/;

export const releaseBuild = moduleUrl => RELEASE.test(new URL(moduleUrl).search);

export const standalone = win => win.navigator.standalone === true
  || ['standalone', 'fullscreen'].some(mode => win.matchMedia?.(`(display-mode: ${mode})`).matches);

// Content requests the page made without the service worker (workerStart 0),
// e.g. before it took control. The worker re-fetches and caches these.
export function uncachedContent(entries, origin) {
  const urls = [];
  for (const entry of entries) {
    if (entry.workerStart > 0) continue;
    let url;
    try { url = new URL(entry.name); } catch { continue; }
    if (url.origin === origin && !url.search && CONTENT.test(url.pathname)) urls.push(url.href);
  }
  return urls;
}

// Ask the browser not to evict the cache. Installed apps are the case that
// matters (and the case browsers usually grant); a refusal changes nothing.
export async function persistStorage(win) {
  const storage = win.navigator.storage;
  if (!standalone(win) || !storage?.persist) return false;
  try { return (await storage.persisted?.()) || await storage.persist(); } catch { return false; }
}

function reportUncachedContent(win, container) {
  const pending = new Set();
  let timer = null;
  const flush = async () => {
    timer = null;
    const urls = [...pending];
    pending.clear();
    try { (await container.ready).active?.postMessage({ type: 'fury-warm', urls }); } catch { /* Best effort. */ }
  };
  const queue = entries => {
    for (const href of uncachedContent(entries, win.location.origin)) pending.add(href);
    if (pending.size && !timer) timer = win.setTimeout(flush, 2000);
  };
  try {
    new win.PerformanceObserver(list => queue(list.getEntries())).observe({ type: 'resource', buffered: true });
  } catch {
    queue(win.performance?.getEntriesByType?.('resource') ?? []);
  }
}

export async function registerOffline(win = globalThis.window, moduleUrl = import.meta.url) {
  const container = win?.navigator?.serviceWorker;
  if (!container || !releaseBuild(moduleUrl)) return null;
  // Observe from startup; reports wait for the worker to become active.
  reportUncachedContent(win, container);
  if (win.document.readyState !== 'complete') await new Promise(resolve => win.addEventListener('load', resolve, { once: true }));
  let registration;
  try { registration = await container.register('/sw.js', { scope: '/' }); } catch { return null; }
  void persistStorage(win);
  return registration;
}

if (typeof window !== 'undefined' && typeof document !== 'undefined') void registerOffline();
