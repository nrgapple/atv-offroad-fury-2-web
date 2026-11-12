// Host-only gate: a room race guest hears the host's CPU riders' terrain
// drive-on sounds (research/AUDIO.md "CPU riders", #500). A Rust host rides
// an ENV01 room filled with CPUs (`roomCpus=4`) and a Rust guest
// (`audioDiagnostics=1`) joins by code; the host's Pause → Play Race starts
// the race and the race path driver (`autopilot=1`) rides both humans. The
// host's CPU motion carries each CPU's sound cues (terrain bytes, events,
// impacts); the guest plays the CPUs' drive-on clips from them through its
// CPU emitters, alongside their engines from the relayed motion. Vocals,
// impacts, chassis sounds and splashes are logged (a short ride need not
// wreck or land hard).
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_race_cpu_sounds.mjs
// ATV_GAMES_SHOTS=<dir> saves a capture.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_GAMES_SHOTS || '';
const errors = [];
const code = `RS${String(Date.now()).slice(-6)}`;
const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const race = async p => (await ui(p)).ride?.roomRace;
const engineAudio = p => p.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}'));
async function until(p, predicate, timeout, label) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const r = await race(p);
    if (r && predicate(r)) return r;
    if (Date.now() > deadline) throw new Error(`timed out ${label}: ${JSON.stringify(r)} errors ${errors.slice(0, 5).join(' | ')}`);
    await p.waitForTimeout(200);
  }
}
async function key(page, k) { await page.keyboard.down(k); await nextFrames(page, 2); await page.keyboard.up(k); await nextFrames(page, 3); }

const browser = await launch();
try {
  const open = async () => {
    const p = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
    await p.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      Object.defineProperty(document, 'hasFocus', { value: () => true });
    });
    return p;
  };
  const flags = 'uiDiagnostics=1&weather=off&autopilot=1&raceLaps=1';
  const host = await open();
  await host.goto(`${base}/play.html?host=1&venue=env01&room=${code}&name=RustHost&roomCpus=4&${flags}`);
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

  const h = await until(host, r => r.loaded, 240000, 'host loads the race');
  assert.ok(h.cpus >= 1, `CPUs fill the field: ${JSON.stringify(h)}`);
  await until(guest, r => r.loaded && r.serial === h.serial, 240000, 'guest loads the race');
  await until(guest, r => r.cpuOrigins.length === h.cpus, 120000, 'guest receives CPU motion');
  await until(host, r => r.went && r.phase >= 2, 90000, 'host racing');

  // A click starts the guest's output; the CPUs ride off the grid onto
  // ENV01's terrain and the guest starts their drive-on clips.
  await guest.bringToFront();
  await guest.mouse.click(320, 180);
  let heard;
  for (const deadline = Date.now() + 90000; ;) {
    heard = await engineAudio(guest);
    if (heard.started && heard.cpu?.engines >= 1 && heard.cpu.drive >= 1) break;
    if (Date.now() > deadline) throw new Error(`guest CPU sounds: ${JSON.stringify(heard.cpu)} errors ${errors.slice(0, 5).join(' | ')}`);
    await guest.waitForTimeout(250);
  }
  assert.ok(heard.cpu.driveVoices <= 2 * h.cpus, `guest CPU drive-on voices: ${heard.cpu.driveVoices}`);
  assert.ok(heard.cpu.engines <= h.cpus, `guest CPU engine voices: ${heard.cpu.engines}`);
  // A few more seconds of riding for the log.
  await guest.waitForTimeout(8000);
  heard = await engineAudio(guest);
  if (shots) {
    await nextFrames(guest, 3);
    await guest.screenshot({ path: `${shots}/mp-race-cpu-sounds-guest.png` });
  }
  assertNoErrors(errors);
  console.log(`PASS: room race guest hears the host's ${h.cpus} CPUs: engines ${heard.cpu.engines}, drive-on ${heard.cpu.drive} (${heard.cpu.driveVoices} voices), vocals ${heard.cpu.vocals}, impacts ${heard.cpu.impacts}, chassis ${heard.cpu.chassis}, splashes ${heard.cpu.splashes}`);
} finally {
  await browser.close();
}
