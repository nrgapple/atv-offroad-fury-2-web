// The whole game runs in Rust (crates/atv-app); this only starts the module.
import init from './atv_app.js';
// Pointer events drive the game (winit). Cancel native touch gestures so
// Safari's loupe, selection, scrolling and pinch zoom never take a riding thumb.
const cancel = event => { if (event.cancelable) event.preventDefault(); };
for (const type of ['touchstart', 'touchmove', 'gesturestart', 'gesturechange', 'gestureend']) document.addEventListener(type, cancel, { passive: false });
for (const type of ['selectstart', 'contextmenu']) document.addEventListener(type, cancel);
init().catch(error => { document.getElementById('status').textContent = `Engine failed: ${error}`; });
