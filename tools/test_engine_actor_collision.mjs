// UseCollision TOY actors collide in the Rust engine
// (research/AMBIENT_ACTORS.md, Collision): Rockbridge's windmill registers
// its original type-0 anim\windmill.col in the ride's dynamic scene list at
// the actor's placement. `?actorPlace=windmill` puts the quad 40 ft south of
// that body at 30 ft/s heading at it; with the throttle held it reaches the
// tower, makes moving-body chassis contact and does not pass through.
// ATV_ACTOR_COLLISION_SCREENSHOT=<prefix> saves the contact view.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, optionalScreenshot, url } from './browser/harness.mjs';

const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?venue=env02&uiDiagnostics=1&weather=off&actorPlace=windmill'));
  await page.waitForFunction(
    () => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return s.step > 5 && s.actors;
    },
    null,
    { timeout: 120000 }
  );
  let s = await state(page);
  assert.equal(s.actors.collision.length, 1, JSON.stringify(s.actors.collision));
  const body = s.actors.collision[0];
  assert.equal(body.mcf.toLowerCase(), 'windmill.mcf');
  assert.equal(s.actors.collisionUnresolved, 0);
  await page.mouse.click(100, 100);
  await page.keyboard.down('ArrowUp');
  let contacts = 0;
  let closest = Infinity;
  let shot = false;
  for (let i = 0; i < 120; i++) {
    await page.waitForTimeout(50); // sample the live ride while it drives
    s = await state(page);
    const origin = s.actors.vehicle;
    if (s.actors.dynamicContacts > 0) {
      contacts++;
      if (!shot) {
        await optionalScreenshot(page, 'ATV_ACTOR_COLLISION_SCREENSHOT', 'contact');
        shot = true;
      }
    }
    if (origin) {
      closest = Math.min(closest, Math.hypot(origin[0] - body.position[0], origin[2] - body.position[2]));
    }
  }
  await page.keyboard.up('ArrowUp');
  console.log(JSON.stringify({ contacts, closest }));
  assert.ok(contacts > 0, 'the quad touches the windmill body');
  if (Number.isFinite(closest)) assert.ok(closest > 1, `the quad passed through the tower (${closest})`);
  assertNoErrors(errors);
  // Ventura County: all three animated funnels carry ten collision sections.
  await page.goto(url('/play.html?venue=free06&uiDiagnostics=1&weather=off&actorPlace=twister3'));
  await page.waitForFunction(() => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return s.step > 5 && s.actors;
  }, null, { timeout: 120000 });
  s = await state(page);
  assert.equal(s.actors.collision.length, 30);
  assert.equal(s.actors.collisionUnresolved, 0);
  const before = s.actors.collision;
  // Stay parked in the funnel's path: its motion must supply the launch.
  await page.mouse.click(100, 100);
  let tornadoContacts = 0;
  let upwardSpeed = 0;
  let spinSpeed = 0;
  for (let i = 0; i < 160; i++) {
    await page.waitForTimeout(50);
    s = await state(page);
    tornadoContacts += Number(s.actors.dynamicContacts > 0);
    if (tornadoContacts > 0) {
      upwardSpeed = Math.max(upwardSpeed, s.actors.velocity[1]);
      spinSpeed = Math.max(spinSpeed, Math.hypot(...s.actors.angularVelocity));
    }
  }
  await page.keyboard.up('ArrowUp');
  assert.ok(s.actors.collision.some((b, i) =>
    Math.hypot(...b.position.map((v, k) => v - before[i].position[k])) > 1), 'funnel collision moves');
  assert.ok(tornadoContacts > 0, 'ATV contacts the animated funnel');
  console.log(JSON.stringify({ tornadoContacts, upwardSpeed, spinSpeed }));
  assert.ok(upwardSpeed > 8, `funnel must lift the ATV, observed ${upwardSpeed} ft/s`);
  assert.ok(spinSpeed > 1, `funnel must disturb the ATV orientation, observed ${spinSpeed} rad/s`);
  await optionalScreenshot(page, 'ATV_ACTOR_COLLISION_SCREENSHOT', 'tornado');
  assertNoErrors(errors);
  console.log(`actor collision: windmill and tornado ok (${tornadoContacts} tornado contact samples)`);
} finally {
  await browser.close();
}
