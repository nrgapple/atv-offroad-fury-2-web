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
  console.log('actor collision: ok');
} finally {
  await browser.close();
}
