// Host-only gate: King of the Hills in a multiplayer room (research/
// MULTIPLAYER.md "Room games"). A Rust host rides a room on Prospect
// Springs (`?host=1&venue=env05`) and a Rust guest joins by code. The
// host's Pause → Play King of the Hills moves both onto the E05_KH course
// (hill bases and flags) and starts one shared countdown. `gameTest=hill`
// sets each rider on hill 1 and, 2 s later, reports a capture worth 1500
// (host) or 1600 (guest) points: the host checks the guest's claim against
// its relayed position and the hill's score, so the guest holds hill 1
// whichever claim lands first, and both pages show its flag owner. The
// 20 s clock (`gameTest=fast`) ends in the same results; a third rider who
// joins after GO spectates (not in the results). Play Again starts
// game 2 for both; the guest then leaves mid-game and drops out.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_koth.mjs
// ATV_GAMES_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { roomGame } from './room_game_test.mjs';

await roomGame({
  prefix: 'KH',
  action: 'playgame:9',
  hook: 'hill',
  label: 'Multiplayer King of the Hills',
  late: true,
  async verify({ host, guest, until, shot }) {
    const h = await until(host, game => game.markers[0]?.owner === 1 && game.markers[0]?.score === 1600, 60000, 'the guest captures hill 1');
    assert.equal(h.game.kind, 'koth');
    assert.ok(h.roomGame.accepted >= 1, JSON.stringify(h.roomGame));
    assert.deepEqual(h.game.scores.slice(0, 2), [0, 1]);
    await until(guest, game => game.markers[0]?.owner === 1 && game.scores[1] === 1, 30000, 'guest mirrors the owner');
    await shot(guest, 'KH-guest-play');
  }
});
