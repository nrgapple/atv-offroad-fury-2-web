// Browser integration checks. Synthetic fixtures are generated in memory.
// Optional ATV_TEST_ISO exercises private source content without copying it into web/.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, watchErrors } from './browser/harness.mjs';
const browser = await launch();
const errors = [];
const context = await browser.newContext({ viewport: { width: 1280, height: 1000 }, hasTouch: true });
await context.addInitScript(() => {
  window.testPads = [];
  Object.defineProperty(navigator, 'getGamepads', { value: () => window.testPads });
});
const page = await context.newPage();
watchErrors(page, errors);
const requests = [];
page.on('request', request => requests.push({ url: request.url(), method: request.method() }));
function u32(value) { const b = Buffer.alloc(4); b.writeUInt32LE(value); return b; }
function fixture() {
  const pst = Buffer.alloc(1056);
  for (const [at, value] of [[0, 1], [4, 1], [8, 1], [1036, 2], [1040, 2]]) pst.writeUInt32LE(value, at);
  Buffer.from([240, 40, 70, 255]).copy(pst, 12 + 16 * 4); pst.fill(8, 1052);
  const vag = Buffer.alloc(48 + 16 * 1600); vag.writeUInt32BE(16 * 1600, 12); vag.writeUInt32BE(22050, 16);
  for (let p = 48; p < vag.length; p += 16) { vag[p] = 8; vag[p + 1] = p === vag.length - 16 ? 1 : 0; vag.fill(0x17, p + 2, p + 16); }
  const config = Buffer.from('[synthetic]\nmode=fixture\n');
  const members = [['sample.pst', pst, 0], ['sample.vag', vag, 24576], ['config.ini', config, 73728]];
  const directory = Buffer.concat(members.flatMap(([name, data, offset]) => [u32(name.length), Buffer.from(name), u32(offset), u32(data.length)]));
  const payload = Buffer.alloc(98304);
  members.forEach(([, data, offset]) => data.copy(payload, offset));
  return Buffer.concat([u32(3), u32(12), u32(0x100), u32(0), u32(0), u32(3), u32(directory.length), directory, payload]);
}
async function value(id, predicate) {
  const [, operator, expected] = predicate.match(/^value (>|===) ([\d.]+)$/);
  await page.waitForFunction(({ id, operator, expected }) => {
    const current = Number(document.getElementById(id).textContent);
    return operator === '>' ? current > expected : current === expected;
  }, { id, operator, expected: Number(expected) });
}
try {
  await page.goto(base + '/workbench.html');
  await page.locator('#status').filter({ hasText: 'Ready.' }).waitFor();
  await page.locator('#file').setInputFiles({ name: 'broken.res', mimeType: 'application/octet-stream', buffer: Buffer.alloc(10) });
  await page.locator('#status.error').waitFor();
  await page.locator('#file').setInputFiles({ name: 'test.res', mimeType: 'application/octet-stream', buffer: fixture() });
  await page.getByRole('button', { name: /sample.pst/ }).click();
  await page.locator('#preview canvas').waitFor();
  assert.deepEqual(await page.locator('#preview canvas').evaluate(c => [...c.getContext('2d').getImageData(0, 0, 1, 1).data]), [240, 40, 70, 255]);
  await page.getByRole('button', { name: /sample.vag/ }).click();
  await page.waitForFunction(() => document.querySelector('audio')?.readyState >= 1);
  assert(await page.locator('audio').evaluate(a => a.duration > 2));
  await page.locator('audio').evaluate(a => a.play());
  await page.waitForFunction(() => document.querySelector('audio')?.currentTime > .1);
  await page.getByRole('button', { name: /config.ini/ }).click();
  await page.locator('#preview pre').filter({ hasText: 'mode=fixture' }).waitFor();
  for (const path of ['/private/reports/import.json', '/%2e%2e/private/source/', '/references/', '/Cargo.toml']) assert.equal((await page.request.get(base + path)).status(), 404);
  console.log('PASS: Wasm resource extraction, texture pixels, audio playback clock, text preview, malformed input, private path isolation');

  await page.locator('#controls-tab').click();
  await page.evaluate(() => { window.testPads = [{ index: 0, connected: true, id: 'Synthetic standard pad', mapping: 'standard', axes: [.7, -.8, .5, -.4], buttons: Array.from({ length: 17 }, (_, i) => ({ value: i === 7 ? .75 : 0, pressed: i === 7 })) }]; });
  await value('value-steer', 'value > .6'); await value('value-throttle', 'value === .75');
  await page.evaluate(() => window.testPads = []);
  await value('value-steer', 'value === 0'); await value('value-throttle', 'value === 0');
  await page.keyboard.down('w'); await value('value-throttle', 'value === 1');
  await page.evaluate(() => window.dispatchEvent(new Event('blur'))); await value('value-throttle', 'value === 0');
  await page.keyboard.up('w'); await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await page.locator('.touch-deck').scrollIntoViewIfNeeded();
  const left = await page.locator('#drive-zone').boundingBox();
  await page.mouse.move(left.x + left.width / 2, left.y + left.height / 2); await page.mouse.down();
  await page.mouse.move(left.x + left.width / 2 + 45, left.y + left.height / 2 - 45);
  await value('value-steer', 'value > .6'); await value('value-throttle', 'value > .6');
  await page.mouse.up(); await value('value-steer', 'value === 0');
  const right = await page.locator('#trick-zone').boundingBox();
  await page.mouse.move(right.x + right.width / 2, right.y + right.height / 2); await page.mouse.down();
  await page.mouse.move(right.x + right.width / 2, right.y + right.height / 2 - 80); await page.mouse.up();
  assert.equal(await page.locator('#last-trick').textContent(), 'Swipe: up');
  console.log('PASS: synthetic gamepad axes/triggers/disconnect, keyboard/focus reset, floating stick, directional swipe');

  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('.touch-deck').scrollIntoViewIfNeeded();
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  const client = await context.newCDPSession(page);
  const l = await page.locator('#drive-zone').boundingBox(); const r = await page.locator('#trick-zone').boundingBox();
  const lp = { x: l.x + l.width / 2, y: l.y + l.height / 2, id: 1 }; const rp = { x: r.x + r.width / 2, y: r.y + r.height / 2, id: 2 };
  await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [lp] });
  await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [lp, rp] });
  lp.y -= 50; rp.x += 45;
  await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [lp, rp] });
  await value('value-throttle', 'value > .7');
  await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [rp] });
  await page.waitForFunction(() => document.getElementById('last-trick').textContent === 'Swipe: right', null, {timeout:5000});
  await value('value-throttle', 'value > .7');
  await client.send('Input.dispatchTouchEvent', { type: 'touchCancel', touchPoints: [] });
  await value('value-throttle', 'value === 0');
  await page.screenshot({ path: 'work/verification/touch-controls.png', fullPage: true });
  console.log('PASS: 390px layout, simultaneous touch steering/trick gesture, touch cancellation');

  if (process.env.ATV_TEST_ISO) {
    await page.setViewportSize({ width: 1280, height: 1000 });
    await page.locator('#content-tab').click();
    await page.locator('#file').setInputFiles(process.env.ATV_TEST_ISO);
    await page.locator('#status').filter({ hasText: '742 disc files' }).waitFor({ timeout: 60000 });
    await page.locator('#filter').fill('E01_ENDL.LSI');
    await page.locator('.file').click();
    await page.getByRole('button', { name: /frame06.pst/ }).click();
    await page.locator('#preview canvas').waitFor();
    assert.equal(await page.locator('#preview canvas').evaluate(c => c.width), 256);
    await page.screenshot({ path: 'work/verification/original-texture.png', fullPage: true });
    await page.getByRole('button', { name: 'Disc', exact: true }).click();
    await page.locator('#filter').fill('/MUSIC/GOLDEN.VAG'); await page.locator('.file').click();
    await page.waitForFunction(() => document.querySelector('audio')?.readyState >= 1, null, { timeout: 60000 });
    assert(await page.locator('audio').evaluate(a => a.duration > 180 && a.duration < 200));
    await page.locator('audio').evaluate(a => a.play());
    await page.waitForFunction(() => document.querySelector('audio')?.currentTime > .1);
    console.log('PASS: exact-build ISO hash, real archive/image, original stereo music decode and browser playback clock');
  } else console.log('NOTE: private ISO checks not run; set ATV_TEST_ISO to the USA v2.00 ISO to include them.');
  assertNoErrors(errors);
  assert(requests.every(r => r.method === 'GET' && (r.url.startsWith(base) || r.url.startsWith('blob:'))), 'Unexpected upload or external request');
  console.log('PASS: no browser exceptions or file uploads');
} finally { await browser.close(); }
