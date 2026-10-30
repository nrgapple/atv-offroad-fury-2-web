// Display → Show FPS: off by default; the Display row turns it on and saves it;
// the readout reads the frames actually presented (within 10% of a rAF count
// taken independently in the page), refreshes about twice a second, and says
// "idle" on an idle menu without making it draw. ?fps=1 shows it for a session.
// Host browser evidence (headless Chromium), not a physical-phone measurement.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null'));
const fps = async page => (await state(page))?.fps;
const wait = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 120000 });
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === name, name);
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  const m = (await state(page)).toolbar.menu, hit = m.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m.page}: ${m.choices}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 3);
}
const shots = process.env.ATV_SCREENSHOTS;
const rate = text => Number(/^(\d+) FPS$/.exec(text)?.[1]);

// An independent count: every animation frame the page gets, in its own loop.
const countFrames = page => page.evaluate(() => {
  window.__frames = [];
  const tick = t => { window.__frames.push(t); requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
});
const framesInLastSecond = page => page.evaluate(() => {
  const f = window.__frames, end = f[f.length - 1];
  return f.filter(t => t > end - 1000).length;
});

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.fps != null);
  // Off by default: nothing counted, nothing drawn.
  assert.equal((await fps(page)).on, false);
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-show-fps')), null);

  // Pause → Settings → Display → Show FPS.
  await page.mouse.click(640, 400);
  await key(page, 'Escape'); await onPage(page, 'pause');
  await click(page, 'settings'); await onPage(page, 'settings');
  await click(page, 'hud-settings'); await onPage(page, 'hud-settings');
  const m = (await state(page)).toolbar.menu;
  assert.equal(m.labels[m.choices.indexOf('show-fps')], 'Show FPS: Off');
  await click(page, 'show-fps');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-show-fps')), 'on');
  const after = (await state(page)).toolbar.menu;
  assert.equal(after.labels[after.choices.indexOf('show-fps')], 'Show FPS: On');
  await key(page, 'Escape'); await onPage(page, 'settings');
  await key(page, 'Escape'); await onPage(page, 'pause');
  await click(page, 'resume');

  // Riding: the readout appears, refreshes, and matches the page's own count.
  await page.keyboard.down('ArrowUp');
  await countFrames(page);
  await wait(page, () => /^\d+ FPS$/.test(JSON.parse(document.querySelector('#ui-state').textContent).fps.top));
  await page.waitForTimeout(1500);
  const samples = [];
  let refreshed = null, refreshes = 0;
  for (let i = 0; i < 6; i++) {
    await page.waitForTimeout(600);
    const s = await fps(page);
    if (s.refreshed !== refreshed) refreshes++;
    refreshed = s.refreshed;
    samples.push([rate(s.top), await framesInLastSecond(page), s.top, s.bottom]);
  }
  await page.keyboard.up('ArrowUp');
  const shown = samples.reduce((a, s) => a + s[0], 0) / samples.length;
  const counted = samples.reduce((a, s) => a + s[1], 0) / samples.length;
  console.log(`ride: readout ${shown.toFixed(1)} FPS, page rAF count ${counted.toFixed(1)}/s`, samples.map(s => `${s[2]} | ${s[3]}`).join('; '));
  assert.ok(refreshes >= 4, `readout refreshes about twice a second (${refreshes} of 6 samples changed)`);
  assert.ok(Math.abs(shown - counted) <= counted * 0.1, `readout ${shown} vs rAF ${counted}`);
  assert.match(samples.at(-1)[3], /^\d+\.\d ms · max \d+$/);
  const [x, y, w, h] = (await fps(page)).rect;
  assert.ok(x >= 0 && y >= 0 && x + w <= 1280 && y + h <= 720 && w < 140 && h < 50, `small and on screen: ${[x, y, w, h]}`);
  if (shots) await page.screenshot({ path: `${shots}/fps-ride.png` });

  // ?fps=1 over a fresh profile, and an idle menu: "idle", and no extra frames.
  const menu = await newPage(browser, { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true }, errors);
  await menu.emulateMedia({ reducedMotion: 'reduce' });
  await menu.goto(url('/play.html?uiDiagnostics=1&front=main&fps=1'));
  await wait(menu, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.fps?.on === true);
  await wait(menu, () => JSON.parse(document.querySelector('#ui-state').textContent).fps.top === 'idle');
  const idle = await state(menu);
  await menu.waitForTimeout(2000);
  const later = await state(menu);
  assert.equal(later.fps.top, 'idle');
  assert.ok(later.frontFrames - idle.frontFrames <= 1, `idle menu drew ${later.frontFrames - idle.frontFrames} frames with Show FPS on`);
  if (shots) await menu.screenshot({ path: `${shots}/fps-menu-idle.png` });
  // ?fps=1 is for the session: nothing saved.
  assert.equal(await menu.evaluate(() => localStorage.getItem('atv-show-fps')), null);
  // Reloading the ride keeps the saved choice.
  await page.reload();
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.fps?.on === true);
  assertNoErrors(errors);
  console.log('PASS: Show FPS setting, presented-frame readout within 10% of rAF, idle menu stays idle');
} finally {
  await browser.close();
}
