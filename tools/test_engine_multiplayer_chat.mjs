// Quick chat in a room (research/QUICK_CHAT.md): two Rust riders on a local
// fury-server. The host's lines, emote and horn reach the guest as bubbles,
// feed lines and a horn; the toolbar's CHAT wheel sends by touch; the guest
// mutes the host from the Players page and stops seeing its chat; a one-tap
// report is accepted by the server. Both riders are online riders (a rider
// secret, so verified sockets): the guest learns the host's verified id
// from /multiplayer/room/riders, and its Block is keyed by that id and is the
// identity block too, so the host shows on the guest's Friends lists as
// blocked (research/QUICK_CHAT.md "Verified ids", issue #666).
//   node tools/run_browser_tests.mjs --only test_engine_multiplayer_chat.mjs --url http://127.0.0.1:4187
// ATV_CHAT_SCREENSHOTS=<dir> saves the wheel, bubble and rider page (the
// guest's on a 360 px portrait phone).
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const errors = [];
const shots = process.env.ATV_CHAT_SCREENSHOTS;
const room = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const ui = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const chat = page => wait(page, () => JSON.parse(window.furyMultiplayer ?? 'null')?.chat);
const centre = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });
const shown = (page, n) =>
  wait(page, n => (JSON.parse(window.furyMultiplayer ?? 'null')?.chat?.shown ?? 0) >= n, n, 20000);

async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 2);
}
async function tap(page, id) {
  const hit = (await ui(page)).toolbar?.hits?.find(h => h.id === id);
  assert.ok(hit, `HUD target ${id}`);
  await page.mouse.click(centre(hit).x, centre(hit).y);
  await nextFrames(page, 3);
}
async function menu(page, action) {
  const m = (await ui(page)).toolbar?.menu;
  const hit = m?.hits?.find(h => h.action === action);
  assert.ok(hit, `${action} in ${JSON.stringify(m?.choices)}`);
  await page.mouse.click(centre(hit).x, centre(hit).y);
  await nextFrames(page, 4);
}

const browser = await launch();
try {
  const open = async (secret, viewport = { width: 1280, height: 720 }) => {
    const page = await newPage(browser, { viewport }, errors);
    await page.addInitScript(secret => {
      Object.defineProperty(document, 'hasFocus', { value: () => true });
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      localStorage.setItem('fury-rider-secret', secret);
    }, secret);
    return page;
  };
  const host = await open('a1'.repeat(16));
  await host.goto(`${base}/play.html?venue=env01&host=1&name=Chatty&uiDiagnostics=1`);
  await wait(host, () => JSON.parse(window.furyMultiplayer ?? 'null')?.code);
  const { code, peer } = await room(host);
  // With screenshots the guest is a 360 px portrait phone.
  const guest = await open('b2'.repeat(16), shots ? { width: 360, height: 740 } : undefined);
  await guest.goto(`${base}/play.html?room=${code}&name=Listener&uiDiagnostics=1`);
  await wait(guest, () => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, 180000);
  await wait(host, () => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, 180000);
  for (const page of [host, guest]) {
    const state = await room(page);
    assert.equal(state.serves.chat, true, 'the server relays chat');
    assert.equal(state.chat.available, true);
  }

  // Keyboard: T opens the wheel, 1 sends "Nice!".
  await host.bringToFront();
  await key(host, 'KeyT');
  assert.ok((await ui(host)).toolbar.chatWheel, 'the wheel opens');
  if (shots) await host.screenshot({ path: `${shots}/wheel.png` });
  await key(host, 'Digit1');
  assert.equal((await ui(host)).toolbar.chatWheel, null, 'a pick closes it');
  await shown(guest, 1);
  await wait(guest, () => JSON.parse(window.furyMultiplayer).chat.bubbles >= 1, null, 5000);
  if (shots) {
    await guest.bringToFront();
    await nextFrames(guest, 3);
    await guest.screenshot({ path: `${shots}/bubble.png` });
    await key(guest, 'KeyT');
    await nextFrames(guest, 3);
    await guest.screenshot({ path: `${shots}/wheel-phone.png` });
    await tap(guest, 321);
    assert.equal((await ui(guest)).toolbar.chatWheel, null, 'a tap outside closes it');
    await host.bringToFront();
  }
  // The host's own line is echoed into its feed.
  await shown(host, 1);

  // An emote (Shift+5, the heart) and the horn (B) from nearby.
  await host.bringToFront();
  await key(host, 'KeyT');
  await host.keyboard.down('ShiftLeft');
  await key(host, 'Digit5');
  await host.keyboard.up('ShiftLeft');
  await shown(guest, 2);
  await key(host, 'KeyB');
  await wait(guest, () => JSON.parse(window.furyMultiplayer).chat.horns >= 1, null, 20000);

  // Touch path: the toolbar's CHAT button (6), then the wheel's "GG"
  // (target 301).
  await tap(host, 6);
  assert.ok((await ui(host)).toolbar.chatWheel, 'CHAT opens the wheel');
  await tap(host, 301);
  await shown(guest, 4);

  // The guest mutes the host from Players → the host's rider page.
  await guest.bringToFront();
  await key(guest, 'Escape');
  await menu(guest, 'players');
  await menu(guest, `rider:${peer}`);
  if (shots) await guest.screenshot({ path: `${shots}/rider-page.png` });
  await menu(guest, `mute:${peer}`);
  assert.deepEqual((await room(guest)).chat.muted, [peer]);
  // One-tap report: the server takes it with the room's recent chat.
  await menu(guest, `report:${peer}:name`);
  await wait(guest, () => {
    const notice = JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.notice;
    return notice && notice !== 'Sending report…';
  }, null, 20000);
  const notice = (await ui(guest)).toolbar.menu.notice;
  assert.equal(notice, 'Report sent. Thanks!');
  for (let i = 0; i < 4 && (await ui(guest)).toolbar?.menu; i++) await key(guest, 'Escape');
  assert.equal((await ui(guest)).toolbar?.menu, null, 'back to riding');

  // Block: the host's verified id came from the server, the block is
  // saved under it, and it is the identity block (Friends → blocked).
  await wait(guest, peer => JSON.parse(window.furyMultiplayer).chat.ids.includes(peer), peer, 60000);
  await key(guest, 'Escape');
  await menu(guest, 'players');
  await menu(guest, `rider:${peer}`);
  await menu(guest, `block:${peer}`);
  const saved = JSON.parse(await guest.evaluate(() => localStorage.getItem('atv-chat-blocked')));
  assert.equal(saved.length, 1);
  assert.equal(saved[0].name, 'Chatty');
  assert.match(saved[0].pid ?? '', /^[0-9a-f]{12}$/, JSON.stringify(saved));
  for (let i = 0; i < 4 && (await ui(guest)).toolbar?.menu; i++) await key(guest, 'Escape');
  // The guest's Friends lists (as the server answers them) hold the block.
  const post = async (path, body) => {
    const response = await fetch(`${base}${path}`, {
      method: 'POST', headers: { 'Content-Type': 'application/json', Origin: new URL(base).origin }, body: JSON.stringify(body),
    });
    return response.json();
  };
  let blocked = [];
  for (let i = 0; i < 40 && !blocked.length; i++) {
    const { token } = await post('/multiplayer/players/register', { secret: 'b2'.repeat(16), name: 'Listener' });
    blocked = (await post('/multiplayer/friends', { token })).blocked ?? [];
    if (!blocked.length) await guest.waitForTimeout(250);
  }
  assert.deepEqual(blocked.map(r => [r.id, r.name]), [[saved[0].pid, 'Chatty']]);

  // Muted: the host's next line is hidden.
  const before = (await room(guest)).chat;
  await host.bringToFront();
  await host.waitForTimeout(5200);
  await key(host, 'KeyT');
  await key(host, 'Digit8');
  await shown(host, 5);
  await wait(guest, n => JSON.parse(window.furyMultiplayer).chat.hidden > n, before.hidden, 20000);
  assert.equal((await room(guest)).chat.shown, before.shown, 'nothing more shown');
  assertNoErrors(errors);
  console.log(`PASS quick chat in ${code}: line, emote, horn, touch wheel, mute, report and block by verified id`);
} finally {
  await browser.close();
}
