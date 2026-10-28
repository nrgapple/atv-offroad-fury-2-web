// The original Profile Store (research/PROFILE_STORE.md): Profile Editor →
// Unlock Items. A new profile enters the original GIMMEPTS cheat, buys the
// Patriot (250 Profile Points) on the ATVs page, is refused the 10,000-point
// Ravage Talon Black, keeps the purchase after a reload and can then choose
// the Patriot in the garage, where locked ATVs stay unselectable.
// ATV_STORE_SHOTS=<dir> saves the store pages for inspection.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const ROW = 2000;
const shots = process.env.ATV_STORE_SHOTS || '';
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front || {});
const onPage = (page, name) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout: 60000 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function settle(page) { await nextFrames(page, 30); }
async function clickHit(page, id) {
  const front = await read(page);
  const hit = front.hits.find(h => h.id === id);
  assert.ok(hit, `hit ${id} on ${front.page}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
  await nextFrames(page, 4);
}
async function clickRow(page, label) {
  const front = await read(page);
  const index = front.labels.findIndex(l => l === label || l.startsWith(label));
  assert.ok(index >= 0, `${label} in ${JSON.stringify(front.labels)}`);
  await clickHit(page, ROW + index);
}
async function shot(page, name) {
  if (!shots) return;
  await settle(page);
  await page.screenshot({ path: `${shots}/${name}.png` });
}

if (shots) await mkdir(shots, { recursive: true });
const browser = await launch();
try {
  for (const viewport of shots ? [{ width: 1280, height: 720 }, { width: 390, height: 844 }] : [{ width: 1280, height: 720 }]) {
    const suffix = `${viewport.width}x${viewport.height}`;
    const errors = [];
    const page = await newPage(browser, { viewport }, errors);
    await page.addInitScript(() => {
      if (sessionStorage.getItem('store-test-seeded')) return;
      sessionStorage.setItem('store-test-seeded', '1');
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('fury-profile', '{}');
      localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
    });
    await page.goto(url('/play.html?uiDiagnostics=1'));
    await onPage(page, 'title');
    await page.mouse.click(8, 8);
    await key(page, 'Enter'); await onPage(page, 'main');
    // The catalogs load behind the title.
    await page.waitForFunction(() => {
      const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store;
      return s && s.atvs > 20 && s.courses > 8;
    }, null, { timeout: 60000 });

    // Main Menu → Profile Editor → Unlock Items (touch/pointer).
    await clickRow(page, 'Profile Editor'); await onPage(page, 'profile');
    await clickRow(page, 'Unlock Items'); await onPage(page, 'store');
    let front = await read(page);
    assert.deepEqual(front.labels, ['ATVs', 'Rider Equipment', 'Games', 'Tracks', 'Cheats', 'Unlock Everything', 'Back']);
    // The points to spend are on the status bar's coin chip, emphasized here.
    assert.equal(front.profile, null);
    assert.equal(front.status.points, 0);
    assert.equal(front.status.store, true);
    await shot(page, `store-menu-${suffix}`);

    // Cheats: GIMMEPTS adds 1,000 Profile Points; a wrong code is refused.
    await clickRow(page, 'Cheats'); await onPage(page, 'store-cheats');
    await page.getByLabel('Enter Cheat:', { exact: true }).fill('nope');
    await clickRow(page, 'Enter');
    assert.equal((await read(page)).notice, 'Invalid cheat!');
    await page.getByLabel('Enter Cheat:', { exact: true }).fill('gimmepts');
    await clickRow(page, 'Enter');
    front = await read(page);
    assert.equal(front.notice, 'Added 1,000 Profile Points!');
    assert.equal(front.status.points, 1000);
    // The coin chip counts up to the new total.
    assert.equal(front.status.counting, true, 'coins count up');
    await page.waitForFunction(() => {
      const s = JSON.parse(document.querySelector('#ui-state').textContent).front.status;
      return !s.counting && s.shown === 1000;
    });
    await shot(page, `store-cheats-${suffix}`);
    await page.mouse.click(8, 8);
    await key(page, 'Escape'); await onPage(page, 'store');
    // Back returned to the Cheats row; up to ATVs.
    for (let i = 0; i < 6 && (await read(page)).choices[(await read(page)).selection] !== 'store-atv'; i++)
      await key(page, 'ArrowUp');

    // ATVs (keyboard): step to the Patriot and buy it.
    await key(page, 'Enter'); await onPage(page, 'store-atv');
    for (let i = 0; i < 30 && !(await read(page)).labels[0].includes('Patriot'); i++) await key(page, 'ArrowRight');
    front = await read(page);
    assert.match(front.labels[0], /^ATV \d+\/24 · Locked: Patriot$/);
    assert.equal(front.profile, 'Point Cost: 250');
    assert.equal(front.store.stageQuad, 8);
    await shot(page, `store-atv-${suffix}`);
    await key(page, 'ArrowDown');
    assert.equal(front.labels[1], 'Purchase Item');
    await key(page, 'Enter');
    front = await read(page);
    assert.equal(front.notice, 'Unlocked Patriot!');
    assert.match(front.labels[0], /· Unlocked: Patriot$/);
    assert.equal(front.store.remaining, 750);
    // Short of points: nothing is spent.
    for (let i = 0; i < 30 && !(await read(page)).labels[0].includes('Ravage Talon Black'); i++) await key(page, 'ArrowRight');
    await key(page, 'ArrowDown'); await key(page, 'Enter');
    front = await read(page);
    assert.equal(front.notice, 'Not enough Profile Points: Ravage Talon Black costs 10,000.');
    assert.equal(front.store.remaining, 750);
    // Training Course Talons are not for sale.
    for (let i = 0; i < 30 && !(await read(page)).labels[0].includes('Ravage Talon Blue'); i++) await key(page, 'ArrowLeft');
    assert.match((await read(page)).profile, /Complete the Training Course/);
    await key(page, 'Escape'); await onPage(page, 'store');

    // Rider Equipment, Tracks (Switch Event) and Games list the original items.
    await clickRow(page, 'Rider Equipment'); await onPage(page, 'store-gear');
    front = await read(page);
    assert.equal(front.labels[0], 'Helmet 1/26 · Unlocked: Thor Black');
    await key(page, 'ArrowRight');
    assert.equal((await read(page)).labels[0], 'Helmet 2/26 · Locked: Thor White');
    assert.equal((await read(page)).profile, 'Point Cost: 50');
    assert.equal((await read(page)).status.points, 750);
    await shot(page, `store-gear-${suffix}`);
    await clickRow(page, 'Switch Equipment');
    assert.match((await read(page)).labels[0], /^Jersey 1\/26/);
    await key(page, 'Escape'); await onPage(page, 'store');
    await clickRow(page, 'Tracks'); await onPage(page, 'store-tracks');
    assert.equal((await read(page)).labels[0], 'Track 1/8 · Unlocked: Prospect Springs');
    for (let i = 0; i < 8; i++) {
      if ((await read(page)).labels.some(l => l === 'Switch Event: Short Tracks')) break;
      await clickRow(page, 'Switch Event');
    }
    front = await read(page);
    assert.equal(front.labels[0], 'Track 1/3 · Locked: Rockbridge Park');
    assert.equal(front.profile, 'Point Cost: 100');
    assert.equal(front.status.points, 750);
    await shot(page, `store-tracks-${suffix}`);
    await key(page, 'Escape'); await onPage(page, 'store');
    await clickRow(page, 'Games'); await onPage(page, 'store-games');
    front = await read(page);
    assert.equal(front.labels[0], 'Game 1/4 · Locked: King of the Hills');
    await shot(page, `store-games-${suffix}`);

    // The purchase is saved with the profile and survives a reload.
    const saved = JSON.parse(await page.evaluate(() => localStorage.getItem('fury-profile')));
    assert.equal(saved.points, 750);
    assert.equal(saved.earned, 1000);
    assert.equal(saved.store.spent, 250);
    assert.deepEqual(saved.store.atvs, [8]);
    await page.reload();
    await onPage(page, 'title');
    await page.mouse.click(8, 8);
    await key(page, 'Enter'); await onPage(page, 'main');
    await key(page, 'Enter'); await onPage(page, 'single');
    await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store?.atvs > 20, null, { timeout: 60000 });
    assert.equal((await read(page)).store.remaining, 750);

    // The garage: the locked Talon Black gives way to the first owned ATV;
    // the bought Patriot can be chosen, a locked ATV cannot.
    await clickRow(page, 'Garage');
    await onPage(page, 'garage');
    front = await read(page);
    assert.equal(front.quad, 22, 'locked ATV01 replaced by the original default');
    assert.match(front.notice, /is locked/);
    // Left steps only through owned ATVs: Honda 250EX (14), Bombardier
    // DS650 (11), then the bought Patriot (8).
    const visited = [];
    for (let i = 0; i < 6 && (await read(page)).quad !== 8; i++) {
      await key(page, 'ArrowLeft');
      visited.push((await read(page)).quad);
    }
    assert.deepEqual(visited, [14, 11, 8]);
    await shot(page, `garage-locked-${suffix}`);
    assertNoErrors(errors);
    await page.close();
    console.log(`${suffix}: store purchase, cheat, persistence and garage gating`);
  }
  console.log('PASS: Rust engine Profile Store');
} finally {
  await browser.close();
}
