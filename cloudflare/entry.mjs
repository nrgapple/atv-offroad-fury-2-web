// The existing game Worker hosts the compiled Rust room service and Durable
// Object. Workers-rs exports a WorkerEntrypoint class; inherit its Rust fetch.
import app from './worker.mjs';
import RustRelay, { LobbyRoom } from '../work/cloudflare/relay/index.js';
export { LobbyRoom };
export { LobbyAdmission } from './security.mjs';
export default class GameWorker extends RustRelay {
  fetch(request) {
    return app.fetch(request, {
      ...this.env,
      MULTIPLAYER: { fetch: forwarded => super.fetch(forwarded) },
    }, this.ctx);
  }
}
