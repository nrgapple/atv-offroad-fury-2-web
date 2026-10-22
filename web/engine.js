// The whole game runs in Rust (crates/atv-app); this only starts the module
// and bridges the page shell: anonymous usage events (analytics.js).
import init from './atv_app.js';
import { UsageAnalytics, deviceContext } from './analytics.js';
// window.furyUsage: the Rust engine reports rides through it (lib.rs `usage`).
// Tests may set window.furyUsageSend before load to receive the events.
const usage = typeof window.furyUsageSend === 'function'
  ? new UsageAnalytics({ send: window.furyUsageSend, enabled: true, context: deviceContext() })
  : new UsageAnalytics();
window.furyUsage = usage;
document.addEventListener('visibilitychange', () => { usage.setVisible(!document.hidden); if (document.hidden) usage.report(1); });
addEventListener('pagehide', () => usage.report(1));
// Pointer events drive the game (winit). Cancel native touch gestures so
// Safari's loupe, selection, scrolling and pinch zoom never take a riding thumb;
// the install banner and its help dialog keep their native taps.
const shell = event => event.target instanceof Element && event.target.closest('#app-install-banner, #app-install-help');
const cancel = event => { if (event.cancelable && !shell(event)) event.preventDefault(); };
for (const type of ['touchstart', 'touchmove', 'gesturestart', 'gesturechange', 'gestureend']) document.addEventListener(type, cancel, { passive: false });
for (const type of ['selectstart', 'contextmenu']) document.addEventListener(type, cancel);
// F3 cycles the engine's diagnostics overlay (Off, Performance, Debug; drawn
// by the Rust UI, ?debug=1 or ?debug=2 to open it). Keep the browser's own F3.
addEventListener('keydown', event => { if (event.code === 'F3') event.preventDefault(); }, { capture: true });
// A lost WebGL context (iOS backgrounding, GPU memory pressure) leaves every
// GPU resource invalid. Ask the browser to restore it, say so, then reopen the
// current ride's link (the engine keeps it in `fury-resume`; a room's invite
// rejoins its slot) or this page. game.js rebuilt in place instead.
const status = () => document.getElementById('status');
document.addEventListener('webglcontextlost', event => {
  event.preventDefault();
  status().textContent = 'Restoring graphics…';
  status().classList.add('fatal');
}, true);
document.addEventListener('webglcontextrestored', () => {
  let resume = null;
  try { resume = sessionStorage.getItem('fury-resume'); } catch { /* storage blocked */ }
  location.replace(resume || location.href);
}, true);
init().catch(error => {
  const status = document.getElementById('status');
  status.textContent = `Engine failed: ${error}`;
  status.classList.add('fatal');
});
