// Shared Rust pause menu: open/resume, navigation and Back, settings rows
// persisted under the browser game's keys, sliders, and touch activation.
// Host browser evidence; parity images come from work capture scripts.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const menu = async page => (await read(page)).toolbar.menu;
const wait = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 30000 });
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === name, name);
const centre = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  const m = await menu(page), hit = m.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m.page}: ${m.choices}`);
  const at = centre(hit);
  await page.mouse.click(at.x, at.y); await nextFrames(page, 3);
}

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(page, 30);
  await page.mouse.click(640, 400);
  await key(page, 'Escape');
  await onPage(page, 'pause');
  let m = await menu(page);
  assert.deepEqual(m.choices.slice(0, 3), ['resume', 'reset', 'settings']);
  for (const hit of m.hits) assert.ok(hit.height >= 44 && hit.x >= 0 && hit.x + hit.width <= 1280);
  assert.equal((await read(page)).paused, true);
  // Keyboard navigation and Back return to the row that opened the page.
  await key(page, 'ArrowDown'); await key(page, 'ArrowDown');
  assert.equal((await menu(page)).selection, 2);
  await key(page, 'Enter'); await onPage(page, 'settings');
  await key(page, 'Escape'); await onPage(page, 'pause');
  assert.equal((await menu(page)).choices[(await menu(page)).selection], 'settings');
  // Display → Minimap configuration: rows and sliders save under minimap.js keys.
  await click(page, 'settings'); await onPage(page, 'settings');
  await click(page, 'hud-settings'); await onPage(page, 'hud-settings');
  await click(page, 'minimap-config'); await onPage(page, 'minimap-config');
  await click(page, 'minimap-toggle');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-minimap-enabled')), 'false');
  m = await menu(page);
  const size = m.hits.find(h => h.action === 'minimap-size');
  await page.mouse.move(size.x + size.width / 2, size.y + 22); await page.mouse.down();
  await page.mouse.move(size.x + 2, size.y + 22, { steps: 4 }); await page.mouse.up(); await nextFrames(page, 3);
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-minimap-size')), '96');
  // Controls: Left/Right step the steering slider (saved like touch.js).
  await key(page, 'Escape'); await onPage(page, 'hud-settings');
  await key(page, 'Escape'); await onPage(page, 'settings');
  await click(page, 'controls'); await onPage(page, 'controls');
  await key(page, 'ArrowRight');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-mobile-steering-sensitivity')), '90');
  // Keyboard bindings: capture a new Gas key, saved in play.html's profile.
  await click(page, 'keyboard'); await onPage(page, 'bindings');
  m = await menu(page);
  assert.ok(m.choices.includes('bind:throttle') && m.choices.includes('bindnext'));
  await click(page, 'bind:throttle');
  assert.deepEqual((await menu(page)).choices, ['bindcancel'], 'capture shows Cancel only');
  await key(page, 'KeyY');
  await wait(page, () => JSON.parse(localStorage.getItem('atv2-gameplay-bindings-v1') ?? 'null')?.keyboard?.throttle?.[0]?.key === 'KeyY');
  await click(page, 'bindreset');
  await wait(page, () => JSON.parse(localStorage.getItem('atv2-gameplay-bindings-v1')).keyboard.throttle[0].key === 'KeyW');
  await key(page, 'Escape'); await onPage(page, 'controls');
  // Controller settings: preset and tuning save under the browser's keys.
  await click(page, 'controller'); await onPage(page, 'controller-settings');
  await click(page, 'padpreset');
  await wait(page, () => JSON.stringify(JSON.parse(localStorage.getItem('atv2-gameplay-bindings-v1')).controller.reset) === JSON.stringify([{chord: [{button: 4}, {button: 5}]}]) && JSON.parse(localStorage.getItem('atv2-gameplay-bindings-v1')).controller.camera[0].button === 8);
  m = await menu(page);
  const steering = m.hits.find(h => h.action === 'pad-steering');
  await page.mouse.click(steering.x + steering.width - 2, steering.y + 22); await nextFrames(page, 3);
  assert.equal(JSON.parse(await page.evaluate(() => localStorage.getItem('atv2-controller-v1'))).steering, 150);
  await key(page, 'Escape'); await onPage(page, 'controls');
  await key(page, 'Escape'); await onPage(page, 'settings');
  // Extras rows share play.html's keys.
  await click(page, 'extras'); await onPage(page, 'extras');
  await click(page, 'flips');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-flips')), 'unleashed');
  // Escape walks back to Pause, then resumes.
  for (const to of ['settings', 'pause']) { await key(page, 'Escape'); await onPage(page, to); }
  await key(page, 'Escape');
  await wait(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).paused);
  assert.equal(await menu(page), null);
  // A Pause tap shorter than a frame still pauses (queued key edges).
  await page.keyboard.press('Escape');
  await onPage(page, 'pause');
  await key(page, 'Escape');
  await wait(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).paused);
  assertNoErrors(errors);
  await page.close();

  // Touch: the toolbar Pause opens the menu, Resume taps close it.
  const phone = await newPage(browser, { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true }, errors);
  await phone.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
  await phone.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(phone, 30);
  const pause = (await read(phone)).toolbar.hits.find(h => h.id === 1);
  await phone.touchscreen.tap(pause.x + pause.width / 2, pause.y + pause.height / 2);
  await onPage(phone, 'pause');
  const resume = (await menu(phone)).hits.find(h => h.action === 'resume');
  assert.ok(resume.width <= 390 - 24 && resume.x >= 12, 'portrait panel inset');
  await phone.touchscreen.tap(resume.x + resume.width / 2, resume.y + resume.height / 2);
  await wait(phone, () => !JSON.parse(document.querySelector('#ui-state').textContent).paused);
  assertNoErrors(errors);
  console.log('PASS: Rust pause menu navigation, Back, settings persistence, sliders and touch');
} finally {
  await browser.close();
}
