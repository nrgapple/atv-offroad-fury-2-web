// Real Controls slider and touch input gate; no vehicle state injection.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, launch, newPage, nextFrames, waitForAnimations } from './browser/harness.mjs';
const engine = process.env.ATV_UI_BROWSER === 'webkit' ? 'webkit' : 'chromium';
const browser = await launch({ engine });
try {
  const errors = []; const page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true }, errors);
  await page.goto(new URL('/play.html?drive=1&environment=env01', base).href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  const open = async () => {
    await page.locator('#menu').tap(); await button(page, 'Controls').tap();
    await page.getByRole('slider', { name: 'Mobile steering sensitivity' }).waitFor();
  };
  const slider = page.getByRole('slider', { name: 'Mobile steering sensitivity' });
  await open(); assert.equal(await slider.inputValue(), '85');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-mobile-steering-sensitivity')), null);
  const set = async value => {
    await slider.fill(String(value));
    await page.waitForFunction(value => localStorage.getItem('atv-mobile-steering-sensitivity') === String(value), value);
  };
  for (const viewport of [{ width: 390, height: 844 }, { width: 320, height: 568 }, { width: 844, height: 390 }]) {
    await page.setViewportSize(viewport);
    await waitForAnimations(page); await nextFrames(page);
    const box = await slider.boundingBox();
    assert(box.x >= 0 && box.y >= 0 && box.x + box.width <= viewport.width && box.y + box.height <= viewport.height);
    assert(box.height >= 34 && (viewport.width > viewport.height || box.height >= 44));
    const keyboard = await button(page, 'Keyboard bindings').boundingBox();
    assert(box.y + box.height <= keyboard.y);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  }
  await page.setViewportSize({ width: 390, height: 844 }); await waitForAnimations(page); await nextFrames(page);
  await slider.focus(); await page.keyboard.press('ArrowRight');
  await page.waitForFunction(() => localStorage.getItem('atv-mobile-steering-sensitivity') === '90');
  assert.equal(await slider.evaluate(el => el === document.activeElement), true, 'redraw preserves keyboard focus');
  const box = await slider.boundingBox();
  await page.mouse.move(box.x + box.width * .4, box.y + box.height / 2);
  await page.mouse.down();
  for (const fraction of [.55, .7, .85]) { await page.mouse.move(box.x + box.width * fraction, box.y + box.height / 2); await nextFrames(page, 3); }
  await page.mouse.up();
  assert(Number(await slider.inputValue()) >= 130, 'drag continues through Rust label redraws');
  await set(50);
  if (process.env.ATV_STEERING_SCREENSHOT) await page.screenshot({ path: process.env.ATV_STEERING_SCREENSHOT });
  await button(page, 'Back').tap();
  await button(page, 'Resume').tap();
  await page.reload(); await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  await open(); assert.equal(await slider.inputValue(), '50', 'setting persists after reload');
  await button(page, 'Back').tap();
  await button(page, 'Resume').tap();
  if (engine === 'chromium') {
    const cdp = await page.context().newCDPSession(page);
    const touch = (type, points = []) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points.map(p => ({ radiusX: 6, radiusY: 6, force: 1, ...p })) });
    const origin = { id: 1, x: 110, y: 560 };
    await touch('touchStart', [origin]); await touch('touchMove', [{ ...origin, x: 145, y: 525 }]);
    await page.waitForFunction(() => Math.abs(window.atvTerrainStatus.input?.steer + .25) < 1e-6 && window.atvTerrainStatus.input.lean === -.5);
    assert.equal(await page.locator('.touch-rider-marker').evaluate(el => parseFloat(el.style.getPropertyValue('--steer'))), 8.5, 'feedback matches softened steering');
    await touch('touchMove', [{ ...origin, x: 250, y: 525 }]);
    await page.waitForFunction(() => window.atvTerrainStatus.input?.steer === -1, null, { timeout: 3000 });
    await touch('touchCancel');
    await page.waitForFunction(() => window.atvTerrainStatus.input?.steer === 0 && !window.atvTerrainStatus.input?.touch_drive?.held);
    await open(); await set(150); await button(page, 'Back').tap(); await button(page, 'Resume').tap();
    await touch('touchStart', [origin]); await touch('touchMove', [{ ...origin, x: 145 }]);
    await page.waitForFunction(() => Math.abs(window.atvTerrainStatus.input?.steer + .75) < 1e-6);
    await touch('touchCancel');
    // The optional left-thumb steering reads the same saved setting.
    await page.evaluate(() => localStorage.setItem('atv-mobile-one-handed', 'false'));
    await page.reload(); await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
    await touch('touchStart', [{ ...origin, x: 60 }]); await touch('touchMove', [{ ...origin, x: 95, y: 525 }]);
    await page.waitForFunction(() => Math.abs(window.atvTerrainStatus.input?.steer + .75) < 1e-6 && window.atvTerrainStatus.input?.lean === -.5);
    await touch('touchCancel');
  }
  assertNoErrors(errors);
  console.log(`PASS: ${engine === 'webkit' ? 'WebKit' : 'Chromium'} Controls slider, gentler default, keyboard/drag focus, three layouts and persistence${engine === 'chromium' ? ', both touch modes, unchanged lean, full lock and cancellation' : ''}.`);
} finally { await browser.close(); }
