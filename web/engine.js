// The whole game runs in Rust (crates/atv-app); this only starts the module.
import init from './atv_app.js';
// Pointer events drive the game (winit). Cancel native touch gestures so
// Safari's loupe, selection, scrolling and pinch zoom never take a riding thumb.
const cancel = event => { if (event.cancelable) event.preventDefault(); };
for (const type of ['touchstart', 'touchmove', 'gesturestart', 'gesturechange', 'gestureend']) document.addEventListener(type, cancel, { passive: false });
for (const type of ['selectstart', 'contextmenu']) document.addEventListener(type, cancel);
// F3 cycles the engine's diagnostics overlay (Off, Performance, Debug; drawn
// by the Rust UI, ?debug=1 or ?debug=2 to open it). Keep the browser's own F3.
addEventListener('keydown', event => { if (event.code === 'F3') event.preventDefault(); }, { capture: true });
init().catch(error => {
  const status = document.getElementById('status');
  status.textContent = `Engine failed: ${error}`;
  status.classList.add('fatal');
});
