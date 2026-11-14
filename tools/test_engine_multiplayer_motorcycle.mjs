// Motorcycles in a room (issue #521): a Rust host rides a motorcycle and an
// ATV guest joins. Each draws the other: the guest the host's bike from its
// linkage and the MX rider's bones (research/MULTIPLAYER.md "Motorcycles"),
// the host the guest's quad. The host rides off and the guest sees the bike
// move.
//   PLAYWRIGHT_MODULE=... node tools/run_browser_tests.mjs --only test_engine_multiplayer_motorcycle.mjs --url http://127.0.0.1:4187
// ATV_MP_SCREENSHOT=dir saves the guest's view of the bike there.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const errors = [];
const room = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);
const BIKE = 30;

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
  await host.goto(`${base}/play.html?venue=env01&host=1&name=BikeHost&quad=${BIKE}`);
  await wait(host, () => JSON.parse(window.furyMultiplayer ?? 'null')?.code);
  const { code, peer } = await room(host);
  const guest = await open();
  await guest.goto(`${base}/play.html?room=${code}&name=AtvGuest`);
  // The guest poses the host's motorcycle with the sender's rider bones.
  await wait(
    guest,
    ([peer, bike]) =>
      JSON.parse(window.furyMultiplayer ?? 'null')?.posed?.some(
        r => r.peer === peer && r.vehicle === bike && r.riderNodes > 0
      ),
    [peer, BIKE],
    180000
  );
  // The host draws the guest's quad.
  const guestPeer = (await room(guest)).peer;
  await wait(
    host,
    peer => JSON.parse(window.furyMultiplayer ?? 'null')?.posed?.some(r => r.peer === peer && r.vehicle <= 26),
    guestPeer,
    180000
  );
  const seen = await room(guest);
  const bike = seen.posed.find(r => r.peer === peer);
  if (shots) {
    await guest.waitForTimeout(1500);
    await guest.screenshot({ path: `${shots}/guest-sees-host-motorcycle.png` });
  }

  // The host rides off; the guest sees the bike move.
  const start = seen.remote.find(r => r.peer === peer).position;
  await host.bringToFront();
  await host.mouse.click(640, 400);
  await host.keyboard.down('KeyW');
  await host.waitForTimeout(3000);
  await host.keyboard.up('KeyW');
  await wait(
    guest,
    ([peer, start]) => {
      const r = JSON.parse(window.furyMultiplayer ?? 'null')?.remote?.find(r => r.peer === peer);
      return r && !r.stale && Math.hypot(r.position[0] - start[0], r.position[2] - start[2]) > 5;
    },
    [peer, start],
    30000
  );
  const after = await room(guest);
  const moved = flat(after.remote.find(r => r.peer === peer).position, start);
  assert.equal(after.closed, null, 'the guest stays in the room');
  assert.equal((await room(host)).closed, null, 'the host stays in the room');
  if (shots) {
    await guest.bringToFront();
    await guest.screenshot({ path: `${shots}/guest-sees-motorcycle-ride-off.png` });
  }
  assertNoErrors(errors);
  console.log(
    `PASS motorcycle in a room: the guest posed the host's bike ${BIKE} (${bike.riderNodes} rider bones), ` +
      `saw it ride ${moved.toFixed(1)} ft; the host drew the guest's ATV ` +
      `(guest received ${after.network.receivedBytes} bytes, ${after.framesReceived} frames)`
  );
} finally {
  await browser.close();
}
