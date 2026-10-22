// The whole game runs in Rust (crates/atv-app); this only starts the module.
import init from './atv_app.js';
// Pointer events drive the game (winit). Cancel native touch gestures so
// Safari's loupe, selection, scrolling and pinch zoom never take a riding thumb.
const cancel = event => { if (event.cancelable) event.preventDefault(); };
for (const type of ['touchstart', 'touchmove', 'gesturestart', 'gesturechange', 'gestureend']) document.addEventListener(type, cancel, { passive: false });
for (const type of ['selectstart', 'contextmenu']) document.addEventListener(type, cancel);
// The debug strip (#status) shows with ?debug=1; F3 toggles it.
const params = new URLSearchParams(location.search);
document.documentElement.classList.toggle('debug', params.has('debug') && params.get('debug') !== '0');
addEventListener('keydown', event => {
  if (event.code === 'F3' && !event.repeat) { event.preventDefault(); document.documentElement.classList.toggle('debug'); }
}, { capture: true });
init().catch(error => {
  const status = document.getElementById('status');
  status.textContent = `Engine failed: ${error}`;
  status.classList.add('fatal');
});
