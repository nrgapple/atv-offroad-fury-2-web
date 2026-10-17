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
    // A mobile layout viewport grows to fit an explicitly sized page. Release the
    // previous size before measuring so narrowing and rotation can shrink it.
    const style = doc.documentElement.style;
    style.removeProperty('--app-width'); style.removeProperty('--app-height'); void doc.documentElement.offsetWidth;
    const { width, height } = gameViewport(win, standalone, isAppleMobile(win.navigator));
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
// iOS shows an apple-touch-startup-image while a saved web app launches, and a
// blank screen without one. It only uses an image matching the device's exact
// pixel size, so rather than ship dozens of per-device PNGs, Safari renders this
// device's portrait and landscape screens (brand gradient + authored wordmark)
// shortly after the title appears. Add to Home Screen reads them from the page.
export const LAUNCH_SCREEN_DELAY_MS = 2500;
export function launchScreenSizes(screen, pixelRatio) {
  const ratio = Math.min(4, Math.max(1, Number(pixelRatio) || 1));
  const short = Math.min(screen.width, screen.height), long = Math.max(screen.width, screen.height);
  if (!(short > 0) || long > 2000) return [];
  const px = value => Math.round(value * ratio);
  return [
    { orientation: 'portrait', width: px(short), height: px(long) },
    { orientation: 'landscape', width: px(long), height: px(short) },
  ];
}
export function drawLaunchScreen(canvas, logo, { width, height }) {
  canvas.width = width; canvas.height = height;
  const context = canvas.getContext('2d');
  const gradient = context.createLinearGradient(0, 0, 0, height);
  gradient.addColorStop(0, '#a61818'); gradient.addColorStop(0.48, '#740a0a'); gradient.addColorStop(1, '#2a0202');
  context.fillStyle = gradient; context.fillRect(0, 0, width, height);
  // The 3:1 wordmark spans 72% of a portrait width, 42% of a landscape width.
  const logoWidth = Math.min(width * (height > width ? 0.72 : 0.42), height * 0.9 * 3);
  context.drawImage(logo, (width - logoWidth) / 2, (height - logoWidth / 3) / 2, logoWidth, logoWidth / 3);
  return canvas.toDataURL('image/png');
}
export function mountLaunchScreens(win = window, doc = document, delay = LAUNCH_SCREEN_DELAY_MS) {
  const standalone = win.navigator.standalone === true || win.matchMedia('(display-mode: standalone)').matches;
  if (!isAppleMobile(win.navigator) || standalone || doc.querySelector('link[rel="apple-touch-startup-image"]')) return null;
  // Render on the idle title screen only, never over loading or a ride.
  const idle = () => { const frontend = doc.getElementById('frontend'); return !frontend || (frontend.dataset.page === 'title' && !frontend.hidden); };
  return new Promise(resolve => win.setTimeout(async function render(attempt = 0) {
    if (!idle()) { if (attempt < 60) win.setTimeout(() => render(attempt + 1), 1000); else resolve(false); return; }
    try {
      const logo = new win.Image();
      logo.src = '/fury-remastered.svg';
      await logo.decode();
      for (const size of launchScreenSizes(win.screen, win.devicePixelRatio)) {
        const link = doc.createElement('link');
        link.rel = 'apple-touch-startup-image';
        link.media = `(orientation: ${size.orientation})`;
        link.dataset.size = `${size.width}x${size.height}`;
        link.href = drawLaunchScreen(doc.createElement('canvas'), logo, size);
        doc.head.append(link);
      }
      resolve(true);
    } catch { resolve(false); /* Launch screens are cosmetic. */ }
  }, delay));
}
if (typeof window !== 'undefined' && typeof document !== 'undefined') { mountGameViewport(); mountInstallBanner(); void mountLaunchScreens(); }
