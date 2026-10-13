// Host/public browser gate for compact UI, real pointer input and native-touch suppression.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { chromium, webkit } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const engine = process.env.ATV_UI_BROWSER === 'webkit' ? webkit : chromium;
const browser = await engine.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto(new URL('/play.html?drive=1&environment=env01', base).href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  // Mount an isolated surface with the real controls to verify native gesture gate
  // scope and teardown without affecting the live simulation or menu callbacks.
  const native = await page.evaluate(async () => {
    const { mountTouchControls } = await import('/touch.js');
    const surface = document.createElement('canvas'); document.body.append(surface);
    let enabled = true;
    const controls = mountTouchControls({ active: () => enabled, surface });
    const send = type => { const e = new Event(type, { cancelable: true, bubbles: true }); surface.dispatchEvent(e); return e.defaultPrevented; };
    const held = [send('touchstart'), send('touchmove')];
    enabled = false; const inactive = send('touchstart');
    enabled = true; controls.destroy(); const destroyed = send('touchstart'); surface.remove();
    return { held, inactive, destroyed };
  });
  assert.deepEqual(native, { held: [true, true], inactive: false, destroyed: false });
  for (const viewport of [{ width: 390, height: 844 }, { width: 320, height: 568 }, { width: 844, height: 390 }]) {
    await page.setViewportSize(viewport);
    const boxes = await page.locator('#toolbar button:not(#freestyle-minimap)').evaluateAll(buttons => buttons.map(el => {
      const box = el.getBoundingClientRect(), css = getComputedStyle(el);
      return { x: box.x, y: box.y, width: box.width, height: box.height, min: css.minHeight, name: el.getAttribute('aria-label') };
    }));
    assert.deepEqual(boxes.map(box => box.name), ['Pause', 'Camera']);
    for (const box of boxes) { assert(box.width >= 44 && box.height >= 44); assert(box.x >= 0 && box.x + box.width <= viewport.width); }
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    assert.equal(await page.locator('#camera svg').evaluate(el => getComputedStyle(el).width), '16px');
  }
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('#camera').focus();
  assert.equal(await page.locator('#camera').evaluate(el => getComputedStyle(el).outlineStyle), 'solid');
  const preset = await page.evaluate(() => window.atvTerrainStatus.followCamera?.preset);
  await page.locator('#camera').click();
  await page.waitForFunction(before => window.atvTerrainStatus.followCamera?.preset !== before, preset);
  await page.locator('#menu').tap();
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.page === 'pause');
  assert.equal(await page.locator('#toolbar').isVisible(), false);
  await page.getByRole('button', { name: 'Resume', exact: true }).tap();
  await page.waitForFunction(() => document.querySelector('#frontend').hidden);
  if (engine === chromium) {
    const cdp = await page.context().newCDPSession(page);
    const touch = (type, points = []) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points.map(p => ({ radiusX: 6, radiusY: 6, force: 1, ...p })) });
    for (let i = 0; i < 8; i++) { await touch('touchStart', [{ id: 1, x: 80, y: 580 }]); await touch('touchEnd'); }
    await touch('touchStart', [{ id: 1, x: 5, y: 830 }]);
    await page.waitForFunction(() => window.atvTerrainStatus.input?.touch_drive?.held);
    await page.waitForTimeout(700); // Exceeds the native long-press interval.
    assert.equal(await page.evaluate(() => getSelection().toString()), '');
    const styles = await page.locator('.touch-rider-marker *').evaluateAll(elements => elements.map(el => getComputedStyle(el).webkitUserSelect));
    assert(styles.every(value => value === 'none'));
    const cue = await page.locator('.touch-rider-marker span').boundingBox();
    assert(cue.x >= 0 && cue.x + cue.width <= 390 && cue.y + cue.height <= 844);
    await touch('touchCancel');
    await page.waitForFunction(() => !window.atvTerrainStatus.input?.touch_drive?.held);
    assert.equal(await page.locator('.touch-rider-marker').evaluate(el => getComputedStyle(el).opacity), '0');
    // Short two-finger input gate before this course's first ramp.
    await page.evaluate(() => localStorage.setItem('atv-mobile-one-handed', 'false'));
    await page.reload();
    await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
    const left = { id: 1, x: 60, y: 560 }, right = { id: 2, x: 280, y: 560 };
    await touch('touchStart', [left, right]);
    await page.waitForFunction(() => window.atvTerrainStatus.input?.throttle === 1 && window.atvTerrainStatus.input?.preload_axis === 1);
    await touch('touchMove', [{ ...left, x: 95, y: 525 }, right]);
    await page.waitForFunction(() => Math.abs(window.atvTerrainStatus.input?.steer + .425) < 1e-6 && window.atvTerrainStatus.input?.lean === -.5);
    assert.equal(await page.locator('.touch-rider-marker').evaluate(el => getComputedStyle(el).width), '72px');
    await touch('touchCancel');
    await page.waitForFunction(() => window.atvTerrainStatus.input?.throttle === 0 && window.atvTerrainStatus.input?.lean === 0);

  }
  assert.deepEqual(errors, []);
  console.log(`PASS: ${engine === webkit ? 'WebKit' : 'Chromium'} mobile layouts, 44px actions, focus, camera/pause/resume, native touch scope/teardown${engine === chromium ? ', rapid/long/edge touches, two-finger input and cancellation' : ''}. Physical iOS loupe remains a hardware check.`);
} finally { await browser.close(); }
