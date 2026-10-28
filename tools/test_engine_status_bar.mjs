// The menus' status bar (crates/atv-ui/src/front/status_bar.rs): profile,
// Profile Points (coins), trophies and Pass Key sync chips across the top of
// the menu pages at phone portrait/landscape, iPad and desktop sizes, never
// overlapping a row, Back or a list; each chip opens a popover with details
// and a link, closed by a tap outside or Back; the title shows the save chip
// alone and rides show no bar. The coin count animation after a purchase is in
// test_engine_profile_store.mjs. ATV_STATUS_SHOTS=<dir> saves screenshots.
// Run: node tools/run_browser_tests.mjs --only test_engine_status_bar.mjs
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const CHIPS = { profile: 2460, coins: 2461, trophies: 2462, save: 2463 };
const LINK = 2464;
const shots = process.env.ATV_STATUS_SHOTS || '';
const front = page =>
  page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front || {});
const onPage = (page, name) =>
  page.waitForFunction(
    name => {
      const f = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
      return f?.page === name && !f.motion && !f.exit;
    },
    name,
    { timeout: 120000 }
  );
const hit = (f, id) => f.hits.find(h => h.id === id);
const centre = h => ({ x: h.x + h.width / 2, y: h.y + h.height / 2 });
const overlaps = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
async function tap(page, h) {
  const c = centre(h);
  await page.mouse.click(c.x, c.y);
  await nextFrames(page, 4);
}

/** Chips present, at least 44px, inside the viewport, clear of every page target. */
function assertBar(f, label, { width, height }) {
  assert.ok(f.status, `${label}: status bar`);
  const chips = Object.values(CHIPS).map(id => hit(f, id));
  assert.ok(chips.every(Boolean), `${label}: four chips ${JSON.stringify(f.hits.filter(h => h.id >= 2460))}`);
  for (const c of chips) {
    assert.ok(c.width >= 44 && c.height >= 40, `${label}: chip target ${JSON.stringify(c)}`);
    assert.ok(c.x >= 0 && c.x + c.width <= width + 0.5 && c.y >= 0 && c.y + c.height <= height, `${label}: on screen`);
  }
  const page = f.hits.filter(h => h.id < 2460 || h.id > 2466);
  for (const c of chips)
    for (const h of page)
      assert.ok(!overlaps(c, h), `${label}: chip ${c.id} overlaps ${h.id} ${JSON.stringify([c, h])}`);
  // The page lays out below the bar: its scroll panes start under it.
  const bottom = Math.max(...chips.map(c => c.y + c.height));
  for (const id of [2450, 2400]) {
    const pane = hit(f, id);
    if (pane) assert.ok(pane.y >= bottom - 0.5, `${label}: pane ${id} below the bar`);
  }
}

if (shots) await mkdir(shots, { recursive: true });
const errors = [];
const sizes = [
  ['phone-portrait', { width: 390, height: 844 }, 3, true],
  ['phone-landscape', { width: 844, height: 390 }, 3, true],
  ['ipad', { width: 820, height: 1180 }, 2, true],
  ['desktop', { width: 1280, height: 800 }, 1, false]
];
const browser = await launch();
try {
  for (const [name, viewport, dpr, touch] of sizes) {
    const page = await newPage(browser, { viewport, deviceScaleFactor: dpr, hasTouch: touch, isMobile: touch }, errors);
    await unlockStore(page);
    await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
    for (const p of ['main', 'single', 'tracks', 'garage', 'store']) {
      await page.goto(url(`/play.html?uiDiagnostics=1&front=${p}`));
      await onPage(page, p);
      const f = await front(page);
      assertBar(f, `${name} ${p}`, viewport);
      assert.equal(f.status.profile, 'PLAYER1');
      assert.equal(f.status.store, p === 'store', 'the coin chip stands out on store pages');
      assert.deepEqual(f.status.trophies.length, 2);
      assert.equal(f.status.trophies[1], 14);
      assert.ok(['No key', 'Saved', 'Saving', 'Offline', 'Error'].includes(f.status.sync), f.status.sync);
      if (shots) await page.screenshot({ path: `${shots}/${name}-${p}.png` });
    }

    // Popovers: the coin chip shows the points; a tap outside closes it.
    await page.goto(url('/play.html?uiDiagnostics=1&front=main'));
    await onPage(page, 'main');
    let f = await front(page);
    await tap(page, hit(f, CHIPS.coins));
    f = await front(page);
    assert.equal(f.status.open, 'Profile Points');
    assert.ok(Object.fromEntries(f.status.lines).Available !== undefined, JSON.stringify(f.status.lines));
    assert.ok(hit(f, LINK), 'popover link');
    if (shots) await page.screenshot({ path: `${shots}/${name}-popover.png` });
    // A tap below the popover (on the scrim) closes it without acting.
    await page.mouse.click(viewport.width / 2, viewport.height - 8);
    await nextFrames(page, 4);
    f = await front(page);
    assert.equal(f.status.open, null, 'tap outside closes');
    assert.equal(f.page, 'main', 'the tap outside did not act on the page');
    // Trophies by series, closed with Escape.
    await tap(page, hit(f, CHIPS.trophies));
    f = await front(page);
    assert.match(f.status.open, /^Trophies/);
    assert.equal(f.status.lines.length, 8, 'won + seven series');
    await page.keyboard.press('Escape');
    await nextFrames(page, 4);
    f = await front(page);
    assert.equal(f.status.open, null, 'Escape closes');
    assert.equal(f.page, 'main', 'Escape only closed the popover');
    // Save chip → popover → Pass Key page; Back returns to the menu.
    await tap(page, hit(f, CHIPS.save));
    f = await front(page);
    assert.equal(f.status.open, 'Pass Key');
    await tap(page, hit(f, LINK));
    await onPage(page, 'passkey');
    await page.keyboard.press('Escape');
    await onPage(page, 'main');
    // The profile chip opens View Profile through its popover.
    f = await front(page);
    await tap(page, hit(f, CHIPS.profile));
    await tap(page, hit(await front(page), LINK));
    await onPage(page, 'profile-view');

    // Notices show as a toast under the bar, seen without scrolling, then fade.
    await page.goto(url('/play.html?uiDiagnostics=1&front=store-atv'));
    await onPage(page, 'store-atv');
    await page.mouse.click(4, viewport.height - 4);
    for (let i = 0; i < 30 && !(await front(page)).labels[0].includes('Locked'); i++) {
      await page.keyboard.press('ArrowRight');
      await nextFrames(page, 2);
    }
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => {
      const f = JSON.parse(document.querySelector('#ui-state').textContent).front;
      return f.notice && f.status.toast > 0.9;
    });
    if (shots) await page.screenshot({ path: `${shots}/${name}-toast.png` });
    await page.waitForFunction(
      () => JSON.parse(document.querySelector('#ui-state').textContent).front.status.toast === 0,
      null,
      {
        timeout: 10000
      }
    );

    // The title: the save chip alone.
    await page.goto(url('/play.html?uiDiagnostics=1'));
    await onPage(page, 'title');
    f = await front(page);
    assert.equal(f.status.minimal, true);
    assert.ok(hit(f, CHIPS.save) && !hit(f, CHIPS.profile) && !hit(f, CHIPS.coins), 'title: save chip only');
    await page.close();
  }

  // Rides: no bar (the HUD rules there).
  const ride = await newPage(browser, { viewport: { width: 390, height: 844 } }, errors);
  await ride.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await ride.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar, null, {
    timeout: 120000
  });
  const state = await ride.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
  assert.ok(!state.front?.status, 'no status bar in a ride');
  await ride.close();
  assertNoErrors(errors);
  console.log('PASS menu status bar: chips at four sizes clear of rows, popovers, links, title and rides');
} finally {
  await browser.close();
}
