// Host-only gate: Treasure Hunt in a multiplayer room (research/
// MULTIPLAYER.md "Room games"). A Rust host rides a room on Prospect
// Springs (`?host=1&venue=env05`) and a Rust guest joins by code. The
// host's Pause → Play Treasure Hunt moves both onto the E05_TH course
// (the mode TOY's treasures and bombs) and starts one shared countdown.
// `gameTest=visit` sets each rider on the next standing treasure each
// second: the host collects for both riders (the guest's from its relayed
// position), both pages show the same counts, and the 20 s clock
// (`gameTest=fast`) ends in the same results. Play Again starts game 2 for
// both; the guest then leaves mid-game and drops out of the results.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_treasure.mjs
// ATV_GAMES_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { roomGame } from './room_game_test.mjs';

await roomGame({
  prefix: 'TH',
  action: 'playgame:12',
  hook: 'visit',
  label: 'Multiplayer Treasure Hunt',
  async verify({ host, guest, until, shot }) {
    const h = await until(host, game => game.scores[0] > 0 && game.scores[1] > 0, 60000, 'both collect');
    assert.equal(h.game.kind, 'treasure');
    await until(guest, game => game.scores[1] >= h.game.scores[1] && game.scores[0] >= h.game.scores[0], 30000, 'guest mirrors the counts');
    await shot(guest, 'TH-guest-play');
  }
});
