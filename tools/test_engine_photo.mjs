// Photo mode (#542): Pause → Photo mode hides the menu and HUD and gives a
// free camera around the paused moment (mouse orbit and wheel zoom, FOV
// keys, touch drag and pinch on a phone), Save photo downloads a PNG of the
// canvas, and Back returns to Pause with the ride exactly where it stopped.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const wait = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 30000 });
const state = () => JSON.parse(document.querySelector('#ui-state').textContent);
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === name, name);
const centre = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const menuHit = async (page, action) => (await read(page)).toolbar.menu.hits.find(h => h.action === action);
const viewerHit = async (page, id) => (await read(page)).toolbar.hits.find(h => h.id === id);

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, acceptDownloads: true }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(page, 30);
  await page.mouse.click(640, 400);
  // Ride forward a moment so the paused state is a real one.
  await page.keyboard.down('KeyW'); await nextFrames(page, 40); await page.keyboard.up('KeyW');
  await key(page, 'Escape');
  await onPage(page, 'pause');
  const paused = await read(page);
  assert.ok(paused.toolbar.menu.choices.includes('photo'), `Photo mode row: ${paused.toolbar.menu.choices}`);
  const { step, origin } = paused;
  let at = centre(await menuHit(page, 'photo'));
  await page.mouse.click(at.x, at.y);
  await wait(page, () => { const s = JSON.parse(document.querySelector('#ui-state').textContent); return s.photo && !s.toolbar.menu; });
  let s = await read(page);
  assert.equal(s.paused, true);
  // Only the photo controls draw: Back, FOV -/+ and Save photo.
  assert.deepEqual(s.toolbar.hits.map(h => h.id).sort(), [50, 51, 52, 53]);
  const start = s.photo;
  // Mouse drag orbits; the drawn eye moves with it.
  await page.mouse.move(640, 360); await page.mouse.down();
  await page.mouse.move(840, 300, { steps: 6 }); await page.mouse.up(); await nextFrames(page, 3);
  s = await read(page);
  assert.notEqual(s.photo.yaw, start.yaw, 'drag orbits');
  assert.ok(Math.hypot(...s.view.eye.map((v, i) => v - s.photo.target[i])) > 1, 'eye off the focus point');
  // The wheel zooms and ] widens the lens.
  await page.mouse.wheel(0, 400); await nextFrames(page, 3);
  assert.ok((await read(page)).photo.distance > s.photo.distance, 'wheel zooms out');
  const fov = (await read(page)).photo.fov;
  await page.keyboard.down('BracketRight'); await nextFrames(page, 20); await page.keyboard.up('BracketRight'); await nextFrames(page, 2);
  s = await read(page);
  assert.ok(s.photo.fov > fov, 'FOV widens');
  assert.ok(Math.abs(s.view.focal - 1 / Math.tan(s.photo.fov * Math.PI / 360)) < 1e-3, 'lens follows the FOV');
  // Save photo downloads a PNG of the canvas.
  const hit = await viewerHit(page, 51);
  at = centre(hit);
  const [download] = await Promise.all([page.waitForEvent('download', { timeout: 15000 }), page.mouse.click(at.x, at.y)]);
  assert.match(download.suggestedFilename(), /^fury-remastered-\d{4}-\d\d-\d\d-\d{6}\.png$/);
  const bytes = await (await import('node:fs/promises')).readFile(await download.path());
  assert.deepEqual([...bytes.subarray(1, 4)], [0x50, 0x4e, 0x47], 'PNG signature');
  assert.ok(bytes.length > 20000, `a real frame (${bytes.length} bytes)`);
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).photo?.notice?.startsWith('Photo saved'));
  // Nothing moved the ride; Escape returns to Pause, then resumes.
  s = await read(page);
  assert.equal(s.step, step);
  assert.deepEqual(s.origin, origin);
  await key(page, 'Escape');
  await onPage(page, 'pause');
  assert.equal((await read(page)).photo, null);
  await key(page, 'Escape');
  await wait(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).paused);
  assertNoErrors(errors);
  await page.close();

  // Phone: touch drag orbits, a pinch zooms, Back is a tap.
  const phone = await newPage(browser, { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true }, errors);
  await phone.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await phone.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(phone, 30);
  const pause = (await read(phone)).toolbar.hits.find(h => h.id === 1);
  await phone.touchscreen.tap(pause.x + pause.width / 2, pause.y + pause.height / 2);
  await onPage(phone, 'pause');
  at = centre(await menuHit(phone, 'photo'));
  await phone.touchscreen.tap(at.x, at.y);
  await wait(phone, () => JSON.parse(document.querySelector('#ui-state').textContent).photo);
  for (const h of (await read(phone)).toolbar.hits) assert.ok(h.height >= 44 && h.x >= 0 && h.x + h.width <= 390, 'touch-size controls on screen');
  const cdp = await phone.context().newCDPSession(phone);
  const touch = (type, points = []) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points.map(p => ({ radiusX: 6, radiusY: 6, force: 1, ...p })) });
  const before = (await read(phone)).photo;
  await touch('touchStart', [{ id: 1, x: 200, y: 400 }]);
  for (let i = 1; i <= 6; i++) { await touch('touchMove', [{ id: 1, x: 200 + i * 20, y: 400 }]); await nextFrames(phone, 1); }
  await touch('touchEnd'); await nextFrames(phone, 3);
  const orbited = (await read(phone)).photo;
  assert.notEqual(orbited.yaw, before.yaw, 'touch drag orbits');
  await touch('touchStart', [{ id: 1, x: 150, y: 400 }, { id: 2, x: 250, y: 400 }]);
  for (let i = 1; i <= 6; i++) { await touch('touchMove', [{ id: 1, x: 150 - i * 15, y: 400 }, { id: 2, x: 250 + i * 15, y: 400 }]); await nextFrames(phone, 1); }
  await touch('touchEnd'); await nextFrames(phone, 3);
  assert.ok((await read(phone)).photo.distance < orbited.distance, 'pinch out zooms in');
  at = centre(await viewerHit(phone, 50));
  await phone.touchscreen.tap(at.x, at.y);
  await onPage(phone, 'pause');
  assertNoErrors(errors);
  console.log('PASS: photo mode orbit, zoom, FOV, PNG save, touch drag and pinch, back to Pause');
} finally {
  await browser.close();
}
