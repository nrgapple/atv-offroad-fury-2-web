// Pass Key cloud saves in the Rust engine (research/CLOUD_SAVE.md, client
// section). A browser context with saved progress gets a Pass Key, syncs it and
// shows it on the badge; a fresh context enters that key (typed into the Pass
// Key page) and continues the same progress; a share code copies progress into a
// third context's save as a new profile; Export Save / Import Save round-trips a
// file; and a `#key=` link opens the confirmation directly.
//
// The save API is the contract's (GET/PUT /api/save, POST /api/save/share, GET
// /api/share), mocked in this process through Playwright routes so the test runs
// against any static server. ATV_PASSKEY_LIVE=1 uses the site's real API instead
// (ATV_TEST_URL=https://furyremastered.com). ATV_PASSKEY_API=https://furyremastered.com
// forwards a local build's /api/ requests to that server instead (no Origin header,
// as the desktop app sends), checking the client against the real API before a deploy.
// Screenshots: ATV_PASSKEY_SHOTS=<dir> (desktop and 390x844 phone).
import assert from 'node:assert/strict';
import { mkdtemp, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const live = process.env.ATV_PASSKEY_LIVE === '1';
const forward = process.env.ATV_PASSKEY_API?.replace(/\/+$/, '');
const shots = process.env.ATV_PASSKEY_SHOTS;
const BADGE = 2413;
const ui = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front || {});
const onPage = (page, name, timeout = 90000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
const cloudState = (page, test, timeout = 30000) =>
  page.waitForFunction(test, null, { timeout }).then(() => ui(page).then(f => f.cloud));
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function choose(page, action) {
  const f = await ui(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.page}: ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
async function clickBadge(page) {
  const f = await ui(page);
  const hit = f.hits.find(h => h.id === BADGE);
  assert.ok(hit, `badge on ${f.page}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
  await onPage(page, 'passkey');
}
async function shot(page, name) {
  if (!shots) return;
  await nextFrames(page, 20);
  await page.screenshot({ path: `${shots}/${name}.png` });
}
const profiles = page => page.evaluate(() => JSON.parse(localStorage.getItem('fury-profiles') || 'null'));

// The contract's server, in memory.
const saves = new Map();
const shares = new Map();
const symbols = text => text.toUpperCase().replace(/[\s-]/g, '').replace(/^FURY(?=.{16}$)/, '').replace(/^SHARE(?=.{16}$)/, '').replace(/O/g, '0').replace(/[IL]/g, '1');
async function api(route) {
  const request = route.request();
  const { pathname } = new URL(request.url());
  const json = (status, body) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
  const pass = symbols(request.headers()['x-fury-pass-key'] || '');
  const now = new Date().toISOString();
  if (pathname === '/api/save' && request.method() === 'GET') {
    const s = saves.get(pass);
    return s ? json(200, { revision: s.revision, updatedAt: s.updatedAt, save: s.save }) : json(404, { error: 'not_found' });
  }
  if (pathname === '/api/save' && request.method() === 'PUT') {
    const body = JSON.parse(request.postData());
    assert.equal(body.save.v, 1);
    assert.ok(JSON.stringify(body.save).length <= 65536);
    const s = saves.get(pass);
    if ((s?.revision ?? 0) !== body.baseRevision) {
      return json(409, { error: 'conflict', revision: s?.revision ?? 0, updatedAt: s?.updatedAt ?? null, save: s?.save ?? null });
    }
    const next = { revision: (s?.revision ?? 0) + 1, updatedAt: now, save: body.save };
    saves.set(pass, next);
    return json(200, { revision: next.revision, updatedAt: now });
  }
  if (pathname === '/api/save/share' && request.method() === 'POST') {
    const s = saves.get(pass);
    if (!s) return json(404, { error: 'not_found' });
    const code = `SHARE-ABCD-EFGH-JKMN-${String(shares.size).padStart(4, '0')}`;
    shares.set(symbols(code), { createdAt: now, save: s.save });
    return json(201, { shareCode: code, createdAt: now });
  }
  if (pathname === '/api/share' && request.method() === 'GET') {
    const s = shares.get(symbols(request.headers()['x-fury-share-code'] || ''));
    return s ? json(200, s) : json(404, { error: 'not_found' });
  }
  return json(405, { error: 'method_not_allowed' });
}

const progress = {
  current: 0,
  profiles: [{ name: 'ACE', points: 340, earned: 400, races: 9, wins: 4, championships: {}, store: { spent: 60, atvs: [7] } }]
};
const browser = await launch();
async function device(name, { seed, viewport = { width: 1280, height: 720 }, hash = '' } = {}) {
  const context = await browser.newContext({ viewport, acceptDownloads: true });
  if (forward) {
    await context.route('**/api/**', async route => {
      const request = route.request();
      const headers = Object.fromEntries(Object.entries(request.headers()).filter(([k]) => k.startsWith('x-fury-') || k === 'content-type'));
      const { pathname, search } = new URL(request.url());
      const response = await fetch(forward + pathname + search, { method: request.method(), headers, body: request.postData() ?? undefined });
      await route.fulfill({ status: response.status, headers: { 'content-type': 'application/json', ...(response.headers.get('retry-after') ? { 'retry-after': response.headers.get('retry-after') } : {}) }, body: await response.text() });
    });
  } else if (!live) await context.route('**/api/**', api);
  const errors = [];
  const page = await newPage(context, {}, errors);
  await page.addInitScript(([seed, name]) => {
    if (sessionStorage.getItem('passkey-test')) return;
    sessionStorage.setItem('passkey-test', name);
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    if (seed) localStorage.setItem('fury-profiles', JSON.stringify(seed));
  }, [seed, name]);
  await page.goto(url(`/play.html?uiDiagnostics=1${hash}`));
  return { context, page, errors };
}

try {
  // A: saved progress → a Pass Key, synced, on the badge.
  const a = await device('a', { seed: progress });
  await onPage(a.page, 'title');
  await a.page.mouse.click(640, 200);
  const synced = await cloudState(a.page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.cloud?.status === 'Saved online', 60000);
  assert.match(synced.key, /^FURY-[0-9A-HJKMNP-TV-Z]{4}(-[0-9A-HJKMNP-TV-Z]{4}){3}$/);
  const passKey = synced.key;
  assert.equal(await a.page.evaluate(() => localStorage.getItem('fury-pass-key')), passKey);
  if (!live && !forward) assert.equal(saves.get(symbols(passKey)).save.profiles.profiles[0].points, 340);
  await shot(a.page, 'title-badge');
  await key(a.page, 'Enter'); await onPage(a.page, 'main');
  await shot(a.page, 'main-badge');
  await clickBadge(a.page);
  let f = await ui(a.page);
  assert.deepEqual(f.profileBook.details[0], ['Your Pass Key', passKey]);
  assert.ok(f.choices.includes('copykey') && f.choices.includes('sharesave'));
  await shot(a.page, 'passkey-page');
  await choose(a.page, 'showqr');
  f = await ui(a.page);
  assert.ok(f.cloud.qr.endsWith(`/play.html#key=${passKey}`), f.cloud.qr);
  await shot(a.page, 'passkey-qr');
  await choose(a.page, 'showqr');

  // B: a fresh browser enters the key and continues the save.
  const b = await device('b');
  await onPage(b.page, 'title');
  await b.page.mouse.click(640, 200);
  await key(b.page, 'Enter'); await onPage(b.page, 'main');
  assert.equal((await ui(b.page)).cloud.key, null);
  await clickBadge(b.page);
  assert.deepEqual((await ui(b.page)).choices.slice(0, 2), ['enterkey', 'entershare']);
  await choose(b.page, 'enterkey'); await onPage(b.page, 'passkey-entry');
  const input = b.page.getByLabel('Enter Pass Key:', { exact: true });
  await input.click();
  await b.page.keyboard.type(passKey.toLowerCase().replace(/0/g, 'o'));
  await b.page.keyboard.press('Enter');
  await nextFrames(b.page, 4);
  await shot(b.page, 'passkey-entry');
  await choose(b.page, 'codeload');
  await onPage(b.page, 'passkey-confirm', 30000);
  f = await ui(b.page);
  assert.deepEqual(f.choices, ['replace', 'cancel', 'back']);
  await shot(b.page, 'passkey-confirm');
  await choose(b.page, 'replace'); await onPage(b.page, 'passkey');
  assert.equal((await profiles(b.page)).profiles[0].points, 340);
  assert.equal(await b.page.evaluate(() => localStorage.getItem('fury-pass-key')), passKey);
  await cloudState(b.page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.cloud?.status === 'Saved online');

  // A shares its progress; C copies it in as a new profile.
  await choose(a.page, 'sharesave');
  await a.page.waitForFunction(() => /SHARE-/.test(JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.cloud?.shareCode || ''), null, { timeout: 30000 });
  const shareCode = (await ui(a.page)).cloud.shareCode;
  assert.match((await ui(a.page)).notice, /SHARE-/);
  const c = await device('c', { seed: { current: 0, profiles: [{ name: 'ZED', points: 5, earned: 5, races: 1 }] } });
  await onPage(c.page, 'title');
  await c.page.mouse.click(640, 200);
  await key(c.page, 'Enter'); await onPage(c.page, 'main');
  await clickBadge(c.page);
  await choose(c.page, 'entershare'); await onPage(c.page, 'passkey-entry');
  const shareInput = c.page.getByLabel('Enter Share Code:', { exact: true });
  await shareInput.click();
  await c.page.keyboard.type(shareCode);
  await c.page.keyboard.press('Enter');
  await nextFrames(c.page, 4);
  await choose(c.page, 'codeload');
  await onPage(c.page, 'passkey-confirm', 30000);
  f = await ui(c.page);
  assert.deepEqual(f.choices, ['sharecurrent', 'sharenew', 'cancel', 'back']);
  await choose(c.page, 'sharenew'); await onPage(c.page, 'passkey');
  const book = await profiles(c.page);
  assert.deepEqual(book.profiles.map(p => p.name), ['ZED', 'ACE']);
  assert.equal(book.profiles[1].points, 340);
  assert.notEqual(await c.page.evaluate(() => localStorage.getItem('fury-pass-key')), passKey);

  // Export Save from A, Import Save into D.
  await choose(a.page, 'savefiles'); await onPage(a.page, 'passkey-file');
  const [download] = await Promise.all([a.page.waitForEvent('download'), choose(a.page, 'exportsave')]);
  assert.match(download.suggestedFilename(), /^fury-save-\d{4}-\d\d-\d\d-\d{6}\.json$/);
  const file = path.join(await mkdtemp(path.join(tmpdir(), 'passkey-')), download.suggestedFilename());
  await download.saveAs(file);
  const exported = JSON.parse(await readFile(file, 'utf8'));
  assert.equal(exported.v, 1);
  assert.equal(exported.profiles.profiles[0].points, 340);
  const d = await device('d', { viewport: { width: 390, height: 844 } });
  await onPage(d.page, 'title');
  await d.page.mouse.click(195, 300);
  await key(d.page, 'Enter'); await onPage(d.page, 'main');
  // Portrait menus keep the corner for their rows: Profile Editor → Pass Key & Saves.
  assert.ok(!(await ui(d.page)).hits.some(h => h.id === BADGE), 'no badge over the phone main menu');
  await choose(d.page, 'profile'); await onPage(d.page, 'profile');
  await shot(d.page, 'phone-profile');
  await choose(d.page, 'passkey'); await onPage(d.page, 'passkey');
  assert.ok((await ui(d.page)).hits.some(h => h.id === BADGE), 'badge on the phone Pass Key page');
  await shot(d.page, 'phone-passkey-page');
  await choose(d.page, 'savefiles'); await onPage(d.page, 'passkey-file');
  const [chooser] = await Promise.all([d.page.waitForEvent('filechooser'), choose(d.page, 'importsave')]);
  await chooser.setFiles(file);
  await onPage(d.page, 'passkey-confirm', 30000);
  assert.equal((await ui(d.page)).profileBook.details[0][1], download.suggestedFilename());
  await shot(d.page, 'phone-import-confirm');
  await choose(d.page, 'replace'); await onPage(d.page, 'passkey');
  assert.equal((await profiles(d.page)).profiles[0].points, 340);

  // A `#key=` link opens the confirmation on load.
  const e = await device('e', { hash: `#key=${passKey}` });
  await onPage(e.page, 'passkey-confirm', 120000);
  assert.equal(new URL(e.page.url()).hash, '', 'the key leaves the address');
  assert.deepEqual((await ui(e.page)).profileBook.details[0], ['Pass Key', passKey]);

  for (const x of [a, b, c, d, e]) assertNoErrors(x.errors);
  console.log(`passkey: ${live || forward ? 'real API, ' : ''}key ${passKey}, share ${shareCode}, export/import and link ok`);
} finally {
  await browser.close();
}
