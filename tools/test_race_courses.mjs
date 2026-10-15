// Host/public integration with original resources. Never runs in private-free CI.
// Starts the original race on every Environment, National and Supercross course
// (or ATV_RACE_COURSES=env02,sx07) and checks the start gate, field and racing.
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { RACE_COURSES } from '../web/launch.js';
import {
  assertNoErrors,
  base,
  button,
  holdKey,
  launch,
  newPage,
  nextFrames,
  waitForAnimations,
  waitForScreen
} from './browser/harness.mjs';
const output = process.env.ATV_RACE_SCREENSHOTS;
if (output) mkdirSync(output, { recursive: true });
const courses = process.env.ATV_RACE_COURSES ? process.env.ATV_RACE_COURSES.split(',') : RACE_COURSES;
const browser = await launch();
const errors = [],
  results = [];
try {
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, hasTouch: true }, errors);
  const screen = name => waitForScreen(page, name);
  const state = () => page.evaluate(() => window.atvTerrainStatus);
  // Menu: the Course row cycles every published original course in both directions.
  await page.goto(base);
  await screen('title');
  await button(page, 'Press START').click();
  await screen('main');
  await button(page, 'Single Player').click();
  await screen('single');
  await button(page, 'Race').click();
  await screen('race');
  const course = () => page.locator('#menu-buttons button[aria-label^="Course: "]').getAttribute('aria-label');
  const first = await course();
  for (const [name, size] of [
    ['desktop', { width: 1280, height: 720 }],
    ['portrait', { width: 390, height: 844 }],
    ['landscape', { width: 844, height: 390 }]
  ]) {
    await page.setViewportSize(size);
    await waitForAnimations(page);
    await nextFrames(page, 2);
    const hits = await page
      .locator('#menu-buttons button')
      .evaluateAll(buttons =>
        buttons.map(b => ({ label: b.getAttribute('aria-label'), rect: b.getBoundingClientRect().toJSON() }))
      );
    for (const hit of hits) {
      assert(
        hit.rect.x >= 0 && hit.rect.y >= 0 && hit.rect.right <= size.width + 1 && hit.rect.bottom <= size.height + 1,
        JSON.stringify(hit)
      );
      assert(hit.rect.height >= 44, JSON.stringify(hit));
    }
    for (const [i, a] of hits.entries())
      for (const b of hits.slice(i + 1))
        assert(
          a.rect.right <= b.rect.x ||
            b.rect.right <= a.rect.x ||
            a.rect.bottom <= b.rect.y ||
            b.rect.bottom <= a.rect.y,
          `${a.label} overlaps ${b.label}`
        );
    if (output) await page.screenshot({ path: `${output}/race-courses-setup-${name}.png` });
  }
  const changed = from =>
    page.waitForFunction(
      from =>
        document.querySelector('#menu-buttons button[aria-label^="Course: "]')?.getAttribute('aria-label') !== from,
      from
    );
  await page.locator('#menu-buttons button[aria-label^="Course: "]').click();
  await changed(first);
  const second = await course();
  await page.keyboard.press('ArrowLeft');
  await changed(second);
  assert.equal(await course(), first);
  for (const id of courses) {
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.goto(`${base}/play.html?mode=single&activity=race&map=${id}&cpu=4&laps=2&quad=1`);
    await page.waitForFunction(
      () =>
        window.atvTerrainStatus?.frame?.race?.phase === 'countdown' ||
        document.querySelector('#frontend')?.dataset.error,
      null,
      { timeout: 180000 }
    );
    assert.equal(
      await page.locator('#frontend').getAttribute('data-error'),
      null,
      `${id}: ${await page.locator('#menu-notice').textContent()}`
    );
    let s = await state();
    const race = s.frame.race;
    assert.equal(race.field, 5, id);
    assert.equal(race.laps, 2, id);
    assert.equal(race.opponents.length, 4, id);
    assert(race.checkpoints >= 40, `${id} route`);
    assert(!race.wrong_way, `${id} grid faces the race direction`);
    // Riders line up across the original start gate, one RiderSpacing apart.
    const xs = race.opponents.map(o => o.origin);
    const gap = Math.hypot(xs[1][0] - xs[2][0], xs[1][2] - xs[2][2]);
    assert(gap >= 15 && gap <= 31, `${id} grid gap ${gap}`);
    await page.keyboard.down('KeyW');
    await page.waitForFunction(() => window.atvTerrainStatus.frame.race.elapsed >= 3, null, { timeout: 60000 });
    await page.keyboard.up('KeyW');
    s = await state();
    assert.equal(s.frame.race.phase, 'racing', id);
    assert.equal(s.glError, 0, id);
    assert(s.frame.race.checkpoint >= 1, `${id} player progressed`);
    assert(s.frame.race.opponents.filter(o => o.distance > 40).length >= 3, `${id} CPUs drive`);
    assert(
      s.frame.race.opponents.every(o => o.physical && o.physical.quad_matrices.flat().every(Number.isFinite)),
      `${id} CPU physics`
    );
    assert(await page.locator('#race-overlay').isVisible());
    if (output) await page.screenshot({ path: `${output}/race-course-${id}.png` });
    if (id === courses[0]) {
      // Leave the track: progress pauses, the HUD points back to the racing line,
      // and Reset returns to that line without restarting the race.
      await page.keyboard.down('KeyW');
      await holdKey(page, 'KeyD', 42);
      await page.waitForFunction(() => window.atvTerrainStatus.frame.race.off_track, null, { timeout: 30000 });
      await page.keyboard.up('KeyW');
      const off = (await state()).frame.race;
      assert.match(await page.locator('.race-direction').textContent(), /OFF TRACK/);
      await page.keyboard.press('Backspace');
      await page.waitForFunction(
        () => window.atvTerrainStatus.frame?.race && !window.atvTerrainStatus.frame.race.off_track,
        null,
        { timeout: 15000 }
      );
      s = await state();
      assert.equal(s.frame.race.phase, 'racing');
      assert(s.frame.race.elapsed >= off.elapsed, 'Reset keeps the race running');
      assert(
        Math.hypot(s.frame.origin[0] - off.track_point[0], s.frame.origin[2] - off.track_point[2]) < 20,
        'Reset returns to the racing line'
      );
    }
    results.push({
      id,
      checkpoints: race.checkpoints,
      gap: Number(gap.toFixed(1)),
      player_checkpoint: s.frame.race.checkpoint,
      cpu: s.frame.race.opponents.map(o => Math.round(o.distance))
    });
  }
  assertNoErrors(errors);
  console.log(JSON.stringify({ gate: 'race-courses', courses: results.length, results }));
  console.log(
    `PASS: ${results.length} original race courses start from their gates with four physical CPUs; course menu cycles in three layouts; no page or GL errors.`
  );
} finally {
  await browser.close();
}
