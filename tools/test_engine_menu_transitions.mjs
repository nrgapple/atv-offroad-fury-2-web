// Rust menu page transitions (frontend.js animatePage): each page shown slides
// in from translateX(36px) and opacity 0 over 240 ms (cubic-bezier(.16,1,.3,1))
// on the frontend, over the main menu (Settings) and in the ride's pause menu;
// input keeps working during the slide; prefers-reduced-motion shows pages at
// rest. Reads the engine's #ui-state (`motion`: [opacity, translateX]).
// Run: node tools/run_browser_tests.mjs --only test_engine_menu_transitions.mjs (content group).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
/** Records `motion` every frame for 1.5 s around `trigger`. */
async function record(page, surface, trigger) {
  await page.evaluate(surface => {
    const pick = surface === 'front' ? s => s.front?.motion : s => s.toolbar?.menu?.motion;
    const samples = window.__slide = [];
    const t0 = performance.now();
    const step = () => {
      const state = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      samples.push([performance.now() - t0, pick(state) ?? null]);
      if (performance.now() - t0 < 1500) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  }, surface);
  await trigger();
  await page.waitForFunction(() => window.__slide.at(-1)?.[0] >= 1500, null, { timeout: 30000 });
  return page.evaluate(() => window.__slide);
}

/** The samples show one slide in: moving right to left and fading in, then at rest. */
function assertSlide(samples, label) {
  const moving = samples.filter(([, m]) => m);
  assert.ok(moving.length >= 2, `${label}: slide frames ${JSON.stringify(samples.slice(0, 12))}`);
  const [first] = moving;
  assert.ok(first[1][1] > 0 && first[1][1] <= 36, `${label}: starts within 36px ${first}`);
  assert.ok(first[1][0] >= 0 && first[1][0] < 1, `${label}: starts faded ${first}`);
  for (let i = 1; i < moving.length; i++) {
    assert.ok(moving[i][1][1] <= moving[i - 1][1][1] + 1e-4, `${label}: moves left`);
    assert.ok(moving[i][1][0] >= moving[i - 1][1][0] - 1e-4, `${label}: fades in`);
  }
  // 240 ms of slide time; a stalled frame advances it by at most 33 ms.
  assert.ok(moving.at(-1)[0] - first[0] < 1000, `${label}: rests after 240 ms (${moving.at(-1)[0] - first[0]} ms)`);
  assert.equal(samples.at(-1)[1], null, `${label}: at rest`);
}

const browser = await launch();
const errors = [];
try {
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/engine.html?uiDiagnostics=1'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title', null, 120000);
  await wait(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).front.motion);
  await page.mouse.click(8, 8);

  // Title → main menu: the page slides in, and Down works during the slide.
  const samples = await record(page, 'front', async () => {
    await page.keyboard.press('Enter');
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.motion);
    await page.keyboard.press('ArrowDown');
  });
  assertSlide(samples, 'main menu');
  let state = await read(page);
  assert.equal(state.front.page, 'main');
  assert.equal(state.front.selection, 1, 'input during the slide moves the selection');
  // A selection change is not a page: nothing moves.
  const still = await record(page, 'front', () => key(page, 'ArrowUp'));
  assert.ok(still.every(([, m]) => !m), 'moving the selection does not slide');

  // Settings over the main menu: its panel slides in; Back slides the menu in.
  const steps = state.front.choices.indexOf('settings');
  assert.ok(steps > 0, state.front.choices.join());
  for (let i = 0; i < steps; i++) await key(page, 'ArrowDown');
  assertSlide(await record(page, 'front', () => page.keyboard.press('Enter')), 'settings panel');
  assert.equal((await read(page)).front.menu?.page, 'settings');
  assertSlide(await record(page, 'front', () => page.keyboard.press('Escape')), 'back to main');
  assert.equal((await read(page)).front.menu, null);

  // Ride pause menu: Pause and each page it opens slide in.
  await page.goto(url('/engine.html?venue=env01&uiDiagnostics=1'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar, null, 120000);
  await nextFrames(page, 30);
  await page.mouse.click(640, 400);
  assertSlide(await record(page, 'pause', () => page.keyboard.press('Escape')), 'pause');
  assert.equal((await read(page)).toolbar.menu.page, 'pause');
  const pause = (await read(page)).toolbar.menu;
  for (let i = 0; i < pause.choices.indexOf('settings'); i++) await key(page, 'ArrowDown');
  assertSlide(await record(page, 'pause', () => page.keyboard.press('Enter')), 'pause settings');
  assert.equal((await read(page)).toolbar.menu.page, 'settings');
  await page.close();

  // prefers-reduced-motion: pages open at rest.
  const reduced = await newPage(browser, { viewport: { width: 1280, height: 720 }, reducedMotion: 'reduce' }, errors);
  await reduced.goto(url('/engine.html?uiDiagnostics=1'));
  await wait(reduced, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title', null, 120000);
  await reduced.mouse.click(8, 8);
  const rest = await record(reduced, 'front', () => reduced.keyboard.press('Enter'));
  assert.equal((await read(reduced)).front.page, 'main');
  assert.ok(rest.every(([, m]) => !m), `reduced motion: ${JSON.stringify(rest.find(([, m]) => m))}`);
  await reduced.close();

  assertNoErrors(errors);
  console.log('PASS Rust menu page transitions: frontend, settings over the menu, pause menu, reduced motion');
} finally {
  await browser.close();
}
