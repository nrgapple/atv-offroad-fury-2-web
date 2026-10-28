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

// Saved progress on this device: a profile book or a Pass Key (research/CLOUD_SAVE.md).
export function savedProgress(win) {
  try { return ['fury-pass-key', 'fury-profiles'].some(key => win.localStorage?.getItem(key)); } catch { return false; }
}

// Ask the browser not to evict the cache and the saves. Installed apps, and any
// tab once a profile is saved (a cleared save loses progress that has not
// synced to its Pass Key yet); a refusal changes nothing.
export async function persistStorage(win) {
  const storage = win.navigator.storage;
  if (!(standalone(win) || savedProgress(win)) || !storage?.persist) return false;
  try { return (await storage.persisted?.()) || await storage.persist(); } catch { return false; }
}

// A start that failed (the module or its WASM did not load, e.g. a transfer
// dropped on a cellular network) reloads once. Under a service worker (a
// returning visitor: an older worker, its caches, or a transfer it passed on)
// it first unregisters it, so the reload loads straight from the network and
// the next load registers the current worker. The flag lasts for the tab's
// session and is cleared by a successful start, so a page that fails again shows
// its error instead of reloading in a loop. Returns true when reloading.
const RECOVERY = 'fury-start-recovery';

export async function recoverFailedStart(win = globalThis.window) {
  if (!win?.location) return false;
  try {
    if (win.sessionStorage.getItem(RECOVERY)) return false;
    win.sessionStorage.setItem(RECOVERY, '1');
  } catch {
    return false;
  }
  const container = win.navigator?.serviceWorker;
  try {
    if (container?.controller) for (const registration of await container.getRegistrations()) await registration.unregister();
  } catch { /* Reload anyway: a fresh load may still succeed. */ }
  win.location.reload();
  return true;
}

export function startSucceeded(win = globalThis.window) {
  try { win?.sessionStorage?.removeItem(RECOVERY); } catch { /* Storage may be unavailable. */ }
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
