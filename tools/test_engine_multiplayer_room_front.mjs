// Host-only gate: the Rust engine's Change ATV and Rider and Select Track over
// a live room. A Rust browser hosts from its menus; a JavaScript guest joins.
//   1. Pause → Change ATV and Rider opens the garage over the held room; the
//      guest keeps seeing the paused host (not stale) behind it; the next ATV
//      and Done reload the ride in place with the new look, which the guest
//      sees.
//   2. Pause → Select Track opens the course picker; the next course and
//      Ride move everyone there.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_room_front.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, base, jsBase, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const report = {};
const errors = [];
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name) =>
  page.waitForFunction(
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name,
    name,
    { timeout: 60000 }
  );
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const centre = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });
const distance = (a, b) => Math.hypot(...a.map((v, i) => v - b[i]));
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function choose(page, action) {
  const f = (await read(page)).front;
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = 0; i < at; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
}
async function pauseRow(page, action) {
  await page.mouse.click(640, 400);
  await key(page, 'Escape');
  await page.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause'
  );
  const hit = (await read(page)).toolbar.menu.hits.find(h => h.action === action);
  assert.ok(hit, `${action} in the pause menu`);
  await page.mouse.click(centre(hit).x, centre(hit).y);
}
const browser = await launch();
try {
  const host = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await host.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  await host.goto(url('/engine.html?uiDiagnostics=1'));
  await onPage(host, 'title');
  await host.mouse.click(8, 8);
  await key(host, 'Enter');
  await onPage(host, 'main');
  await choose(host, 'multiplayer');
  await onPage(host, 'multiplayer');
  await choose(host, 'create-lobby');
  await onPage(host, 'create-lobby');
  await host.mouse.click(8, 8);
  await choose(host, 'hosttrack');
  await onPage(host, 'tracks');
  await key(host, 'Enter');
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.code, null, { timeout: 180000 });
  const code = (await engine(host)).code;

  const guest = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await guest.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  await guest.goto(`${jsBase}/play.html?room=${code}&player=JsGuest`);
  await guest.waitForFunction(() => window.atvTerrainStatus?.multiplayer?.remote?.some(r => !r.stale), null, {
    timeout: 180000
  });
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });
  await host.bringToFront();
  const before = await engine(host);

  // 1. Change ATV and Rider.
  await pauseRow(host, 'changeride');
  await onPage(host, 'garage');
  // The ride waits paused behind the garage and stays present: the guest
  // never sees the host go stale (moving poses go stale after 1 s, still ones
  // after 2.5 s; paused presence lasts 15 s).
  const behind = [];
  for (let i = 0; i < 24; i++) {
    behind.push(
      await guest.evaluate(() => {
        const r = window.atvTerrainStatus.multiplayer?.remote?.find(p => p.peer === 0);
        return r ? !r.stale : false;
      })
    );
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  report.behindGarage = { seconds: 6, present: behind.filter(Boolean).length, samples: behind.length };
  assert.ok(behind.every(Boolean), `present behind the garage: ${JSON.stringify(report.behindGarage)}`);
  await key(host, 'ArrowRight');
  const quad = (await read(host)).front.quad;
  await key(host, 'Escape');
  await host.waitForFunction(
    () => {
      const m = JSON.parse(window.furyMultiplayer ?? 'null');
      return m?.drawn >= 1 && m.network?.messages > 0 && m.origin;
    },
    null,
    { timeout: 180000 }
  );
  await guest.waitForFunction(
    q =>
      window.atvTerrainStatus.multiplayer?.state?.lobby?.players.find(p => p.id === 0)?.atv_model === q &&
      window.atvTerrainStatus.multiplayer.remote.some(r => r.peer === 0 && !r.stale),
    quad,
    { timeout: 60000 }
  );
  let after = await engine(host);
  report.changeRide = { quad, movedFt: distance(before.origin, after.origin), code: after.code };
  assert.equal(after.code, code);
  assert.ok(report.changeRide.movedFt < 20, `reloaded in place: ${JSON.stringify(report.changeRide)}`);

  // 2. Select Track.
  await pauseRow(host, 'hosttrack');
  await onPage(host, 'tracks');
  await key(host, 'ArrowRight');
  const next = (await read(host)).front.track;
  await key(host, 'Enter');
  await guest.waitForFunction(map => window.atvTerrainStatus.multiplayer?.state?.environment === map, next, {
    timeout: 60000
  });
  await host.waitForFunction(
    map => {
      const m = JSON.parse(window.furyMultiplayer ?? 'null');
      return m?.environment === map && m.drawn >= 1;
    },
    next,
    { timeout: 240000 }
  );
  report.selectTrack = { to: next, drawn: (await engine(host)).drawn };
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
