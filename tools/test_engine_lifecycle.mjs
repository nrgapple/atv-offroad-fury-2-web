// A persisted pagehide/pageshow in the Rust engine (game.js pagehide and
// pageshow, tools/test_lifecycle.mjs): the ride freezes while the page sits
// in the back/forward cache, resumes on pageshow without the pause menu, and
// a throttle held before the page hid is cleared until pressed again. The
// lifecycle events are synthetic: this proves recovery semantics, not that a
// browser used its back/forward cache. Host browser evidence. Content group.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const until = (page, predicate, arg, timeout = 30000) => page.waitForFunction(predicate, arg, { timeout });

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 844, height: 390 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(page, 20);
  await page.mouse.click(420, 200);
  await page.keyboard.down('KeyW');
  await until(page, () => {
    const s = JSON.parse(document.querySelector('#ui-state').textContent);
    return s.controls.throttle === 1 && s.speed > 10;
  });
  const before = await state(page);
  await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true })));
  await page.keyboard.up('KeyW');
  // Time-based on purpose: the frozen ride must not step over wall-clock time.
  await page.waitForTimeout(300);
  const frozen = (await state(page)).step;
  await page.waitForTimeout(300);
  assert.equal((await state(page)).step, frozen, 'a hidden page does not simulate');
  await page.evaluate(() => {
    window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
  });
  await until(page, step => JSON.parse(document.querySelector('#ui-state').textContent).step > step + 15, frozen, 10000);
  let s = await state(page);
  assert.equal(s.paused, false, 'pageshow resumes the ride, not the pause menu');
  assert.equal(s.toolbar?.menu ?? null, null, 'no menu over the resumed ride');
  assert.equal(s.controls.throttle, 0, 'the throttle held before the page hid is cleared');
  await page.keyboard.down('KeyW');
  await until(page, step => {
    const s = JSON.parse(document.querySelector('#ui-state').textContent);
    return s.step > step + 60 && s.controls.throttle === 1;
  }, frozen, 15000);
  await page.keyboard.up('KeyW');
  s = await state(page);
  assert.notDeepEqual(s.origin, before.origin, 'the ride moves on');
  assertNoErrors(errors);
  console.log(`PASS: Rust persisted pagehide froze at step ${frozen}, pageshow resumed without a menu and cleared the held throttle`);
} finally {
  await browser.close();
}
