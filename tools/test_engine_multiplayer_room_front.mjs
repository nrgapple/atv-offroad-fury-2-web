// Host-only gate: the Rust engine's Change ATV and Rider and Select Track over
// a live room. A Rust browser hosts from its menus; a JavaScript guest joins.
//   1. Pause → Change ATV and Rider opens the garage over the held room. As in
//      game.js (a live room keeps simulating under a menu), the host's ride
//      keeps riding with neutral input behind it: the guest keeps seeing it
//      (not stale) coast on from its throttle run. The next ATV and Done
//      reload the ride in place, where it came to rest, with the new look,
//      which the guest sees.
//   2. Pause → Select Track opens the course picker; the next course and
//      Ride move everyone there.
//   3. A Rust guest's garage is open over the room: only the menu tune plays
//      (the garage covers the ride; audio.js silences ride and rider sounds).
//      The host moves the room again: like multiplayer.js `follow`, the guest
//      leaves the garage for the new course's loading page at once and rides
//      it with the garage's current ATV.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_room_front.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames, url } from './browser/harness.mjs';

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
  await guest.goto(`${base}/play.html?room=${code}&player=JsGuest`);
  await guest.waitForFunction(() => window.atvTerrainStatus?.multiplayer?.remote?.some(r => !r.stale), null, {
    timeout: 180000
  });
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });
  await host.bringToFront();

  // 1. Change ATV and Rider, straight after a throttle run.
  await host.mouse.click(640, 400);
  await host.keyboard.down('KeyW');
  await new Promise(resolve => setTimeout(resolve, 2500));
  await host.keyboard.up('KeyW');
  await pauseRow(host, 'changeride');
  await onPage(host, 'garage');
  // The ride keeps riding behind the garage and stays present: the guest never
  // sees the host go stale (moving poses go stale after 1 s, still ones after
  // 2.5 s), and sees it coast on.
  const behind = [];
  for (let i = 0; i < 24; i++) {
    behind.push(
      await guest.evaluate(() => {
        const r = window.atvTerrainStatus.multiplayer?.remote?.find(p => p.peer === 0);
        return r && !r.stale ? r.position : null;
      })
    );
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  const seen = behind.filter(Boolean);
  report.behindGarage = {
    seconds: 6,
    present: seen.length,
    samples: behind.length,
    coastedFt: seen.length ? distance(seen[0], seen.at(-1)) : 0
  };
  assert.ok(behind.every(Boolean), `present behind the garage: ${JSON.stringify(report.behindGarage)}`);
  assert.ok(report.behindGarage.coastedFt > 2, `rides on behind the garage: ${JSON.stringify(report.behindGarage)}`);
  const before = { origin: seen.at(-1) };
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

  // 3. The host changes course while a Rust guest is in its garage.
  const rust = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await rust.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  await rust.goto(url(`/engine.html?uiDiagnostics=1&room=${code}&player=RustGuest`));
  await rust.waitForFunction(
    map => {
      const m = JSON.parse(window.furyMultiplayer ?? 'null');
      return m?.environment === map && m.drawn >= 1;
    },
    next,
    { timeout: 240000 }
  );
  await rust.bringToFront();
  await pauseRow(rust, 'changeride');
  await onPage(rust, 'garage');
  await rust.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').menuMusic?.playing === true,
    null,
    { timeout: 60000 }
  );
  await key(rust, 'ArrowRight');
  const guestQuad = (await read(rust)).front.quad;
  await host.bringToFront();
  await pauseRow(host, 'hosttrack');
  await onPage(host, 'tracks');
  await key(host, 'ArrowRight');
  const third = (await read(host)).front.track;
  assert.notEqual(third, next);
  // The loader's status lines, sampled from before the host's Ride.
  await rust.evaluate(() => {
    window.statusTrace = [];
    const sample = () => {
      const text = document.querySelector('#status')?.textContent ?? '';
      if (text !== window.statusTrace.at(-1)?.[1]) window.statusTrace.push([performance.now(), text]);
    };
    sample();
    window.statusTimer = setInterval(sample, 10);
  });
  const pressed = await rust.evaluate(() => performance.now());
  await key(host, 'Enter');
  await rust.bringToFront();
  await rust.waitForFunction(
    map => {
      const m = JSON.parse(window.furyMultiplayer ?? 'null');
      return m?.environment === map && m.drawn >= 1;
    },
    third,
    { timeout: 240000 }
  );
  await guest.waitForFunction(
    q => window.atvTerrainStatus.multiplayer?.state?.lobby?.players.find(p => p.name === 'RustGuest')?.atv_model === q,
    guestQuad,
    { timeout: 60000 }
  );
  const trace = await rust.evaluate(() => {
    clearInterval(window.statusTimer);
    return window.statusTrace;
  });
  const reading = trace.find(([, text]) => text.includes(`Reading original ${third.toUpperCase()}`));
  report.followCourse = { to: third, loadingAfterMs: reading && Math.round(reading[0] - pressed), quad: guestQuad };
  assert.ok(
    reading && reading[0] - pressed < 10000,
    `the garage went to the new course's loader: ${JSON.stringify(report.followCourse)}`
  );
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
