// Quick chat in a room (research/QUICK_CHAT.md): two Rust riders on a local
// fury-server. The host's lines, emote and horn reach the guest as bubbles,
// feed lines and a horn; the toolbar's CHAT wheel sends by touch; the guest
// mutes the host from the Players page and stops seeing its chat; a one-tap
// report is accepted by the server.
//   node tools/run_browser_tests.mjs --only test_engine_multiplayer_chat.mjs --url http://127.0.0.1:4187
// ATV_CHAT_SCREENSHOTS=<dir> saves the wheel, bubble and rider page.
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
  const open = async () => {
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(() => {
      Object.defineProperty(document, 'hasFocus', { value: () => true });
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    });
    return page;
  };
  const host = await open();
  await host.goto(`${base}/play.html?venue=env01&host=1&name=Chatty&uiDiagnostics=1`);
  await wait(host, () => JSON.parse(window.furyMultiplayer ?? 'null')?.code);
  const { code, peer } = await room(host);
  const guest = await open();
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
  console.log(`PASS quick chat in ${code}: line, emote, horn, touch wheel, mute and report`);
} finally {
  await browser.close();
}
