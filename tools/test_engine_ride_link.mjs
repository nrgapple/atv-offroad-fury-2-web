// Pause → Copy ride link in the Rust engine (frontend.js rideUrl +
// launch.js copyRideUrl): the link opens this page with the ride's setup,
// is copied to the clipboard and confirmed on the notice line.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url, base } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: new URL(base).origin });
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 3, riderId: 1, playerName: 'Ada L' }));
  });
  await page.goto(url('/engine.html?venue=env03&quad=3&rider=1&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(page, 20);
  await page.mouse.click(640, 400);
  await key(page, 'Escape');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  const m = (await read(page)).toolbar.menu, hit = m.hits.find(h => h.action === 'copyride');
  assert.ok(hit, `copyride on pause: ${m.choices}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.notice === 'Ride link copied.', null, { timeout: 10000 });
  // RIDE_LINK_SHOT=out.png: the notice line for inspection.
  if (process.env.RIDE_LINK_SHOT) { await nextFrames(page, 3); await page.screenshot({ path: process.env.RIDE_LINK_SHOT }); }
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  const link = new URL(copied);
  assert.equal(link.pathname, '/engine.html');
  assert.deepEqual(Object.fromEntries(link.searchParams), { mode: 'single', activity: 'freestyle', map: 'env03', quad: '3', rider: '1', player: 'Ada L', play: '1' });
  assertNoErrors(errors);
  console.log(`PASS: Rust Copy ride link: ${copied}`);
} finally {
  await browser.close();
}
