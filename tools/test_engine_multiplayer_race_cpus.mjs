// Host-only gate: CPU riders in a room race (research/MULTIPLAYER.md "Room
// race CPUs"). A Rust host rides a Supercross sx01 room filled to five
// riders (`roomCpus=5`, Host Game → CPU Riders: Fill to 5) and a Rust guest
// joins by code. The host's Pause → Play Race puts both humans and three
// host-simulated CPUs on one shuffled grid; the guest draws the CPUs from the
// host's relayed motion and sees them move; the race path driver
// (`autopilot=1`) rides both humans through one lap (`raceLaps=1`) and both
// pages show the same Race Results with every CPU ranked among the riders.
// The guest (`audioDiagnostics=1`) also hears the host's CPUs' engines from
// their relayed motion (research/AUDIO.md "CPU riders").
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_race_cpus.mjs
// ATV_GAMES_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_GAMES_SHOTS || '';
const errors = [];
const code = `RU${String(Date.now()).slice(-6)}`;
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
    await p.waitForTimeout(200);
  }
}
async function key(page, k) { await page.keyboard.down(k); await nextFrames(page, 2); await page.keyboard.up(k); await nextFrames(page, 3); }
async function shot(p, name) {
  if (!shots) return;
  await nextFrames(p, 3);
  await p.screenshot({ path: `${shots}/${name}.png` });
}
const moved = (a, b) => a.some((o, i) => b[i] && Math.hypot(o[0] - b[i][0], o[2] - b[i][2]) > 10);

const browser = await launch();
try {
  const open = async () => {
    const p = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
    await p.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      Object.defineProperty(document, 'hasFocus', { value: () => true });
    });
    pages.push(p);
    return p;
  };
  const flags = 'uiDiagnostics=1&weather=off&autopilot=1&raceLaps=1';
  const host = await open();
  await host.goto(`${base}/play.html?host=1&venue=sx01&room=${code}&name=RustHost&roomCpus=5&${flags}`);
  await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  const guest = await open();
  await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=RustGuest&audioDiagnostics=1&${flags}`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 180000 });
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });

  // Pause → Play Race (the host's row).
  await host.bringToFront();
  await host.mouse.click(320, 180);
  await key(host, 'Escape');
  await host.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await ui(host)).toolbar.menu;
  const at = menu.choices.indexOf('playrace');
  assert.ok(at > 0, `Play Race in ${menu.choices}`);
  for (let i = 0; i < at; i++) await key(host, 'ArrowDown');
  await key(host, 'Enter');

  // Two humans and three CPUs share one shuffled grid of five.
  let h = await until(host, r => r.loaded, 240000, 'host loads the race');
  assert.equal(h.cpus, 3, `three CPUs fill the field: ${JSON.stringify(h)}`);
  const slots = [...h.grid, ...h.cpuGrid].filter(s => s > 0).sort();
  assert.deepEqual(slots, [1, 2, 3, 4, 5], `five distinct start slots ${h.grid} ${h.cpuGrid}`);
  let g = await until(guest, r => r.loaded && r.serial === h.serial, 240000, 'guest loads the race');
  assert.deepEqual(g.grid, h.grid);
  assert.deepEqual(g.cpuGrid, h.cpuGrid);
  // The guest draws the host's CPUs and sees them move once racing.
  g = await until(guest, r => r.cpuOrigins.length === 3, 120000, 'guest receives CPU motion');
  const before = g.cpuOrigins;
  await until(host, r => r.went && r.phase >= 2, 90000, 'host racing');
  g = await until(guest, r => r.phase >= 2 && moved(r.cpuOrigins, before), 120000, 'guest sees the CPUs move');
  // The guest hears them: a click starts its output, and the host's CPUs
  // join its engine voices with the race sound table's sets.
  await guest.bringToFront();
  await guest.mouse.click(320, 180);
  const engineAudio = p => p.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}'));
  let heard;
  for (const deadline = Date.now() + 60000; ;) {
    heard = await engineAudio(guest);
    if (heard.started && heard.cpu?.engines >= 1 && heard.cpu.levels.some(l => l[2][0] + l[2][1] > 0)) break;
    if (Date.now() > deadline) throw new Error(`guest CPU engines: ${JSON.stringify(heard.cpu)} errors ${errors.slice(0, 5).join(' | ')}`);
    await guest.waitForTimeout(250);
  }
  assert.ok(heard.cpu.engines <= 3, `guest CPU engine voices: ${heard.cpu.engines}`);
  assert.ok(heard.cpu.engineStarts >= heard.cpu.engines, `guest CPU engine starts: ${heard.cpu.engineStarts}`);
  assert.ok(heard.cpu.voices >= heard.cpu.engines, `guest CPU-labelled mixer voices: ${heard.cpu.voices}`);
  console.log(`guest CPU audio: ${heard.cpu.engines} engine voices, ${heard.cpu.engineStarts} starts, levels ${JSON.stringify(heard.cpu.levels)}`);
  await host.bringToFront();
  h = await until(host, r => r.progress.filter(p => p.peer >= 4).length === 3 && r.progress.some(p => p.peer >= 4 && p.distance > 50), 120000, 'CPU progress');
  g = await until(guest, r => r.progress.filter(p => p.peer >= 4).length === 3, 60000, 'guest has CPU progress');
  await shot(guest, 'mp-race-cpus-guest-racing');
  await shot(host, 'mp-race-cpus-host-racing');
  const hud = (await ui(guest)).ride?.race;
  console.log('guest race HUD', JSON.stringify({ position: hud?.position, field: hud?.field }));
  // Results: two humans and three CPUs in one agreed order.
  for (const p of [host, guest]) {
    await until(p, r => r.phase === 3 && r.results.length === 5, 600000, 'results');
  }
  h = await race(host);
  g = await race(guest);
  assert.deepEqual(g.scores, h.scores, 'agreed rider results');
  assert.deepEqual(g.cpuScores, h.cpuScores, 'agreed CPU results');
  assert.deepEqual(g.results.map(r => r.name), h.results.map(r => r.name), 'agreed order and CPU names');
  assert.equal(g.summary, h.summary);
  const cpuRows = h.results.filter(r => r.id >= 4);
  assert.equal(cpuRows.length, 3, h.summary);
  assert.ok(cpuRows.every(r => r.name && !/^Rider /.test(r.name)), `CPUs keep roster names: ${h.summary}`);
  assert.ok(h.results.filter(r => r.id < 4).every(r => r.time > 0), `both humans finished: ${h.summary}`);
  // Finishers rank by time, then the rest.
  const times = h.results.map(r => r.time ?? Infinity);
  assert.deepEqual([...times].sort((a, b) => a - b), times, `ordered: ${h.summary}`);
  await shot(guest, 'mp-race-cpus-guest-results');
  assertNoErrors(errors);
  console.log(`Multiplayer race with CPUs: five-rider grid, guest draws moving CPUs, agreed results (${h.summary}) verified`);
} finally {
  await browser.close();
}
