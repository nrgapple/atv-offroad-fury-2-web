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
// A lost WebGL context is the engine's: it rebuilds in place once the browser
// restores it (crates/atv-app/src/context.rs).
// A WebAssembly trap (out of memory, unreachable) stops the engine without a
// Rust panic message: show it rather than a frozen frame.
addEventListener('error', event => {
  const error = event.error;
  if (!(error instanceof WebAssembly.RuntimeError) && !/memory|unreachable|wasm/i.test(event.message ?? '')) return;
  const line = document.getElementById('status');
  if (line.classList.contains('fatal')) return;
  document.getElementById('loading-screen')?.setAttribute('hidden', '');
  line.textContent = `The game stopped: ${error?.message ?? event.message}. Reload to try again.`;
  line.classList.add('fatal');
});
// A module that will not start closes the boot loader and shows its error
// on the notice line, as the JavaScript game did for a startup failure.
init().catch(error => {
  const text = `Engine failed: ${error?.message ?? error}`;
  document.getElementById('status').textContent = text;
  document.getElementById('loading-screen')?.setAttribute('hidden', '');
  document.getElementById('frontend')?.setAttribute('data-error', 'true');
  document.getElementById('menu-notice').textContent = text;
});
