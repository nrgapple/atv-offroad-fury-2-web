// Keyframed TOY actors in the Rust engine (research/AMBIENT_ACTORS.md):
// Rockbridge's 25 crows and windmill from the venue TOY, posed by their
// looping MOTs on the 60 Hz scene clock.
//
// 1. ATV Freeride on env02: 26 actors on two shared models and motions;
//    the crows fly (their posed roots move) and every motion clock follows
//    the scene steps; pause freezes the clock, resume continues it.
// 2. A motorcycle on env02 with a fixed camera on the windmill: the actor
//    pass poses and draws it inside the view and culls the rest.
// 3. A National race on nat03 loads the race archive's 26 seagulls.
// ATV_ACTORS_SCREENSHOT=<prefix> saves the windmill view.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const until = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const ready = page =>
  until(page, () => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return s.step > 5 && s.actors;
  });
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
const actorSteps = (page, above) =>
  until(page, n => JSON.parse(document.querySelector('#ui-state').textContent).actors.steps > n, above, 60000);

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));

  // 1. ATV Freeride.
  await page.goto(url('/play.html?venue=env02&uiDiagnostics=1&weather=off'));
  await ready(page);
  let s = await state(page);
  const first = s.actors;
  assert.equal(first.count, 26, JSON.stringify(first.list.map(a => a.mcf)));
  assert.equal(first.models, 2, 'crows share one model');
  assert.equal(first.clips, 2, 'and one motion');
  assert.equal(first.list.filter(a => a.model === 'crow').length, 25);
  const windmill = first.list.find(a => a.model === 'windmill');
  assert.ok(windmill, 'the windmill');
  // No random phase: every actor starts its motion at the scene clock.
  assert.ok(
    first.list.every(a => a.time === first.list[0].time),
    'shared start phase'
  );
  await actorSteps(page, first.steps + 60);
  s = await state(page);
  const later = s.actors;
  const crows = later.list.map((a, i) => [a, first.list[i]]).filter(([a]) => a.model === 'crow');
  const moved = crows.filter(([a, b]) => Math.hypot(...a.root.map((v, k) => v - b.root[k])) > 1);
  assert.ok(moved.length >= 20, `crows fly: ${moved.length} of 25 moved`);
  assert.ok(later.list[0].time > first.list[0].time, 'motion clocks advance');
  assert.ok(later.visible + later.culled === 26, JSON.stringify({ visible: later.visible, culled: later.culled }));
  // Pause freezes the scene clock; resume continues it.
  await page.mouse.click(100, 100);
  await key(page, 'Escape');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).paused === true, null, 30000);
  const paused = (await state(page)).actors.steps;
  await page.waitForTimeout(500); // deliberately time-based: nothing may advance while paused
  assert.equal((await state(page)).actors.steps, paused, 'paused actors hold');
  await key(page, 'Escape');
  await actorSteps(page, paused + 10);
  assertNoErrors(errors);

  // 2. A motorcycle with a fixed camera 120 ft south of the windmill.
  const [x, y, z] = windmill.root;
  const [cx, cy, cz] = [x, y + 30, z - 120];
  const yaw = Math.atan2(x - cx, z - cz);
  const pitch = Math.atan2(y - cy, Math.hypot(x - cx, z - cz));
  await page.goto(
    url(`/play.html?venue=env02&quad=29&uiDiagnostics=1&weather=off&camera=${cx},${cy},${cz},${yaw},${pitch},1.7`)
  );
  await ready(page);
  await nextFrames(page, 10);
  s = await state(page);
  assert.equal(s.actors.count, 26);
  assert.ok(s.actors.visible >= 1 && s.actors.draws >= 2, JSON.stringify(s.actors));
  assert.ok(s.actors.culled >= 1, 'actors outside the view are culled');
  await optionalScreenshot(page, 'ATV_ACTORS_SCREENSHOT', 'windmill');
  const steps = s.actors.steps;
  await actorSteps(page, steps + 30);
  assertNoErrors(errors);

  // 3. A National race on Pleasant Valley loads the race TOY's seagulls.
  await page.goto(url('/play.html?venue=nat03&race=1&laps=1&cpu=0&uiDiagnostics=1&weather=off'));
  await ready(page);
  s = await state(page);
  assert.equal(s.actors.count, 26, JSON.stringify(s.actors.list.map(a => a.mcf)));
  assert.ok(s.actors.list.every(a => a.model === 'seagull'));
  await actorSteps(page, s.actors.steps + 30);
  assertNoErrors(errors);
  console.log('TOY actors: env02 crows and windmill animate and pause, motorcycle view draws, nat03 race seagulls');
} finally {
  await browser.close();
}
