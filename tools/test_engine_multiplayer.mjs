// Host-only gate: the Rust engine's multiplayer client in a room with the
// JavaScript game, on a local relay with original content.
//   1. Native `fury --host` creates a room on env01 and keeps it running.
//   2. A JavaScript guest (the pinned reference at ATV_JS_URL) opens play.html?room=CODE;
//      both must see each other.
//   3. A Rust browser guest opens play.html?room=CODE; all three see each other.
//   4. The JavaScript guest drives; the Rust browser guest sees it move.
//   5. Native fury captures its view with both remote riders and reports.
// Needs the relay stack of BROWSER_TESTS.md (relay group) and a native build:
//   FURY_BIN=target/release/fury FURY_CONTENT=private/reports/hosted-content \
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer.mjs
// ATV_MP_SCREENSHOT=out.png also saves the browser pages and the native capture.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { assertNoErrors, base, holdKey, jsBase, launch, watchErrors } from './browser/harness.mjs';

const fury = process.env.FURY_BIN;
const content = process.env.FURY_CONTENT ?? 'private/reports/hosted-content';
assert.ok(fury, 'FURY_BIN must name a native fury build');
const shots = process.env.ATV_MP_SCREENSHOT;
const nativeShot = shots ? shots.replace('.png', '-native.png') : '/dev/null';
const report = {};
const errors = [];
const distance = (a, b) => Math.hypot(...a.map((v, i) => v - b[i]));

// 1. Native host: waits up to 150 s for two other riders, then captures.
const native = spawn(
  fury,
  [
    '--content',
    content,
    '--host',
    '--relay',
    base,
    '--name',
    'Native',
    '--venue',
    'env01',
    '--capture',
    nativeShot,
    '1280x720'
  ],
  { env: { ...process.env, FURY_ROOM_WAIT: '150', FURY_ROOM_PEERS: '2' } }
);
let output = '';
native.stdout.on('data', d => (output += d));
native.stderr.on('data', d => (output += d));
const exited = new Promise(resolve => native.on('exit', resolve));
const code = await new Promise((resolve, reject) => {
  const timer = setInterval(() => {
    const match = output.match(/Multiplayer room ([A-Z0-9]{8})/);
    if (match) {
      clearInterval(timer);
      resolve(match[1]);
    }
  }, 100);
  exited.then(status => reject(new Error(`fury exited (${status}) before hosting:\n${output}`)));
});
report.code = code;

const browser = await launch();
async function page(viewport = { width: 1280, height: 720 }) {
  const context = await browser.newContext({ viewport, hasTouch: true });
  const p = await context.newPage();
  watchErrors(p, errors);
  await p.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  return p;
}
const jsRemote = p =>
  p.evaluate(() => (window.atvTerrainStatus?.multiplayer?.remote ?? []).filter(r => !r.stale).length);
const engineState = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));

try {
  // 2. JavaScript guest by room link.
  const js = await page();
  await js.goto(`${jsBase}/play.html?room=${code}&quad=14&rider=1&player=JsGuest`);
  await js.waitForFunction(
    () => {
      const s = window.atvTerrainStatus;
      return (
        s?.multiplayerError || (s?.simulation && s.multiplayer?.remote?.some(r => !r.stale) && s.multiplayerDraws > 0)
      );
    },
    null,
    { timeout: 150000 }
  );
  let s = await js.evaluate(() => window.atvTerrainStatus);
  assert.equal(s.multiplayerError, undefined, s.multiplayerError);
  report.js = { peer: s.multiplayer.state.peer, remote: await jsRemote(js), draws: s.multiplayerDraws };

  // 3. Rust browser guest (WebGL2) by room link.
  const web = await page();
  await web.goto(`${base}/play.html?room=${code}&name=WebGuest&quad=5`);
  await web.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 2, null, {
    timeout: 150000
  });
  let e = await engineState(web);
  assert.equal(e.code, code);
  assert.equal(e.closed, null);
  report.engine = { peer: e.peer, drawn: e.drawn, players: e.players, rttMs: e.rttMs };
  await js.waitForFunction(
    () => (window.atvTerrainStatus?.multiplayer?.remote ?? []).filter(r => !r.stale).length >= 2,
    null,
    { timeout: 60000 }
  );
  report.js.remoteWithEngine = await jsRemote(js);

  // 4. The JavaScript guest drives; the Rust browser guest sees it move.
  const jsPeer = report.js.peer;
  const before = (await engineState(web)).remote.find(r => r.peer === jsPeer).position;
  await js.bringToFront();
  await holdKey(js, 'KeyW', 240);
  await web.waitForTimeout(500);
  e = await engineState(web);
  const after = e.remote.find(r => r.peer === jsPeer).position;
  report.engine.jsMovedFt = distance(before, after);
  assert.ok(
    report.engine.jsMovedFt > 10,
    `JS rider motion should reach the Rust guest (${report.engine.jsMovedFt} ft)`
  );
  report.engine.network = e.network;
  if (shots) {
    await js.screenshot({ path: shots.replace('.png', '-js.png') });
    await web.screenshot({ path: shots.replace('.png', '-engine.png') });
  }

  // 5. Native capture with both remote riders drawn.
  const status = await exited;
  assert.equal(status, 0, output);
  const line = output.split('\n').find(l => l.startsWith('multiplayer '));
  assert.ok(line, output);
  report.native = JSON.parse(line.slice('multiplayer '.length));
  assert.ok(report.native.drawn >= 2, `native should draw both remote riders: ${line}`);
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  native.kill();
  await browser.close();
}
