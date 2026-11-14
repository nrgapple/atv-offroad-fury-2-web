// Authored CPU riders in a free ride (research/CPU_MINIGAMES.md "Free roam",
// a port adaptation; the original Freestyle has none):
// - Off by default: a free ride link on Prospect Springs loads no CPU riders and
//   no roam map, with the setting absent and with it saved as 0, and Options
//   → Game Settings reads "Free Ride CPU riders: Off (original)";
// - the Game Settings row cycles Off / 1 / 2 / 3 and saves; the ride in progress
//   keeps its riders (the next free ride takes the choice);
// - with 2 saved, the next free ride loads two CPU riders that leave the
//   start, reach points of interest or follow the venue's race line, keep
//   out of deep water and inside the map, rarely wreck, and are on no
//   standings; Pause →
//   Main menu still leaves the ride;
// - with 3 saved, frame times are sampled beside the option-off ride's.
// ATV_FREE_CPUS_SHOTS=<dir> saves captures.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_FREE_CPUS_SHOTS || '';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const ride = async page => (await state(page)).ride;
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function until(page, predicate, timeout = 60000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const r = await ride(page);
    if (r && predicate(r)) return r;
    if (Date.now() > deadline) throw new Error(`timed out: ${predicate} ${JSON.stringify(r?.cpus)}`);
    await page.waitForTimeout(100);
  }
}
const onMenu = (page, name) =>
  page.waitForFunction(
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === name,
    name,
    { timeout: 20000 }
  );
async function click(page, action) {
  const menu = (await state(page)).toolbar.menu;
  const hit = menu.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${menu.page}: ${menu.choices}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
  await nextFrames(page, 3);
}
const label = async (page, action) => (await state(page)).toolbar.menu.labels[action];
async function shot(page, name) {
  if (!shots) return;
  await nextFrames(page, 3);
  await page.screenshot({ path: `${shots}/${name}.png` });
}
async function freeRide(page) {
  await page.goto(url('/play.html?map=env05&play=1&weather=off&uiDiagnostics=1'));
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').active === true, null, {
    timeout: 300000
  });
  await page.mouse.click(640, 400);
  return until(page, r => r.environment === 'env05' && Array.isArray(r.cpus));
}
// Presented frame intervals over `seconds` (requestAnimationFrame).
const frames = (page, seconds) =>
  page.evaluate(
    seconds =>
      new Promise(resolve => {
        const times = [];
        let last = performance.now();
        const end = last + seconds * 1000;
        const tick = now => {
          times.push(now - last);
          last = now;
          if (now < end) requestAnimationFrame(tick);
          else resolve(times);
        };
        requestAnimationFrame(tick);
      }),
    seconds
  );
function summary(times) {
  const sorted = [...times].sort((a, b) => a - b);
  const at = q => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))];
  const mean = times.reduce((a, b) => a + b, 0) / times.length;
  return `${times.length} frames, mean ${mean.toFixed(2)} ms, median ${at(0.5).toFixed(2)} ms, p95 ${at(0.95).toFixed(2)} ms, max ${sorted.at(-1).toFixed(1)} ms`;
}
const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);

if (shots) await mkdir(shots, { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
  });

  // Off by default: no CPU riders and no roam map.
  let r = await freeRide(page);
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-free-ride-cpus')), null);
  assert.deepEqual([r.cpus.length, r.roam], [0, null], 'no CPU riders by default');
  await page.waitForTimeout(3000); // Time-based on purpose: the ride settles before sampling.
  const off = await frames(page, 10);
  console.log(`Frame times, option off: ${summary(off)}`);
  // Saved as 0 is the same ride.
  await page.evaluate(() => localStorage.setItem('atv-free-ride-cpus', '0'));
  r = await freeRide(page);
  assert.deepEqual([r.cpus.length, r.roam], [0, null], 'no CPU riders with the option saved off');

  // Options → Game Settings: the row cycles and saves; this ride is unchanged.
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  await click(page, 'settings');
  await onMenu(page, 'settings');
  await click(page, 'game-settings');
  await onMenu(page, 'game-settings');
  assert.equal(await label(page, 'free-ride-cpus'), 'Free Ride CPU riders: Off (original)');
  await shot(page, 'game-settings-row');
  await click(page, 'free-ride-cpus');
  assert.equal(await label(page, 'free-ride-cpus'), 'Free Ride CPU riders: 1');
  await click(page, 'free-ride-cpus');
  assert.equal(await label(page, 'free-ride-cpus'), 'Free Ride CPU riders: 2');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-free-ride-cpus')), '2');
  assert.equal((await ride(page)).cpus.length, 0, 'the ride in progress keeps its riders');

  // The next free ride: two CPU riders that roam.
  r = await freeRide(page);
  assert.equal(r.cpus.length, 2);
  assert.ok(r.roam.points >= 6, `points of interest: ${r.roam.points}`);
  assert.ok(r.roam.line > 2, `Prospect Springs' race line: ${r.roam.line}`);
  assert.deepEqual([r.game, r.tag, r.hockey], [null, null, null], 'no game: nothing is scored');
  console.log(`Roam map: ${r.roam.points} points of interest, ${r.roam.line} line points (closed ${r.roam.closed})`);
  const startAt = r.cpus.map(c => c.origin);
  r = await until(page, r => r.cpus.every((c, i) => flat(c.origin, startAt[i]) > 60), 60000);
  await shot(page, 'roaming');
  // They get somewhere: a point of interest reached or the line followed.
  r = await until(page, r => r.cpus.every(c => c.roam.visits >= 1 || c.roam.lineFeet > 300), 240000);
  console.log(`Stalls so far: ${r.cpus.map(c => c.stalls)}`);
  console.log(`Objectives: ${r.cpus.map(c => `CPU ${c.player - 1} visits ${c.roam.visits}, line ${c.roam.lineFeet.toFixed(0)} ft`).join('; ')}`);
  // A minute of roaming: inside the map, out of deep water, not stranded.
  const wet = r.cpus.map(() => 0);
  const far = r.cpus.map(() => 0);
  let last = r.cpus.map(c => c.origin);
  for (let i = 0; i < 120; i++) {
    await page.waitForTimeout(500); // Time-based on purpose: a sample every half second.
    r = await ride(page);
    r.cpus.forEach((c, n) => {
      const [x, , z] = c.origin;
      assert.ok(
        x > r.roam.min[0] - 60 && x < r.roam.max[0] + 60 && z > r.roam.min[1] - 60 && z < r.roam.max[1] + 60,
        `CPU ${n + 1} left the map at ${c.origin}`
      );
      wet[n] = c.roam.wet ? wet[n] + 1 : 0;
      assert.ok(wet[n] < 8, `CPU ${n + 1} stayed in deep water at ${c.origin}`);
      far[n] += flat(c.origin, last[n]);
    });
    last = r.cpus.map(c => c.origin);
  }
  console.log(
    `A minute of roaming: ${r.cpus.map((c, n) => `CPU ${n + 1} rode ${far[n].toFixed(0)} ft, visits ${c.roam.visits}, turns ${c.roam.turns}, back-outs ${c.escapes}, resets ${c.resets}, wrecks ${c.wrecks}, stalls ${c.stalls}`).join('; ')}`
  );
  assert.ok(far.every(feet => feet > 600), `each CPU keeps riding: ${far}`);
  // Steep ground is kept off, so a roamer rarely wrecks.
  assert.ok(r.cpus.every(c => c.wrecks <= 3), `wrecks: ${r.cpus.map(c => c.wrecks)}`);
  await shot(page, 'roaming-later');
  assert.deepEqual([r.game, r.tag, r.hockey], [null, null, null]);
  const twoCpus = await frames(page, 10);
  console.log(`Frame times, 2 CPU riders: ${summary(twoCpus)}`);

  // Three CPU riders: the frame time beside the option-off ride's.
  await page.evaluate(() => localStorage.setItem('atv-free-ride-cpus', '3'));
  r = await freeRide(page);
  assert.equal(r.cpus.length, 3);
  const start3 = r.cpus.map(c => c.origin);
  await until(page, r => r.cpus.every((c, i) => flat(c.origin, start3[i]) > 60), 60000);
  const on = await frames(page, 10);
  console.log(`Frame times, 3 CPU riders: ${summary(on)}`);
  await shot(page, 'three-cpus');

  // The ride still ends: Pause → Main menu.
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  await click(page, 'main');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page != null, null, {
    timeout: 60000
  });
  assertNoErrors(errors);
  console.log('Free ride CPU riders: off by default, the Game Settings row, roaming, hazards and frame times verified');
} finally {
  await browser.close();
}
