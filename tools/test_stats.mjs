#!/usr/bin/env node
// The public rider stats page (/stats.html) against /api/stats answers built by the
// Worker's own summarize(): full data, no analytics token, an unavailable endpoint,
// no JavaScript and the ?stats=off device opt-out. No game runtime or original game
// data is loaded.
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
      maps: { env02: [Math.round(rides / 2), rides * 200, 3], sx12: [6, rides * 90, 2], free05: [4, rides * 60, 0], env01: [3, rides * 30, 0], env03: [2, rides * 20, 0], env04: [2, rides * 15, 0], nat01: [1, rides * 10, 0], sx01: [1, rides * 5, 0], ...(i === 0 ? { nat03: [1, 300, 0] } : {}) },
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

async function open(answer, viewport = { width: 1280, height: 1000 }, path = '/stats.html') {
  const page = await browser.newPage({ viewport });
  watchErrors(page, errors, { console: true });
  await page.route('**/api/stats', route => (answer ? route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(answer) })
    : route.fulfill({ status: 503, body: 'unavailable' })));
  await page.goto(url(path), { waitUntil: 'networkidle' });
  await page.waitForFunction(() => document.querySelector('#stats')?.getAttribute('aria-busy') === 'false');
  return page;
}

const noSideways = page => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth);

try {
  // Everything present: big numbers first, detail on demand.
  const page = await open(full);
  assert.equal(await page.locator('#live-riders').textContent(), '6');
  assert.equal(await page.locator('#live-note').textContent(), '2 rooms · record 17');
  assert.equal(await page.locator('#daily-note').textContent(), 'Motorcycles · 7 riders');
  for (const id of ['hero', 'tile-hours', 'tile-daily', 'days-section', 'venues-section', 'splits-section', 'boards-section', 'countries-section']) {
    assert.ok(await visible(page, `#${id}`), `${id} shows with data`);
  }
  assert.equal(await page.locator('#total-rides').textContent(), full.totals.allTime.rides.toLocaleString('en'));
  await page.getByRole('radio', { name: 'Today' }).click();
  assert.equal(await page.locator('#total-rides').textContent(), String(full.totals.today.rides));
  await page.keyboard.press('ArrowRight');
  assert.equal(await page.getByRole('radio', { name: 'This week' }).getAttribute('aria-checked'), 'true');
  assert.equal(await page.locator('#total-rides').textContent(), full.totals.week.rides.toLocaleString('en'));

  assert.equal(await page.locator('#days-chart .day').count(), 30);
  assert.equal(await page.locator('#days-table tbody tr').count(), 30);
  // Tracks: the top five, the rest behind "Show all"; nat03 (below MIN_BUCKET) never arrives.
  const byHours = full.venues.map(venue => venue.name);
  assert.equal(byHours.length, 8);
  assert.ok(!full.venues.some(venue => venue.id === 'nat03'));
  assert.deepEqual(await page.locator('#venues li:visible .bar-name').allTextContents(), byHours.slice(0, 5));
  const more = page.locator('#venues-more');
  assert.equal(await more.textContent(), 'Show all 8');
  assert.equal(await more.getAttribute('aria-expanded'), 'false');
  await more.focus();
  await page.keyboard.press('Enter');
  assert.equal(await more.getAttribute('aria-expanded'), 'true');
  assert.equal(await page.locator('#venues li:visible').count(), 8);
  await page.getByRole('radio', { name: 'Rides' }).click();
  assert.match(await page.locator('#venues li').first().locator('.bar-value').textContent(), /rides$/);
  assert.equal(await page.locator('#venues li:visible').count(), 8, 'the ranking keeps the list open');
  await more.click();
  assert.equal(await page.locator('#venues li:visible').count(), 5);

  // Splits and countries are closed rows with a summary; Enter opens them.
  assert.equal(await visible(page, '#splits'), false);
  assert.match(await page.locator('#splits-teaser').textContent(), /% Race · \d+% ATV$/);
  await page.locator('#splits-section summary').focus();
  await page.keyboard.press('Enter');
  assert.ok(await visible(page, '#splits'));
  assert.equal(await page.locator('#splits .split').count(), 4);
  assert.match(await page.locator('#splits').textContent(), /Single player.*Multiplayer.*Free Ride/s);
  // Countries: four rode, only those with 10+ visits are named.
  assert.equal(await page.locator('#country-count').textContent(), '4');
  await page.locator('#countries-section summary').click();
  assert.deepEqual(await page.locator('#countries .name').allTextContents(), ['United States', 'United Kingdom']);
  assert.equal(await page.locator('#countries .flags-rest').textContent(), '+2 more');
  assert.deepEqual(await page.locator('#boards-section b').allTextContents(), ['412', '96', '230']);
  assert.equal(await visible(page, '#opted-out'), false);
  assert.equal(await noSideways(page), true);
  await page.close();

  // Phones: no sideways scroll; the live count sits above hours and rides, side by side.
  for (const width of [390, 320]) {
    const phone = await open(full, { width, height: 800 });
    await phone.evaluate(() => document.querySelectorAll('details').forEach(details => { details.open = true; }));
    await phone.locator('#venues-more').click();
    assert.equal(await noSideways(phone), true, `no sideways scroll at ${width} px`);
    const [live, hours, rides] = await Promise.all(['#tile-riders', '#tile-hours', '#tile-rides'].map(id => phone.locator(id).boundingBox()));
    assert.ok(live.y + live.height <= hours.y && hours.y === rides.y, `hero layout at ${width} px`);
    await phone.close();
  }

  // The record shows only once two riders were online together.
  const quiet = await open({ ...full, live: { riders: 0, rooms: 0 }, peak: { ...full.peak, riders: 1 } });
  assert.equal(await quiet.locator('#live-note').textContent(), '');
  await quiet.close();

  // Without the analytics token: live, daily event and boards only.
  const lean = await open(noToken);
  assert.equal(await lean.locator('#live-riders').textContent(), '0');
  assert.equal(await lean.locator('#live-note').textContent(), 'record 17');
  for (const id of ['tile-riders', 'tile-daily', 'boards-section']) assert.ok(await visible(lean, `#${id}`), `${id} shows without usage data`);
  for (const id of ['period', 'tile-hours', 'hero-more', 'days-section', 'venues-section', 'splits-section', 'countries-section']) assert.equal(await visible(lean, `#${id}`), false, `${id} hides without usage data`);
  await lean.close();

  // Endpoint unavailable: a friendly note, no empty sections.
  const down = await open(null);
  assert.match(await down.locator('#stats-status').textContent(), /pit stop/);
  assert.equal(await down.locator('main section:not([hidden])').count(), 0);
  await down.close();

  // Without JavaScript: a short note and nothing half-drawn.
  const context = await browser.newContext({ javaScriptEnabled: false });
  const plain = await context.newPage();
  await plain.goto(url('/stats.html'));
  // (Locators skip <noscript>; the rendered text shows it.)
  assert.match(await plain.evaluate(() => document.querySelector('main').innerText.trim()), /^Turn on JavaScript to see the numbers\.$/);
  assert.equal(await plain.locator('main section:not([hidden]), #stats-status:visible').count(), 0);
  await context.close();

  // ?stats=off opts this device out of analytics until ?stats=on.
  const off = await open(full, undefined, '/stats.html?stats=off');
  assert.equal(await off.evaluate(() => localStorage.getItem('fury.stats.off')), '1');
  await off.locator('.about summary').click();
  assert.ok(await visible(off, '#opted-out'));
  await off.goto(url('/stats.html?stats=on'), { waitUntil: 'networkidle' });
  assert.equal(await off.evaluate(() => localStorage.getItem('fury.stats.off')), null);
  assert.equal(await visible(off, '#opted-out'), false);
  await off.close();

  // The dev log links here.
  const log = await browser.newPage();
  await log.route('**/devlog.json', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ entries: [] }) }));
  await log.goto(url('/devlog.html'));
  assert.equal(await log.locator('.topbar a[href="/stats.html"]').count(), 1);
  await log.close();

  // The 503 above is the only expected browser error.
  assertNoErrors(errors.filter(error => !/503|Failed to load resource/.test(String(error))));
  console.log('stats page: full, phone, no-token, unavailable, no-JS and opt-out states pass');
} finally {
  await browser.close();
}
