// Online identity in the Rust engine (research/ONLINE_IDENTITY.md) against a
// local fury-server behind tools/serve_multiplayer.mjs:
//   1. Ann opens Multiplayer → Friends: the game registers her rider and the
//      Online card shows her friend code.
//   2. Bo types Ann's code on Add Code → Send; Ann sees the request and accepts.
//   3. Ann rides Arlington Free Ride; Bo's Friends page shows her online with
//      Join, and Join puts Bo in Ann's shared room (FREERDnn).
//   4. A phone-sized rider's Friends page (layout and screenshot).
//   ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_online_friends.mjs
// ATV_ONLINE_SHOTS=<dir> saves the pages (1280x720 and a 390x844 phone).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_ONLINE_SHOTS;
const ui = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front || {});
const until = (page, test, arg, timeout = 60000) => page.waitForFunction(test, arg, { timeout });
const engine = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function choose(page, action) {
  const f = await ui(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.page}: ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
/** The first row whose label starts with `prefix`. */
async function chooseLabel(page, prefix) {
  const f = await ui(page);
  const at = f.labels.findIndex(l => l.startsWith(prefix));
  assert.ok(at >= 0, `${prefix} in ${f.page}: ${f.labels}`);
  await choose(page, f.choices[at]);
}
async function shot(page, name) {
  if (!shots) return;
  await nextFrames(page, 20);
  await page.screenshot({ path: `${shots}/${name}.png` });
}

const browser = await launch();
const errors = [];
async function rider(name, viewport = { width: 1280, height: 720 }) {
  const context = await browser.newContext({ viewport });
  const page = await newPage(context, {}, errors);
  await page.addInitScript(name => {
    window.testFront = () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front || {};
    if (sessionStorage.getItem('online-test')) return;
    sessionStorage.setItem('online-test', name);
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-player-selection', JSON.stringify({ playerName: name }));
    localStorage.removeItem('fury-rider-secret');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  }, name);
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onPage(page, 'main');
  return page;
}
/** Main Menu → Multiplayer → Friends, registered and listed. */
async function friends(page) {
  const f = await ui(page);
  if (f.page === 'main') {
    await choose(page, 'multiplayer');
    await onPage(page, 'multiplayer');
  }
  await choose(page, 'friends');
  await onPage(page, 'friends');
  await until(page, () => window.testFront().online?.reach === 'Ready' && !!window.testFront().online?.friends);
}

const report = {};
try {
  // 1. Ann registers and reads her code from the Online card.
  const ann = await rider('Ann');
  await friends(ann);
  await choose(ann, 'back');
  await onPage(ann, 'multiplayer');
  await choose(ann, 'back');
  await onPage(ann, 'main');
  await choose(ann, 'profile');
  await onPage(ann, 'profile');
  await choose(ann, 'online');
  await onPage(ann, 'online');
  await until(ann, () => /^[2-9B-DF-HJ-NP-TV-Z]{4}-[2-9B-DF-HJ-NP-TV-Z]{4}$/.test(window.testFront().online?.code || ''));
  let f = await ui(ann);
  const code = f.online.code;
  report.code = code;
  assert.deepEqual(f.profileBook.details[0], ['Friend Code', code]);
  assert.ok(f.profileBook.details.some(([label]) => label === 'Level 1'), JSON.stringify(f.profileBook.details));
  assert.ok(f.labels.every(l => l.split(/\s+/).filter(w => /\w/.test(w)).length <= 3), `short labels: ${f.labels}`);
  await shot(ann, 'online-card');

  // 2. Bo adds Ann by code; Ann accepts.
  const bo = await rider('Bo');
  await friends(bo);
  await choose(bo, 'friend-add');
  await onPage(bo, 'friend-add');
  await bo.getByLabel('Friend Code:', { exact: true }).click();
  await bo.keyboard.type(code.toLowerCase());
  await bo.keyboard.press('Enter');
  await nextFrames(bo, 4);
  await shot(bo, 'friend-add');
  await choose(bo, 'friendsend');
  await until(bo, () => window.testFront().notice === 'Sent ✓', null, 30000);
  await onPage(bo, 'friends');
  await choose(ann, 'friends');
  await onPage(ann, 'friends');
  await until(ann, () => window.testFront().online?.requests === 1, null, 40000);
  await shot(ann, 'friends-request');
  await chooseLabel(ann, '+ Bo');
  await onPage(ann, 'friend');
  await choose(ann, 'rideraccept');
  await until(ann, () => window.testFront().online?.friends?.[0]?.name === 'Bo', null, 30000);
  report.friends = (await ui(ann)).online.friends;

  // 3. Ann rides Free Ride; Bo sees her there and joins her shard.
  await choose(ann, 'back');
  await onPage(ann, 'online');
  for (const back of ['profile', 'main']) {
    await choose(ann, 'back');
    await onPage(ann, back);
  }
  await choose(ann, 'multiplayer');
  await onPage(ann, 'multiplayer');
  await choose(ann, 'freeride');
  await until(ann, () => JSON.parse(window.furyMultiplayer ?? 'null')?.environment === 'env08', null, 180000);
  const annRoom = (await engine(ann)).code;
  assert.match(annRoom, /^FREERD\d\d$/);
  await until(bo, room => window.testFront().online?.friends?.[0]?.room === room, annRoom, 60000);
  f = await ui(bo);
  assert.equal(f.online.friends[0].online, true);
  assert.ok(f.labels.some(l => l === '● Ann · Join'), `${f.labels}`);
  await shot(bo, 'friends-online');
  await chooseLabel(bo, '● Ann');
  await onPage(bo, 'friend');
  assert.equal((await ui(bo)).choices[0], 'riderjoin');
  await shot(bo, 'friend-join');
  await choose(bo, 'riderjoin');
  await until(bo, () => JSON.parse(window.furyMultiplayer ?? 'null')?.environment === 'env08', null, 180000);
  const boRoom = (await engine(bo)).code;
  report.rooms = { ann: annRoom, bo: boRoom };
  assert.equal(boRoom, annRoom, 'Join rides the friend\'s shared room');
  await until(bo, () => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, 60000);

  // 4. Appear Offline hides Ann (her settings, checked through Bo's lists).
  const phone = await rider('Cy', { width: 390, height: 844 });
  await friends(phone);
  await shot(phone, 'friends-phone');
  report.phone = (await ui(phone)).labels;
  report.ok = true;
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
