// Host-only gate: a guest arrives beside a host whose app has stopped
// sending (a background tab's throttled timers, here a native host stopped
// with SIGSTOP after riding alone for a few seconds).
//   1. Native `fury --host` rides env01 alone; its presence pose reaches the
//      relay even with nobody to see it.
//   2. The host is stopped; a Rust browser guest joins through Join Lobby.
//   3. The guest meets the host from the relay's copy of its pose instead of
//      starting on top of it at the start line.
// Needs the relay stack of BROWSER_TESTS.md (relay group) and a native build:
//   FURY_BIN=target/release/fury FURY_CONTENT=private/reports/hosted-content \
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_idle_host.mjs
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { assertNoErrors, base, launch, watchErrors } from './browser/harness.mjs';

const fury = process.env.FURY_BIN;
const content = process.env.FURY_CONTENT ?? 'private/reports/hosted-content';
assert.ok(fury, 'FURY_BIN must name a native fury build');
const errors = [];
const report = {};
const code = `ID${String(Date.now()).slice(-6)}`;
const native = spawn(
  fury,
  ['--content', content, '--host', '--room', code, '--relay', base, '--name', 'Native', '--venue', 'env01', '--mute'],
  { stdio: ['ignore', 'pipe', 'pipe'] }
);
let output = '';
native.stdout.on('data', d => (output += d));
native.stderr.on('data', d => (output += d));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const browser = await launch();
try {
  // 1. The host rides alone for a few seconds.
  for (let i = 0; i < 300 && !output.includes('Multiplayer room'); i++) await sleep(200);
  assert.ok(output.includes(`Multiplayer room ${code}`), output);
  await sleep(4000);
  // 2. Stopped: it sends nothing more until resumed.
  process.kill(native.pid, 'SIGSTOP');
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  const guest = await context.newPage();
  watchErrors(guest, errors);
  await guest.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  await guest.goto(`${base}/play.html?mode=multiplayer&lobby=${code}&name=Guest&play=1&uiDiagnostics=1`);
  // 3. The guest meets the stopped host.
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.met, null, { timeout: 180000 });
  const e = await guest.evaluate(() => JSON.parse(window.furyMultiplayer));
  report.met = e.met;
  report.host = e.remote.find(r => r.peer === e.met.peer)?.position;
  assert.equal(e.met.peer, 0, 'the guest meets the host');
  assert.ok(e.met.distanceFt > 4 && e.met.distanceFt < 40, `beside the host: ${JSON.stringify(e.met)}`);
  process.kill(native.pid, 'SIGCONT');
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  try {
    process.kill(native.pid, 'SIGCONT');
  } catch {
    /* Already gone. */
  }
  native.kill();
  await browser.close();
}
