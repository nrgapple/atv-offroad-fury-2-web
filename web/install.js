// Installed iOS can expose a fixed/percentage viewport shorter than the screen.
// Only compensate a small system-inset gap in a full-width, unzoomed app.
export function gameViewport(win, standalone, apple) {
  const vv = win.visualViewport, scale = vv?.scale ?? 1;
  let width = scale === 1 ? (vv?.width ?? win.innerWidth) : win.innerWidth;
  let height = scale === 1 ? (vv?.height ?? win.innerHeight) : win.innerHeight;
  if (standalone && apple && scale === 1) {
    const screen = win.screen;
    const candidates = [[screen?.width, screen?.height], [screen?.height, screen?.width]];
    const full = candidates.find(([w, h]) => Math.abs(w - width) <= 2 && h >= height && h - height <= 128);
    if (full) [width, height] = full;
  }
  return { width: Math.max(1, Math.round(width)), height: Math.max(1, Math.round(height)) };
}
export function mountGameViewport(win = window, doc = document) {
  if (!doc.getElementById('game')) return;
  const update = () => {
    const standalone = win.navigator.standalone === true || win.matchMedia('(display-mode: standalone)').matches || win.matchMedia('(display-mode: fullscreen)').matches;
    const { width, height } = gameViewport(win, standalone, isAppleMobile(win.navigator));
    const style = doc.documentElement.style;
    style.setProperty('--app-width', `${width}px`);
    style.setProperty('--app-height', `${height}px`);
  };
  for (const name of ['resize', 'orientationchange', 'pageshow', 'appdisplaychange']) win.addEventListener(name, update);
  win.visualViewport?.addEventListener('resize', update);
  update();
}

// iOS installation belongs to Safari's Share menu. Only offer a native prompt
// after the browser supplies beforeinstallprompt; a Share API call cannot install.
export const INSTALL_DISMISS_KEY = 'fury-app-install-dismissed-until';
export const INSTALL_DISMISS_MS = 7 * 24 * 60 * 60 * 1000;

export function isAppleMobile(navigator) {
  return /iPhone|iPad|iPod/.test(navigator.userAgent) ||
    (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
}
export function isDismissed(read, now = Date.now()) {
  try { const until = Number(read()); return Number.isFinite(until) && until > now && until <= now + INSTALL_DISMISS_MS; }
  catch { return false; }
}

export function mountInstallBanner(win = window, doc = document) {
  const banner = doc.getElementById('app-install-banner');
  const dialog = doc.getElementById('app-install-help');
  const frontend = doc.getElementById('frontend');
  if (!banner || !dialog || !frontend) return;
  const open = doc.getElementById('app-install-open');
  const standalone = win.matchMedia('(display-mode: standalone)');
  const fullscreen = win.matchMedia('(display-mode: fullscreen)');
  const apple = isAppleMobile(win.navigator);
  let installed = false, prompt = null, dismissed = isDismissed(() => win.localStorage.getItem(INSTALL_DISMISS_KEY));
  let returnFocus = null;
  const runningStandalone = () => win.navigator.standalone === true || standalone.matches || fullscreen.matches;
  const appMode = () => installed || runningStandalone();
  const eligible = () => !appMode() && !dismissed && frontend.dataset.page === 'title' && !frontend.hidden && !frontend.dataset.error;
  const update = () => {
    const mode = runningStandalone();
    if (doc.documentElement.classList.contains('app-standalone') !== mode) {
      doc.documentElement.classList.toggle('app-standalone', mode);
      win.dispatchEvent(new win.Event('appdisplaychange'));
    }
    banner.hidden = !eligible() || !(apple || prompt);
    open.textContent = prompt ? 'Install' : 'Add app';
    if ((!eligible() || appMode()) && dialog.open) dialog.close();
  };
  const dismiss = () => {
    dismissed = true;
    try { win.localStorage.setItem(INSTALL_DISMISS_KEY, String(Date.now() + INSTALL_DISMISS_MS)); } catch { /* private browsing: session-only dismissal */ }
    update();
  };
  const instructions = () => {
    const steps = apple ? [
      'Open Safari’s Share menu. In the compact layout, tap the page menu beside the address first, then Share.',
      'Scroll down and choose Add to Home Screen.',
      'Keep Open as Web App enabled if shown, then tap Add.',
    ] : ['Open your browser’s menu.', 'Choose Install app or Add to Home Screen, if available.', 'Confirm, then launch Fury Remastered from its app icon.'];
    if (apple && /CriOS|FxiOS|EdgiOS|OPiOS|DuckDuckGo/.test(win.navigator.userAgent)) {
      steps[0] = 'Use your browser’s Share menu. If Add to Home Screen is missing, open furyremastered.com in Safari and tap Share there.';
    }
    doc.getElementById('app-install-steps').replaceChildren(...steps.map(text => { const item = doc.createElement('li'); item.textContent = text; return item; }));
    returnFocus = doc.activeElement;
    dialog.showModal();
  };
  open.addEventListener('click', async () => {
    if (!prompt) { instructions(); return; }
    const pending = prompt; prompt = null; open.disabled = true;
    try {
      await pending.prompt();
      const choice = await pending.userChoice;
      if (choice.outcome === 'accepted') installed = true;
      else dismiss();
    } catch { instructions(); }
    finally { open.disabled = false; update(); }
  });
  doc.getElementById('app-install-dismiss').addEventListener('click', dismiss);
  doc.getElementById('app-install-close').addEventListener('click', () => dialog.close());
  dialog.addEventListener('close', () => { if (returnFocus?.isConnected && !banner.hidden) returnFocus.focus({ preventScroll: true }); });
  // Native buttons/dialog must not also activate the game's window key handlers.
  for (const element of [banner, dialog]) for (const type of ['keydown', 'keyup']) element.addEventListener(type, event => event.stopPropagation());
  win.addEventListener('beforeinstallprompt', event => { event.preventDefault(); prompt = event; update(); });
  win.addEventListener('appinstalled', () => { installed = true; prompt = null; update(); });
  win.addEventListener('pageshow', update);
  for (const query of [standalone, fullscreen]) query.addEventListener('change', update);
  const observer = new win.MutationObserver(update);
  observer.observe(frontend, { attributes: true, attributeFilter: ['hidden', 'data-page', 'data-error'] });
  update();
  return { update };
}
if (typeof window !== 'undefined' && typeof document !== 'undefined') { mountGameViewport(); mountInstallBanner(); }
