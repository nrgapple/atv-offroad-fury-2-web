// Menu crispness and motion (research/RUST_UI.md "Crisp menus, turntable and
// wordmark sway"): the frontend draws at the display's full density (a DPR 3
// phone's canvas is 3x its CSS size, the 3D preview too) with sharp text
// edges; the menu ATV turns on its own like a showroom turntable, a drag
// flings it and it eases back; the Fury Remastered wordmark sways in 3D; with
// prefers-reduced-motion both stay still and the idle menu draws nothing.
// Chromium and WebKit at 390x844 DPR 3. ATV_MOTION_SHOTS=<dir> saves screenshots.
// Run: node tools/run_browser_tests.mjs --only test_engine_menu_motion.mjs
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const MODEL_QUAD = 2409;
const TURNTABLE = 0.45;
const shots = process.env.ATV_MOTION_SHOTS || '';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await state(page)).front || {};
const onPage = (page, name, models = false) =>
  page.waitForFunction(
    ([name, models]) => {
      const f = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
      return f?.page === name && !f.motion && !f.exit && (!models || (f.modelTargets && f.modelTargets[0]));
    },
    [name, models],
    { timeout: 120000 }
  );
// Time-based on purpose: the turntable and the sway run on the wall clock.
const wait = (page, ms) => page.waitForTimeout(ms);
/** Turn between two yaw readings, unwrapped. */
const turned = (a, b) => {
  let d = b - a;
  while (d > Math.PI) d -= 2 * Math.PI;
  while (d < -Math.PI) d += 2 * Math.PI;
  return d;
};

/**
 * Sharpness of the screenshot inside `rect` (CSS px): the largest luminance
 * step between neighbouring device pixels along rows. Text drawn at the
 * display's density steps almost from background to ink in one pixel; a 1.5x
 * surface upscaled to DPR 3 spreads every edge over two or more pixels.
 */
async function edgeStep(page, rect) {
  const png = await page.screenshot({ clip: rect, scale: 'device' });
  return page.evaluate(async data => {
    // A Blob, not a data: URL (the page's CSP limits image sources).
    const bytes = Uint8Array.from(atob(data), c => c.charCodeAt(0));
    const image = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    const canvas = new OffscreenCanvas(image.width, image.height);
    const g = canvas.getContext('2d');
    g.drawImage(image, 0, 0);
    const { data: px, width, height } = g.getImageData(0, 0, image.width, image.height);
    const lum = i => 0.2126 * px[i] + 0.7152 * px[i + 1] + 0.0722 * px[i + 2];
    const steps = [];
    for (let y = 0; y < height; y++)
      for (let x = 1; x < width; x++) steps.push(Math.abs(lum((y * width + x) * 4) - lum((y * width + x - 1) * 4)));
    steps.sort((a, b) => b - a);
    // The 20th largest step, so one stray pixel does not decide.
    return steps[Math.min(19, steps.length - 1)];
  }, png.toString('base64'));
}

if (shots) await mkdir(shots, { recursive: true });
const errors = [];
for (const engine of ['chromium', 'webkit']) {
  const browser = await launch({ engine });
  try {
    const viewport = { width: 390, height: 844 };
    const page = await newPage(
      browser,
      { viewport, deviceScaleFactor: 3, hasTouch: true, isMobile: engine === 'webkit' },
      errors
    );
    await unlockStore(page);
    await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));

    // Crisp: the UI and the canvas backing are at DPR 3.
    await page.goto(url('/play.html?uiDiagnostics=1&front=main'));
    await onPage(page, 'main', true);
    let f = await front(page);
    assert.deepEqual(f.surface, [1170, 2532, 3], `${engine}: menu surface at DPR 3 ${JSON.stringify(f.surface)}`);
    const canvas = await page.evaluate(() => {
      const c = document.querySelector('#game');
      return [c.width, c.height];
    });
    assert.deepEqual(canvas, [1170, 2532], `${engine}: canvas backing`);
    const slot = f.hits.find(h => h.id === MODEL_QUAD);
    const [tw, th] = f.modelTargets[0];
    assert.ok(Math.abs(tw - slot.width * 3) <= 1 && Math.abs(th - slot.height * 3) <= 1, `${engine}: preview at 3x`);
    // Text edges: the heading's letters step from the backdrop to ink in a pixel.
    const row = f.hits.find(h => h.id === 2000);
    assert.ok(row, 'first menu row');
    const step = await edgeStep(page, { x: row.x, y: row.y, width: Math.min(row.width, 200), height: row.height });
    assert.ok(step > 150, `${engine}: menu text edge step ${step.toFixed(0)} (soft when upscaled)`);
    if (shots) await page.screenshot({ path: `${shots}/${engine}-main.png` });

    // Turntable: the ATV turns on its own at about TURNTABLE rad/s.
    const y0 = (await front(page)).modelYaw[0];
    await wait(page, 1500);
    f = await front(page);
    const rate = turned(y0, f.modelYaw[0]) / 1.5;
    assert.ok(rate > TURNTABLE * 0.5 && rate < TURNTABLE * 1.6, `${engine}: turntable rate ${rate.toFixed(2)} rad/s`);
    // The wordmark sways: its angles change, a few degrees at most.
    const t0 = f.tilt;
    await wait(page, 400);
    const t1 = (await front(page)).tilt;
    assert.ok(t0[0] !== t1[0] || t0[1] !== t1[1], `${engine}: wordmark sways ${t0} ${t1}`);
    assert.ok(Math.abs(t1[0]) <= (8 * Math.PI) / 180 && Math.abs(t1[1]) <= (4 * Math.PI) / 180, 'a gentle sway');

    // Drag to spin: a quick swipe flings it, then it eases back to the turntable.
    const cx = slot.x + slot.width / 2;
    const cy = slot.y + slot.height / 2;
    await page.mouse.move(cx - 80, cy);
    await page.mouse.down();
    for (let i = 1; i <= 8; i++) {
      await page.mouse.move(cx - 80 + i * 20, cy);
      await nextFrames(page, 1);
    }
    await page.mouse.up();
    await nextFrames(page, 2);
    f = await front(page);
    assert.equal(f.page, 'main', 'dragging the preview did not open a row');
    assert.ok(Math.abs(f.modelSpin[0]) > TURNTABLE * 2, `${engine}: fling ${f.modelSpin[0]}`);
    await wait(page, 4000);
    f = await front(page);
    assert.ok(Math.abs(f.modelSpin[0] - TURNTABLE) < 0.1, `${engine}: eased back to the turntable ${f.modelSpin[0]}`);

    // The title's wordmark sways too.
    await page.goto(url('/play.html?uiDiagnostics=1'));
    await onPage(page, 'title');
    await wait(page, 1200);
    const a = (await front(page)).tilt;
    await wait(page, 400);
    const b = (await front(page)).tilt;
    assert.ok(a[0] !== b[0], `${engine}: title wordmark sways`);
    if (shots) await page.screenshot({ path: `${shots}/${engine}-title.png` });

    // Reduced motion: still, and the idle menu draws nothing.
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto(url('/play.html?uiDiagnostics=1&front=main'));
    await onPage(page, 'main', true);
    await wait(page, 1500); // The page settles and sleeps.
    const still = await state(page);
    await wait(page, 1000); // Proving nothing moves or draws.
    const later = await state(page);
    assert.equal(later.front.modelYaw[0], still.front.modelYaw[0], `${engine}: reduced motion: no turntable`);
    assert.deepEqual(later.front.tilt, [0, 0], 'reduced motion: no sway');
    assert.ok(later.frontFrames - still.frontFrames <= 1, `${engine}: reduced motion idle drew frames`);
    // A drag still turns it, without a fling.
    await page.mouse.move(cx - 60, cy);
    await page.mouse.down();
    for (let i = 1; i <= 6; i++) {
      await page.mouse.move(cx - 60 + i * 20, cy);
      await nextFrames(page, 1);
    }
    await page.mouse.up();
    await nextFrames(page, 2);
    f = await front(page);
    assert.notEqual(f.modelYaw[0], later.front.modelYaw[0], 'reduced motion: a drag still turns it');
    assert.equal(f.modelSpin[0], 0, 'reduced motion: no fling');
    await page.close();
  } finally {
    await browser.close();
  }
}
assertNoErrors(errors);
console.log('PASS menu crispness, turntable and wordmark sway (Chromium, WebKit)');
