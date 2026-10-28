// Host-only gate: a race in a multiplayer room (research/MULTIPLAYER.md
// "Room races"). A Rust host rides a room on Supercross sx01
// (`?host=1&venue=sx01`) and a Rust guest joins by code. The host's Pause →
// Play Race reloads both on the course: each takes its own start slot from
// the original shuffle, both hold on the grid until the host's 3-2-1-GO,
// the race path driver (`autopilot=1`) rides each through one lap
// (`raceLaps=1`), each reports its progress, and the host settles the
// finish order, which both pages show as the same Race Results. Then the
// guest quits back to the room's Free Ride, the host's Race Again starts a
// second race that the guest joins from Free Ride, and the guest leaves the
// room mid-race: the host is not held up and ranks it DNF.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_race.mjs
// ATV_GAMES_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_GAMES_SHOTS || '';
const errors = [];
const code = `RC${String(Date.now()).slice(-6)}`;
const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const pages = [];
const race = async p => (await ui(p)).ride?.roomRace;
async function until(p, predicate, timeout = 60000, label = '') {
  const deadline = Date.now() + timeout;
  for (;;) {
    const r = await race(p);
    if (r && predicate(r)) return r;
    if (Date.now() > deadline) {
      const others = await Promise.all(pages.filter(o => o !== p && !o.isClosed()).map(o => race(o).catch(() => null)));
      throw new Error(`timed out ${label}: ${JSON.stringify(r)} others ${JSON.stringify(others)} errors ${errors.slice(0, 5).join(' | ')}`);
    }
    if (process.env.ATV_RACE_TRACE && Date.now() % 10000 < 220) {
      const all = await Promise.all(pages.filter(o => !o.isClosed()).map(async o => ({
        r: await race(o).catch(() => null),
        mp: await o.evaluate(() => window.furyMultiplayer).catch(() => null),
        hidden: await o.evaluate(() => [document.hidden, document.visibilityState, JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page]).catch(() => null)
      })));
      console.log(label, JSON.stringify(all.map(({ r, mp, hidden }) => ({ host: r?.host, phase: r?.phase, timer: r?.timer, went: r?.went, progress: r?.progress?.map(p => [p.peer, Math.round(p.distance), p.finish]), hidden, mp }))));
    }
    await p.waitForTimeout(200);
  }
}
async function key(page, k) { await page.keyboard.down(k); await nextFrames(page, 2); await page.keyboard.up(k); await nextFrames(page, 3); }
async function shot(p, name) {
  if (!shots) return;
  await nextFrames(p, 3);
  await p.screenshot({ path: `${shots}/${name}.png` });
}

const browser = await launch();
try {
  const open = async () => {
    const p = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
    await p.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      Object.defineProperty(document, 'hasFocus', { value: () => true });
    });
    pages.push(p);
    return p;
  };
  const flags = 'uiDiagnostics=1&weather=off&autopilot=1&raceLaps=1';
  const host = await open();
  await host.goto(`${base}/play.html?host=1&venue=sx01&room=${code}&name=RustHost&${flags}`);
  await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  const guest = await open();
  await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=RustGuest&${flags}`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 180000 });
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });

  // Pause → Play Race (the host's row).
  await host.bringToFront();
  await host.mouse.click(480, 300);
  await key(host, 'Escape');
  await host.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await ui(host)).toolbar.menu;
  const at = menu.choices.indexOf('playrace');
  assert.ok(at > 0, `Play Race in ${menu.choices}`);
  for (let i = 0; i < at; i++) await key(host, 'ArrowDown');
  await key(host, 'Enter');

  // Both reload the course on their own start slots and wait on the grid.
  let h = await until(host, r => r.loaded, 240000, 'host loads the race');
  assert.equal(h.host, true);
  assert.equal(h.laps, 1);
  const slots = h.grid.filter(s => s > 0).sort();
  assert.deepEqual(slots, [1, 2], `two distinct start slots ${h.grid}`);
  let g = await until(guest, r => r.loaded && r.serial === h.serial, 240000, 'guest loads the race');
  assert.equal(g.host, false);
  assert.deepEqual(g.grid, h.grid);
  await shot(host, 'mp-race-host-grid');
  // One shared countdown, then racing.
  await until(guest, r => r.went && r.phase >= 1, 60000, 'guest countdown');
  await until(host, r => r.went && r.phase >= 2, 60000, 'host racing');
  h = await until(host, r => r.progress.length === 2 && r.progress.every(p => p.distance > 50), 120000, 'both make progress');
  await shot(guest, 'mp-race-guest-racing');
  // Both finish; the host's order reaches both pages.
  for (const p of [host, guest]) {
    await until(p, r => r.phase === 3 && r.results.length === 2, 480000, 'results');
    await p.waitForFunction(() => {
      const ride = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride;
      return ride?.results?.length >= 1;
    }, null, { timeout: 60000 });
  }
  h = await race(host);
  g = await race(guest);
  assert.deepEqual(g.scores, h.scores, 'agreed finish times');
  assert.deepEqual(g.results.map(r => r.name), h.results.map(r => r.name), 'agreed order');
  assert.equal(g.summary, h.summary);
  assert.ok(h.results.every(r => r.time > 0), `both finished: ${h.summary}`);
  await shot(guest, 'mp-race-guest-results');
  await shot(host, 'mp-race-host-results');
  const first = h.summary;

  // Results → Continue → Quit: the guest returns to the room's Free Ride.
  await guest.bringToFront();
  await key(guest, 'Enter');
  await key(guest, 'ArrowDown');
  await key(guest, 'Enter');
  await guest.waitForFunction(() => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return s.ride && !s.ride.roomRace && !s.ride.race && JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1;
  }, null, { timeout: 240000 });
  // Results → Continue → Race Again: the host starts the next race and the
  // guest follows from Free Ride.
  await host.bringToFront();
  await key(host, 'Enter');
  await key(host, 'Enter');
  h = await until(host, r => r.serial === h.serial + 1 && r.loaded, 240000, 'host loads race 2');
  g = await until(guest, r => r.serial === h.serial && r.loaded, 240000, 'guest loads race 2');
  assert.deepEqual(g.grid, h.grid);
  await until(guest, r => r.went && r.phase >= 2, 60000, 'guest races race 2');
  await until(host, r => r.progress.length === 2 && r.progress.every(p => p.distance > 20), 60000, 'guest progress in race 2');
  // The guest leaves the room mid-race.
  await guest.close();
  h = await until(host, r => r.phase === 3 && r.results.length === 2, 480000, 'race 2 results without the guest');
  assert.ok(h.results[0].time > 0, `host finished race 2: ${h.summary}`);
  assert.equal(h.results[1].time, null, `leaver ranks DNF: ${h.summary}`);
  assert.match(h.summary, /2\. RustGuest DNF$/, 'the leaver keeps its name');
  assertNoErrors(errors);
  console.log(`Multiplayer race: grid, shared countdown, per-rider laps, agreed results (${first}), return to room, Race Again and a mid-race leaver (${h.summary}) verified`);
} finally {
  await browser.close();
}
