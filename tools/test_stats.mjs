#!/usr/bin/env node
// The public rider stats page (/stats.html) against /api/stats answers built by the
// Worker's own summarize(): full data, no analytics token, and an unavailable endpoint.
// No game runtime or original game data is loaded.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, url, watchErrors } from './browser/harness.mjs';
import { summarize } from '../cloudflare/stats.mjs';

const AT = Date.parse('2026-11-14T12:00:00Z');
const day = offset => new Date(AT - offset * 86_400_000).toISOString().slice(0, 10);

// Forty days of rollups shaped like the cron writes them (cloudflare/stats.mjs).
function rows() {
  return Array.from({ length: 40 }, (_, i) => {
    const offset = 39 - i, rides = 20 + ((i * 37) % 60);
    return { day: day(offset), data: JSON.stringify({
      usage: 1, sessions: rides + 8, rides, seconds: rides * 420, finishes: Math.round(rides / 3),
      modes: { single: [rides - 8, rides * 300], multiplayer: [5, rides * 80], freeride: [3, rides * 40] },
      activities: { race: [Math.round(rides * 0.6), rides * 250], freestyle: [rides - Math.round(rides * 0.6), rides * 170] },
      vehicles: { atv: [rides - 4, rides * 380], motorcycle: [4, rides * 40] },
      devices: { touch: rides, desktop: 8 },
      maps: { env02: [Math.round(rides / 2), rides * 200, 3], sx12: [6, rides * 90, 2], free05: [4, rides * 60, 0], ...(i === 0 ? { nat03: [1, 300, 0] } : {}) },
      countries: { US: rides, GB: 3, ...(i < 3 ? { CA: 2 } : {}), ...(i === 0 ? { DE: 1 } : {}) },
      peakRiders: offset === 3 ? 17 : 4, peakRooms: 2,
      ...(offset === 0 ? { boards: { times: 412, riders: 96, boards: 58, daily: 230, today: 7 } } : {}),
    }) };
  });
}

const full = summarize(rows(), { live: { riders: 6, rooms: 2 }, at: AT });
const noToken = summarize(rows().map(row => {
  const { peakRiders, peakRooms, boards } = JSON.parse(row.data);
  return { day: row.day, data: JSON.stringify({ peakRiders, peakRooms, ...(boards ? { boards } : {}) }) };
}), { live: { riders: 0, rooms: 0 }, at: AT });
assert.equal(noToken.totals, undefined);

const browser = await launch();
const errors = [];
const visible = (page, selector) => page.locator(selector).isVisible();

async function open(answer, viewport = { width: 1280, height: 1000 }) {
  const page = await browser.newPage({ viewport });
  watchErrors(page, errors, { console: true });
  await page.route('**/api/stats', route => (answer ? route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(answer) })
    : route.fulfill({ status: 503, body: 'unavailable' })));
  await page.goto(url('/stats.html'), { waitUntil: 'networkidle' });
  await page.waitForFunction(() => document.querySelector('#stats')?.getAttribute('aria-busy') === 'false');
  return page;
}

try {
  // Everything present.
  const page = await open(full);
  assert.equal(await page.locator('#live-riders').textContent(), '6');
  assert.equal(await page.locator('#live-rooms').textContent(), '2');
  assert.equal(await page.locator('#peak-riders').textContent(), '17');
  assert.match(await page.locator('#daily-note').textContent(), /7 riders entered/);
  for (const id of ['totals-section', 'days-section', 'venues-section', 'splits-section', 'boards-section', 'countries-section']) {
    assert.ok(await visible(page, `#${id}`), `${id} shows with data`);
  }
  assert.equal(await page.locator('#total-rides').textContent(), full.totals.allTime.rides.toLocaleString('en'));
  await page.getByRole('radio', { name: 'Today' }).click();
  assert.equal(await page.locator('#total-rides').textContent(), String(full.totals.today.rides));
  assert.match(await page.locator('#totals-note').textContent(), /Today so far/);
  await page.keyboard.press('ArrowRight');
  assert.equal(await page.getByRole('radio', { name: 'This week' }).getAttribute('aria-checked'), 'true');
  assert.equal(await page.locator('#total-rides').textContent(), full.totals.week.rides.toLocaleString('en'));

  assert.equal(await page.locator('#days-chart .day').count(), 30);
  assert.equal(await page.locator('#days-table tbody tr').count(), 30);
  // Tracks: the venue below MIN_BUCKET rides (nat03) never arrives; ranking switches order.
  assert.deepEqual(await page.locator('#venues .bar-name').allTextContents(), ['Tioga Valley', 'Amherst', 'Vancouver Ridge']);
  await page.getByRole('radio', { name: 'Rides' }).click();
  assert.match(await page.locator('#venues li').first().locator('.bar-value').textContent(), /rides$/);
  assert.equal(await page.locator('#splits .split').count(), 4);
  assert.match(await page.locator('#splits').textContent(), /Single player.*Multiplayer.*Free Ride/s);
  // Countries: four rode, only those with 10+ visits are named.
  assert.equal(await page.locator('#country-count').textContent(), '4');
  assert.deepEqual(await page.locator('#countries .name').allTextContents(), ['United States', 'United Kingdom']);
  assert.match(await page.locator('#countries-note').textContent(), /2 more countries/);
  assert.equal(await page.locator('#board-times').textContent(), '412');
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  await page.close();

  // Phone width: no horizontal scrolling, tiles in two columns.
  const phone = await open(full, { width: 390, height: 844 });
  assert.equal(await phone.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, 'no sideways scroll at phone width');
  const [a, b] = await phone.locator('.tile').evaluateAll(tiles => tiles.slice(0, 2).map(tile => tile.getBoundingClientRect().top));
  assert.equal(a, b);
  await phone.close();

  // Without the analytics token: live, peak, daily event and boards only.
  const lean = await open(noToken);
  assert.equal(await lean.locator('#live-riders').textContent(), '0');
  assert.match(await lean.locator('#live-riders-note').textContent(), /be the first/);
  for (const id of ['tile-riders', 'tile-rooms', 'tile-peak', 'tile-daily', 'boards-section']) assert.ok(await visible(lean, `#${id}`), `${id} shows without usage data`);
  for (const id of ['totals-section', 'days-section', 'venues-section', 'splits-section', 'countries-section']) assert.equal(await visible(lean, `#${id}`), false, `${id} hides without usage data`);
  await lean.close();

  // Endpoint unavailable: a friendly note, no empty sections.
  const down = await open(null);
  assert.match(await down.locator('#stats-status').textContent(), /pit stop/);
  assert.equal(await down.locator('main section:not([hidden]) .tile:not([hidden]), main section.panel:not([hidden])').count(), 0);
  await down.close();

  // The dev log links here.
  const log = await browser.newPage();
  await log.route('**/devlog.json', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ entries: [] }) }));
  await log.goto(url('/devlog.html'));
  assert.equal(await log.locator('.topbar a[href="/stats.html"]').count(), 1);
  await log.close();

  // The 503 above is the only expected browser error.
  assertNoErrors(errors.filter(error => !/503|Failed to load resource/.test(String(error))));
  console.log('stats page: full, phone, no-token and unavailable states pass');
} finally {
  await browser.close();
}
