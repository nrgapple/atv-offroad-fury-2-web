// Host/public integration with original resources. Never runs in private-free CI.
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import {
  assertNoErrors,
  base,
  button,
  holdKey,
  launch,
  newPage,
  nextFrames,
  waitForAnimations,
  waitForScreen,
  waitForSteps
} from './browser/harness.mjs';
const browser = await launch();
const output = process.env.ATV_RACE_SCREENSHOTS;
if (output) mkdirSync(output, { recursive: true });
const errors = [];
try {
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, hasTouch: true }, errors);
  const screen = name => waitForScreen(page, name);
  const state = () => page.evaluate(() => window.atvTerrainStatus);
  await page.goto(base);
  await screen('title');
  await button(page, 'Press START').click();
  await screen('main');
  await button(page, 'Single Player').click();
  await screen('single');
  if (process.env.ATV_RACE_MODEL) {
    const id = Number(process.env.ATV_RACE_MODEL);
    assert(id >= 1 && id <= 46);
    await button(page, 'Quad and Rider').click();
    const next = id - 1,
      previous = 47 - id;
    for (let i = 0; i < Math.min(next, previous); i++)
      await button(page, next <= previous ? 'Next quad' : 'Previous quad').click();
    await page.waitForFunction(
      id => [...document.querySelectorAll('.model-view canvas')].every(c => c.dataset.selection === `${id}:0`),
      id
    );
    await button(page, 'Done').click();
  }
  await button(page, 'Race').click();
  await screen('race');
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
    const start = hits.find(h => h.label === 'Start Race').rect,
      back = hits.find(h => h.label === 'Back').rect;
    assert(start.bottom <= back.y || start.right <= back.x || start.x >= back.right, 'Start and Back must not overlap');
    if (output) await page.screenshot({ path: `${output}/race-setup-${name}.png` });
  }
  await page.setViewportSize({ width: 390, height: 844 });
  await button(page, 'Laps: 3').tap();
  await button(page, 'Laps: 4').waitFor();
  await page.keyboard.press('ArrowLeft');
  await button(page, 'Laps: 3').waitFor();
  await button(page, 'CPU riders: 3').tap();
  await button(page, 'CPU riders: 4').waitFor();
  await button(page, 'Start Race').tap();
  await page.waitForFunction(
    () => document.querySelector('#frontend').hidden && window.atvTerrainStatus.frame?.race?.phase === 'countdown',
    null,
    { timeout: 120000 }
  );
  let s = await state();
  assert.equal(s.minimap.visible, false, 'Freestyle GPS stays out of Race');
  assert.equal(s.minimap.buildSlices, 0, 'Race does not generate GPS pixels');
  assert.equal(s.frame.race.field, 5);
  assert.equal(s.frame.race.checkpoints, 252);
  assert.deepEqual(
    s.frame.race.opponents.map(o => o.name),
    ['YUKARI', 'PANZER', 'MCNABB', 'BALS']
  );
  const startOrigin = s.frame.origin;
  await page.keyboard.down('KeyW');
  await waitForSteps(page, 30);
  s = await state();
  assert.deepEqual(s.frame.origin, startOrigin, 'Countdown blocks throttle');
  await button(page, 'Pause').tap();
  await screen('pause');
  // Time-based on purpose: let an in-flight step land, then prove the paused race stays frozen over wall-clock time.
  await page.waitForTimeout(100);
  const paused = await state();
  await page.waitForTimeout(250);
  assert.deepEqual((await state()).frame, paused.frame, 'Pause freezes all racers and clocks');
  await button(page, 'Resume').tap();
  await page.waitForFunction(() => window.atvTerrainStatus.frame.race.phase === 'racing', null, { timeout: 15000 });
  await page.keyboard.up('KeyW');
  await holdKey(page, 'KeyW', 90);
  assert((await state()).frame.speed > 1, 'Player can accelerate in the race');
  await waitForSteps(page, 30);
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [{ id: 1, x: 180, y: 620, radiusX: 6, radiusY: 6, force: 1 }]
  });
  await page.waitForFunction(() => window.atvTerrainStatus.input?.touch_drive?.held);
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [{ id: 1, x: 210, y: 610, radiusX: 6, radiusY: 6, force: 1 }]
  });
  await page.waitForFunction(() => Math.abs(window.atvTerrainStatus.input?.steer) > 0.05);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchCancel', touchPoints: [] });
  await page.waitForFunction(() => !window.atvTerrainStatus.input?.touch_drive?.held);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.race.elapsed >= 4, null, { timeout: 30000 });
  s = await state();
  assert(s.frame.race.elapsed >= 4);
  assert(s.frame.race.opponents.every(o => o.distance > 40));
  assert.equal(s.glError, 0);
  if (Number(process.env.ATV_RACE_MODEL || 1) < 27) {
    for (const cpu of s.frame.race.opponents) {
      assert(cpu.physical, 'CPU must use an independent physical quad');
      const p = cpu.physical;
      assert(p.orientation.every(Number.isFinite));
      assert(p.speed > 1, 'CPU accelerates through drivetrain');
      assert(
        p.wheel_angles.some(a => Math.abs(a) > 0.1),
        'CPU wheels must roll'
      );
      assert.equal(p.quad_matrices.length, 27);
      assert.equal(p.rider_matrices.length, 18);
      assert(p.quad_matrices.flat().every(Number.isFinite));
      assert(p.rider_matrices.flat().every(Number.isFinite));
      assert(p.decisions.desired_speed >= 0);
      assert(p.decisions.route_error < 40, 'CPU must follow track');
    }
    assert(
      s.frame.race.opponents.some(o => o.physical.wheel_compression.some(c => Math.abs(c) > 0.01)),
      'CPU suspension responds to terrain'
    );
  } else {
    assert(
      s.frame.race.opponents.every(o => !o.physical),
      'Motorcycle opponents retain their explicitly authored route preview'
    );
  }

  for (const [name, size] of [
    ['portrait', { width: 390, height: 844 }],
    ['landscape', { width: 844, height: 390 }],
    ['desktop', { width: 1280, height: 720 }]
  ]) {
    await page.setViewportSize(size);
    await waitForSteps(page, 30);
    assert(await page.locator('#race-overlay').isVisible());
    const hud = await page
      .locator('.race-strip,.race-times,.race-guide')
      .evaluateAll(nodes => nodes.map(n => n.getBoundingClientRect().toJSON()));
    for (const rect of hud)
      assert(rect.x >= 0 && rect.right <= size.width + 1 && rect.bottom <= size.height + 1, JSON.stringify(rect));
    if (output) await page.screenshot({ path: `${output}/race-driving-${name}.png` });
  }
  await button(page, 'Pause').click();
  await screen('pause');
  await button(page, 'Restart Race').click();
  await page.waitForFunction(
    () => document.querySelector('#frontend').hidden && window.atvTerrainStatus.frame?.race?.phase === 'countdown',
    null,
    { timeout: 120000 }
  );
  assert.equal((await state()).frame.race.elapsed, 0);
  assert.equal(await page.locator('#disc').isHidden(), true);
  // Render-only results fixture exercises finish UI without mutating race state
  // or claiming a complete original-course lap replay.
  await button(page, 'Pause').click();
  await screen('pause');
  await page.evaluate(async () => {
    const { RaceOverlay } = await import('/race.js');
    document.querySelector('#race-overlay').remove();
    window.raceActions = [];
    window.resultFixture = new RaceOverlay({
      restart: () => window.raceActions.push('retry'),
      quit: () => window.raceActions.push('quit')
    });
    const player = window.atvTerrainStatus.frame;
    window.resultFixtureFrame = {
      ...player.race,
      phase: 'finished',
      elapsed: 123.4,
      best: 60,
      position: 2,
      lap_time: 63.4,
      standings: player.race.standings.map((r, i) => ({ ...r, laps: 3, time: i < 2 ? 123.4 + i : null }))
    };
    window.resultFixture.draw(window.resultFixtureFrame, player, true);
  });
  for (const size of [
    { width: 390, height: 844 },
    { width: 844, height: 390 }
  ]) {
    await page.setViewportSize(size);
    await page.waitForFunction(w => Math.abs(document.body.getBoundingClientRect().width - w) < 1, size.width);
    const result = await page.locator('.race-results').boundingBox();
    assert(
      result.x >= 0 &&
        result.y >= 0 &&
        result.x + result.width <= size.width + 1 &&
        result.y + result.height <= size.height + 1,
      `results fit ${JSON.stringify(size)}: ${JSON.stringify(result)}`
    );
    if (output) await page.screenshot({ path: `${output}/race-results-${size.width}.png` });
  }
  await button(page, 'Race Again').click();
  assert.deepEqual(await page.evaluate(() => window.raceActions), ['retry']);
  await button(page, 'Single Player').click();
  assert.deepEqual(await page.evaluate(() => window.raceActions), ['retry', 'quit']);
  assertNoErrors(errors);
  console.log(
    JSON.stringify({
      gate: 'race',
      coursePoints: s.frame.race.checkpoints,
      field: s.frame.race.field,
      CPU: s.frame.race.opponents.map(o => ({
        name: o.name,
        distance: o.distance,
        speed: o.physical?.speed,
        grounded: o.physical?.grounded,
        steer: o.physical?.decisions.steer,
        lane: o.physical?.decisions.lane_offset
      })),
      errors
    })
  );
} finally {
  await browser.close();
}
