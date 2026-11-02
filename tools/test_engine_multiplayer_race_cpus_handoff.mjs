// Host leaving a room race with CPU riders (research/MULTIPLAYER.md "Room
// race CPUs", "Host leaving"). A Rust host rides a Supercross sx01 room
// filled to five riders (`roomCpus=5`) and two Rust guests join by code, so
// the field is three humans and two host-simulated CPUs. Once both guests
// draw the CPUs moving and hear their engines (`audioDiagnostics=1`), the
// host closes its page mid-race. The room passes to one guest (its mirror
// never simulated the CPUs: `Field::adopt` retires them) and the other guest
// sees the host change and retires them too. Both remaining pages report
// `cpusRetired`, draw no CPU riders (`ride.cpuDrawn`), drop the CPUs' engine
// voices, keep racing (the humans' progress grows), and agree on the
// results: both guests finish, the leaver and the CPUs rank as not finished.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_race_cpus_handoff.mjs
// ATV_GAMES_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_GAMES_SHOTS || '';
const errors = [];
const code = `RH${String(Date.now()).slice(-6)}`;
const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const pages = [];
const race = async p => (await ui(p)).ride?.roomRace;
async function until(p, predicate, timeout = 60000, label = '') {
  const deadline = Date.now() + timeout;
  for (;;) {
    const s = await ui(p);
    const r = s.ride?.roomRace;
    if (r && predicate(r, s)) return r;
    if (Date.now() > deadline) {
      const others = await Promise.all(pages.filter(o => o !== p && !o.isClosed()).map(o => race(o).catch(() => null)));
      throw new Error(`timed out ${label}: ${JSON.stringify(r)} drawn ${s.ride?.cpuDrawn} others ${JSON.stringify(others)} errors ${errors.slice(0, 5).join(' | ')}`);
    }
    await p.waitForTimeout(200);
  }
}
async function key(page, k) {
  await page.keyboard.down(k);
  await nextFrames(page, 2);
  await page.keyboard.up(k);
  await nextFrames(page, 3);
}
async function shot(p, name) {
  if (!shots) return;
  await nextFrames(p, 3);
  await p.screenshot({ path: `${shots}/${name}.png` });
}
const engineAudio = p => p.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}'));
async function audioUntil(p, predicate, timeout, label) {
  let heard;
  for (const deadline = Date.now() + timeout; ;) {
    heard = await engineAudio(p);
    if (predicate(heard)) return heard;
    if (Date.now() > deadline) throw new Error(`${label}: ${JSON.stringify(heard.cpu)} errors ${errors.slice(0, 5).join(' | ')}`);
    await p.waitForTimeout(250);
  }
}
const moved = (a, b) => a.some((o, i) => b[i] && Math.hypot(o[0] - b[i][0], o[2] - b[i][2]) > 10);
const humans = r => r.progress.filter(p => p.peer < 4);

const browser = await launch();
try {
  const open = async () => {
    const p = await newPage(browser, { viewport: { width: 480, height: 270 } }, errors);
    await p.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      Object.defineProperty(document, 'hasFocus', { value: () => true });
    });
    pages.push(p);
    return p;
  };
  const flags = 'uiDiagnostics=1&weather=off&autopilot=1&raceLaps=1&audioDiagnostics=1';
  const host = await open();
  await host.goto(`${base}/play.html?host=1&venue=sx01&room=${code}&name=RustHost&roomCpus=5&${flags}`);
  await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  const guests = [];
  for (const name of ['GuestA', 'GuestB']) {
    const g = await open();
    await g.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=${name}&${flags}`);
    guests.push(g);
  }
  for (const g of guests) {
    await g.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 2, null, { timeout: 180000 });
  }
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 2, null, { timeout: 60000 });

  // Pause → Play Race (the host's row).
  await host.bringToFront();
  await host.mouse.click(240, 135);
  await key(host, 'Escape');
  await host.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await ui(host)).toolbar.menu;
  const at = menu.choices.indexOf('playrace');
  assert.ok(at > 0, `Play Race in ${menu.choices}`);
  for (let i = 0; i < at; i++) await key(host, 'ArrowDown');
  await key(host, 'Enter');

  // Three humans and two CPUs share one grid of five.
  const h = await until(host, r => r.loaded, 240000, 'host loads the race');
  assert.equal(h.cpus, 2, `two CPUs fill the field: ${JSON.stringify(h)}`);
  for (const g of guests) await until(g, r => r.loaded && r.serial === h.serial, 240000, 'guest loads the race');
  await until(host, r => r.went && r.phase >= 2, 120000, 'host racing');
  // Every guest draws the host's CPUs moving and hears their engines.
  for (const g of guests) {
    const first = await until(g, (r, s) => r.cpuOrigins.length === 2 && s.ride.cpuDrawn === 2, 120000, 'guest draws the CPUs');
    await until(g, r => r.phase >= 2 && moved(r.cpuOrigins, first.cpuOrigins), 120000, 'guest sees the CPUs move');
    await g.bringToFront();
    await g.mouse.click(240, 135);
    const heard = await audioUntil(g, a => a.started && a.cpu?.engines >= 1, 60000, 'guest CPU engines');
    console.log(`guest hears ${heard.cpu.engines} CPU engines before the host leaves`);
    assert.equal((await race(g)).cpusRetired, false, 'CPUs live before the host leaves');
  }
  await shot(guests[1], 'mp-race-cpus-handoff-before');

  // The host leaves mid-race.
  const before = await Promise.all(guests.map(async g => humans(await race(g))));
  await host.close();
  const after = [];
  for (const g of guests) {
    const r = await until(g, (r, s) => r.cpusRetired && s.ride.cpuDrawn === 0, 60000, 'guest retires the CPUs');
    const heard = await audioUntil(g, a => a.cpu?.engines === 0, 30000, 'CPU engines stop');
    assert.ok(heard.cpu.voices === 0, `no CPU-labelled mixer voices: ${JSON.stringify(heard.cpu)}`);
    after.push(r);
  }
  const hosts = after.filter(r => r.host).length;
  assert.equal(hosts, 1, `one guest takes the room: ${JSON.stringify(after.map(r => r.host))}`);
  // The race runs on for the humans.
  for (const [i, g] of guests.entries()) {
    const was = Object.fromEntries(before[i].map(p => [p.peer, p.distance]));
    await until(
      g,
      r => r.phase === 2 && humans(r).some(p => was[p.peer] !== undefined && p.distance > was[p.peer] + 30),
      60000,
      'humans keep racing'
    );
    // The retired CPUs stay retired.
    await nextFrames(g, 30);
    assert.equal((await ui(g)).ride.cpuDrawn, 0, 'no CPUs drawn later');
  }
  await shot(guests[1], 'mp-race-cpus-handoff-after');

  // Results: both guests finish, the leaver and the CPUs do not.
  for (const g of guests) await until(g, r => r.phase === 3 && r.results.length === 5, 600000, 'results');
  const [a, b] = await Promise.all(guests.map(race));
  assert.deepEqual(a.results.map(r => r.name), b.results.map(r => r.name), 'agreed order');
  assert.equal(a.summary, b.summary);
  const cpuRows = a.results.filter(r => r.id >= 4);
  assert.equal(cpuRows.length, 2, a.summary);
  const finishers = a.results.filter(r => r.time > 0).map(r => r.name).sort();
  assert.deepEqual(finishers, ['GuestA', 'GuestB'], `only the remaining humans finish: ${a.summary}`);
  assert.ok(
    a.results.find(r => r.name === 'RustHost')?.time == null,
    `the leaver ranks DNF: ${a.summary}`
  );
  assertNoErrors(errors);
  console.log(`Multiplayer race with CPUs, host leaves: every guest retires the CPUs, stops drawing and hearing them, races on (${a.summary}) verified`);
} finally {
  await browser.close();
}
