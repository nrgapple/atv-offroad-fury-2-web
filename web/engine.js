// The whole game runs in Rust (crates/atv-app); this only starts the module.
import init from './atv_app.js';
init().catch(error => { document.getElementById('status').textContent = `Engine failed: ${error}`; });
