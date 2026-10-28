// Unlock Everything (port option, research/PROFILE_STORE.md): a fresh profile
// presses Profile Editor → Unlock Everything (touch), confirms it for the
// loaded profile (keyboard), and then a previously locked ATV (the garage keeps
// the Ravage Talon Black), track (Short Tracks' Rockbridge Park), championship
// (Pro Supercross on Expert, Short Tracks, Pro Freestyle) and game (King of the
// Hills) are open. The unlock survives a reload, gives the save a Pass Key and
// syncs, and a fresh browser that enters the key gets everything too.
//
// The save API is mocked through Playwright routes (as test_engine_passkey.mjs
// does); ATV_UNLOCK_LIVE=1 uses the site's real API instead
// (ATV_TEST_URL=https://furyremastered.com). Screenshots:
// ATV_UNLOCK_SHOTS=<dir> (desktop and a 390x844 phone).
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const live = process.env.ATV_UNLOCK_LIVE === '1';
const shots = process.env.ATV_UNLOCK_SHOTS || '';
const ROW = 2000;
const ui = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front || {});
const onPage = (page, name, timeout = 90000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function choose(page, action) {
  const f = await ui(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.page}: ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
async function tap(page, action) {
  const f = await ui(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.page}: ${f.choices}`);
  const hit = f.hits.find(h => h.id === ROW + at);
  assert.ok(hit, `row ${action} on ${f.page}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
  await nextFrames(page, 4);
}
async function shot(page, name) {
  if (!shots) return;
  await nextFrames(page, 30);
  await page.screenshot({ path: `${shots}/${name}.png` });
}
const label = (f, action) => f.labels[f.choices.indexOf(action)];
const catalogs = page => page.waitForFunction(() => {
  const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store;
  return s && s.atvs > 20 && s.courses > 8;
}, null, { timeout: 60000 });

// The contract's server (GET/PUT /api/save), in memory.
const saves = new Map();
const symbols = text => text.toUpperCase().replace(/[\s-]/g, '').replace(/^FURY(?=.{16}$)/, '').replace(/O/g, '0').replace(/[IL]/g, '1');
async function api(route) {
  const request = route.request();
  const { pathname } = new URL(request.url());
  const json = (status, body) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
  const pass = symbols(request.headers()['x-fury-pass-key'] || '');
  const now = new Date().toISOString();
  if (pathname === '/api/save' && request.method() === 'GET') {
    const s = saves.get(pass);
    return s ? json(200, s) : json(404, { error: 'not_found' });
  }
  if (pathname === '/api/save' && request.method() === 'PUT') {
    const body = JSON.parse(request.postData());
    const s = saves.get(pass);
    if ((s?.revision ?? 0) !== body.baseRevision) {
      return json(409, { error: 'conflict', revision: s?.revision ?? 0, updatedAt: s?.updatedAt ?? null, save: s?.save ?? null });
    }
    const next = { revision: (s?.revision ?? 0) + 1, updatedAt: now, save: body.save };
    saves.set(pass, next);
    return json(200, { revision: next.revision, updatedAt: now });
  }
  return json(404, { error: 'not_found' });
}

const browser = await launch();
async function device(name, viewport = { width: 1280, height: 720 }) {
  const context = await browser.newContext({ viewport });
  if (!live) await context.route('**/api/**', api);
  const errors = [];
  const page = await newPage(context, {}, errors);
  await page.addInitScript(name => {
    if (sessionStorage.getItem('unlock-test')) return;
    sessionStorage.setItem('unlock-test', name);
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    // The Ravage Talon Black (ATV01), locked on a new profile.
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
  }, name);
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await catalogs(page);
  return { context, page, errors };
}

// What a profile with everything reports; `open` false checks the new
// profile's locks instead.
async function checkUnlocks(page, open) {
  await choose(page, 'single'); await onPage(page, 'single');
  // The garage steps only through unlocked vehicles: the Talon Black (ATV01)
  // is among them only with everything unlocked.
  await choose(page, 'garage'); await onPage(page, 'garage');
  let f = await ui(page);
  const seen = new Set([f.quad]);
  for (let i = 0; i < 60 && !seen.has(1); i++) {
    await key(page, 'ArrowLeft');
    const quad = (await ui(page)).quad;
    if (i > 0 && quad === [...seen][0]) break;
    seen.add(quad);
  }
  assert.equal(seen.has(1), open, `garage visited ${[...seen]}`);
  if (open) await shot(page, 'garage-unlocked');
  await key(page, 'Escape'); await onPage(page, 'single');
  await choose(page, 'championship'); await onPage(page, 'championship');
  f = await ui(page);
  assert.equal(/Locked$/.test(label(f, 'champ:st')), !open, label(f, 'champ:st'));
  assert.equal(/Locked$/.test(label(f, 'free:spstunt')), !open, label(f, 'free:spstunt'));
  await choose(page, 'champdifficulty');
  f = await ui(page);
  assert.equal(/Locked$/.test(label(f, 'champ:prosx')), !open, label(f, 'champ:prosx'));
  if (open) await shot(page, 'championship-unlocked');
  await choose(page, 'champdifficulty');
  await key(page, 'Escape'); await onPage(page, 'single');
  await choose(page, 'games'); await onPage(page, 'games');
  // Step through the four games on the Game row (Right).
  const games = new Set();
  for (let i = 0; i < 4; i++) {
    f = await ui(page);
    if (i === 0) for (let j = f.selection; j > f.choices.indexOf('gametype'); j--) await key(page, 'ArrowUp');
    games.add(label(await ui(page), 'gametype'));
    await key(page, 'ArrowRight');
  }
  const mark = open ? '' : ' (Locked)';
  for (const game of [`Game: King of the Hills${mark}`, `Game: Treasure Hunt${mark}`, 'Game: Tag', 'Game: Hockey'])
    assert.ok(games.has(game), `${game} in ${[...games]}`);
  await key(page, 'Escape'); await onPage(page, 'single');
  await key(page, 'Escape'); await onPage(page, 'main');
  // Short Tracks' first course in Unlock Items.
  await choose(page, 'profile'); await onPage(page, 'profile');
  await choose(page, 'profile-store'); await onPage(page, 'store');
  await choose(page, 'store-tracks'); await onPage(page, 'store-tracks');
  for (let i = 0; i < 9 && !(await ui(page)).labels.includes('Switch Event: Short Tracks'); i++)
    await choose(page, 'storeswitch');
  assert.equal((await ui(page)).labels[0], `Track 1/3 · ${open ? 'Unlocked' : 'Locked'}: Rockbridge Park`);
  await key(page, 'Escape'); await onPage(page, 'store');
  await key(page, 'Escape'); await onPage(page, 'profile');
  await key(page, 'Escape'); await onPage(page, 'main');
}

try {
  if (shots) await mkdir(shots, { recursive: true });
  const a = await device('a');
  // A new game: nothing to sync, everything locked.
  assert.equal((await ui(a.page)).cloud.key, null);
  await checkUnlocks(a.page, false);

  // Profile Editor → Unlock Everything (pointer), then This Profile (keyboard).
  await tap(a.page, 'profile'); await onPage(a.page, 'profile');
  let f = await ui(a.page);
  assert.equal(label(f, 'profile-unlock'), 'Unlock Everything');
  assert.equal(f.choices.indexOf('profile-unlock'), f.choices.indexOf('profile-store') + 1);
  await shot(a.page, 'profile-editor');
  await tap(a.page, 'profile-unlock'); await onPage(a.page, 'profile-unlock');
  f = await ui(a.page);
  assert.deepEqual(f.choices, ['unlockthis', 'unlockcancel', 'back']);
  assert.equal(f.labels[0], 'Unlock For PLAYER1');
  assert.match(f.profileBook.details[0][0], /^Unlocks every ATV, rider gear, track, championship and game for this profile\. Your records and trophies stay as earned\.$/);
  assert.deepEqual(f.profileBook.details[1], ['PLAYER1', 'Not yet']);
  await shot(a.page, 'unlock-confirm');
  await choose(a.page, 'unlockthis'); await onPage(a.page, 'profile');
  f = await ui(a.page);
  assert.equal(f.notice, 'Unlocked Everything!');
  await shot(a.page, 'unlocked-notice');
  const saved = JSON.parse(await a.page.evaluate(() => localStorage.getItem('fury-profile')));
  assert.ok(saved.store.cheats.includes('EVERYTHING'), JSON.stringify(saved.store));
  // Nothing earned is faked.
  assert.equal(saved.points ?? 0, 0);
  assert.deepEqual(saved.championships.progress ?? {}, {});
  await key(a.page, 'Escape'); await onPage(a.page, 'main');
  await checkUnlocks(a.page, true);

  // The unlock is progress: the save gets a Pass Key and syncs.
  await a.page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.cloud?.status === 'Saved online', null, { timeout: 60000 });
  const passKey = (await ui(a.page)).cloud.key;
  assert.match(passKey, /^FURY-/);
  if (!live) assert.ok(saves.get(symbols(passKey)).save.profiles.profiles[0].store.cheats.includes('EVERYTHING'));

  // It survives a reload.
  await a.page.reload();
  await onPage(a.page, 'title');
  await a.page.mouse.click(8, 8);
  await key(a.page, 'Enter'); await onPage(a.page, 'main');
  await catalogs(a.page);
  await checkUnlocks(a.page, true);
  assertNoErrors(a.errors);

  // B: a fresh browser enters the Pass Key and gets everything.
  const b = await device('b');
  await checkUnlocks(b.page, false);
  await choose(b.page, 'profile'); await onPage(b.page, 'profile');
  await choose(b.page, 'passkey'); await onPage(b.page, 'passkey');
  assert.ok((await ui(b.page)).choices.includes('profile-unlock'));
  await choose(b.page, 'enterkey'); await onPage(b.page, 'passkey-entry');
  const input = b.page.getByLabel('Enter Pass Key:', { exact: true });
  await input.click();
  await b.page.keyboard.type(passKey);
  await b.page.keyboard.press('Enter');
  await nextFrames(b.page, 4);
  await choose(b.page, 'codeload');
  await onPage(b.page, 'passkey-confirm', 30000);
  await choose(b.page, 'replace'); await onPage(b.page, 'passkey');
  await key(b.page, 'Escape'); await onPage(b.page, 'profile');
  await key(b.page, 'Escape'); await onPage(b.page, 'main');
  await checkUnlocks(b.page, true);
  assertNoErrors(b.errors);

  if (shots) {
    // The phone layout of the row and the confirmation.
    const phone = await device('phone', { width: 390, height: 844 });
    await choose(phone.page, 'profile'); await onPage(phone.page, 'profile');
    await shot(phone.page, 'phone-profile-editor');
    await tap(phone.page, 'profile-unlock'); await onPage(phone.page, 'profile-unlock');
    await shot(phone.page, 'phone-unlock-confirm');
    await choose(phone.page, 'unlockthis'); await onPage(phone.page, 'profile');
    await shot(phone.page, 'phone-unlocked-notice');
    await choose(phone.page, 'passkey'); await onPage(phone.page, 'passkey');
    await shot(phone.page, 'phone-passkey');
    assertNoErrors(phone.errors);
  }
  console.log(`PASS: Unlock Everything opens a fresh profile's ATVs, tracks, championships and games, persists and syncs through the Pass Key${live ? ' (live API)' : ''}`);
} finally {
  await browser.close();
}
