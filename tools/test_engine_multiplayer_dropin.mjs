// Host-only gate: Rust engine drop-in multiplayer against the JavaScript
// game on a local relay with original content.
//   1. A JavaScript host rides env01 from the menus.
//   2. A Rust browser guest (play.html?room=) arrives beside the host.
//   3. Its socket drops (?dropAfter test hook) and it reclaims its slot; a
//      reload reclaims it from session storage.
//   4. The host moves the room to another course; the Rust guest follows.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_dropin.mjs
// ATV_MP_SCREENSHOT=out.png saves the pages after the arrival and the move.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, jsBase, launch, watchErrors } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const report = {};
const errors = [];
const distance = (a, b) => Math.hypot(...a.map((v, i) => v - b[i]));
const browser = await launch();
async function page(viewport = { width: 1280, height: 720 }) {
  const context = await browser.newContext({ viewport, hasTouch: true });
  const p = await context.newPage();
  watchErrors(p, errors);
  await p.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  return p;
}
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const jsState = p => p.evaluate(() => window.atvTerrainStatus);
const jsRiding = (p, map) =>
  p.waitForFunction(
    map => {
      const s = window.atvTerrainStatus,
        m = s?.multiplayer;
      return s?.multiplayerError || (s?.simulation && m?.state?.environment === map && m.remote.some(r => !r.stale));
    },
    map,
    { timeout: 150000 }
  );

try {
  // 1. JavaScript host.
  const host = await page();
  await host.goto(jsBase + '/play.html');
  await button(host, 'Press START').click({ timeout: 90000 });
  await button(host, 'Multiplayer').click();
  await button(host, 'Host Game').click();
  await host.getByLabel('Rider name').fill('JsHost');
  await button(host, 'Select Track').click();
  await host.locator('button.ms-ride').click({ timeout: 60000 });
  await host.waitForFunction(
    () => window.atvTerrainStatus?.simulation && window.atvTerrainStatus.multiplayer?.state?.lobby?.started,
    null,
    {
      timeout: 120000
    }
  );
  let s = await jsState(host);
  const code = s.multiplayer.state.code,
    map = s.multiplayer.state.environment;
  report.room = { code, map };

  // 2. Rust guest arrives beside the host (dropping its socket after 25 s).
  const guest = await page();
  await guest.goto(`${base}/play.html?room=${code}&name=RustGuest&quad=7&dropAfter=25`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.met, null, { timeout: 150000 });
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 30000 });
  await jsRiding(host, map);
  let e = await engine(guest);
  s = await jsState(host);
  report.arrival = {
    met: e.met,
    hostDistanceFt: distance(e.origin, s.frame.origin),
    guestPeer: e.peer
  };
  assert.equal(e.met.peer, 0, 'the guest meets the host');
  assert.ok(
    report.arrival.hostDistanceFt < 40,
    `arrival should be beside the host: ${report.arrival.hostDistanceFt} ft`
  );
  if (shots) {
    await guest.screenshot({ path: shots.replace('.png', '-arrival-engine.png') });
    await host.screenshot({ path: shots.replace('.png', '-arrival-js.png') });
  }

  // 3. Socket drop and slot-key reconnect.
  await guest
    .waitForFunction(
      () => {
        const m = JSON.parse(window.furyMultiplayer ?? 'null');
        return m?.reconnects >= 1 && !m.reconnecting && m.drawn >= 1;
      },
      null,
      { timeout: 60000 }
    )
    .catch(async error => {
      console.log(JSON.stringify(await engine(guest)));
      throw error;
    });
  e = await engine(guest);
  assert.equal(e.closed, null);
  assert.equal(e.peer, report.arrival.guestPeer, 'reconnect keeps the slot');
  report.reconnect = { reconnects: e.reconnects, peer: e.peer, drawn: e.drawn };
  await jsRiding(host, map);

  // 3b. Reloading the Rust tab reclaims the same slot with its kept key.
  await guest.reload();
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, {
    timeout: 150000
  });
  e = await engine(guest);
  assert.equal(e.peer, report.arrival.guestPeer, 'reload keeps the rider slot');
  report.reloadSlot = e.peer;
  await jsRiding(host, map);

  // 4. Host moves the room; the Rust guest reloads there and rejoins.
  await host.bringToFront();
  await host.evaluate(() => window.dispatchEvent(new Event('focus')));
  await host.locator('#menu').click();
  await button(host, 'Select Track').click();
  await host.locator('button.ms-ride').waitFor({ timeout: 60000 });
  await host.locator('[data-action=right]').click();
  await host.locator('button.ms-ride').click();
  await host.waitForFunction(old => window.atvTerrainStatus.multiplayer?.state?.environment !== old, map, {
    timeout: 30000
  });
  const map2 = (await jsState(host)).multiplayer.state.environment;
  await guest.waitForFunction(
    map2 => {
      const m = JSON.parse(window.furyMultiplayer ?? 'null');
      return m?.environment === map2 && m.drawn >= 1;
    },
    map2,
    { timeout: 150000 }
  );
  await jsRiding(host, map2);
  e = await engine(guest);
  s = await jsState(host);
  report.trackChange = { from: map, to: map2, hostDistanceFt: distance(e.origin, s.frame.origin), drawn: e.drawn };
  if (shots) await guest.screenshot({ path: shots.replace('.png', '-moved-engine.png') });
  report.network = e.network;
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
