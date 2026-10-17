// Keeps the screen awake while riding. Controller and keyboard play produce no
// touches, so phones and tablets would otherwise dim and lock mid-ride. Saved
// iOS Home Screen apps support the Screen Wake Lock API from iOS 18.4.
// The browser releases the lock whenever the page is hidden; it is requested
// again on return, or on the next gesture if the browser wanted an activation.
export function createScreenWakeLock(nav = globalThis.navigator, doc = globalThis.document) {
  const supported = typeof nav?.wakeLock?.request === 'function';
  let wanted = false, sentinel = null, pending = null;
  const visible = () => doc?.visibilityState !== 'hidden';
  const acquire = () => {
    if (!supported || !wanted || sentinel || pending || !visible()) return pending ?? Promise.resolve();
    pending = Promise.resolve().then(() => nav.wakeLock.request('screen')).then(lock => {
      pending = null;
      if (!wanted || !visible()) { void Promise.resolve(lock.release?.()).catch(() => {}); return; }
      sentinel = lock;
      lock.addEventListener?.('release', () => { if (sentinel === lock) sentinel = null; });
    }, () => { pending = null; });
    return pending;
  };
  const release = () => {
    const lock = sentinel; sentinel = null;
    if (lock) void Promise.resolve(lock.release?.()).catch(() => {});
  };
  if (supported) {
    doc?.addEventListener?.('visibilitychange', () => { if (visible()) void acquire(); else sentinel = null; });
    for (const type of ['pointerup', 'keydown']) doc?.addEventListener?.(type, () => { if (wanted && !sentinel) void acquire(); }, { passive: true });
  }
  return {
    supported,
    get held() { return Boolean(sentinel); },
    set(value) {
      wanted = Boolean(value);
      if (wanted) return acquire();
      release();
      return Promise.resolve();
    },
  };
}
