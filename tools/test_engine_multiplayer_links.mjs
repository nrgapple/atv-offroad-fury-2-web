// Host-only gate: direct links between riders (research/MULTIPLAYER.md
// "Direct links"). Two Rust pages in one room open a WebRTC data channel
// through the relay's introductions; their poses then travel on the link and
// the relay only gets a presence pose every few seconds.
//   1. Host and guest link up; the driving host's poses reach the guest
//      directly while the relay sees almost none.
//   2. A third rider with `?p2p=0` (no links, as desktop builds) joins: the
//      host becomes the room's courier and bundles the driving guest's poses
//      with its own, the guest stays off the relay, and the third rider draws
//      both of them.
//   3. It leaves: the relay falls back to presence poses.
//   4. The guest's room socket drops (`?dropAfter`): it reclaims its slot and
//      links up again.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_links.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, LINK_ARGS, newPage } from './browser/harness.mjs';

const report = {};
const errors = [];
const code = `LK${String(Date.now()).slice(-6)}`;
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const browser = await launch({ args: LINK_ARGS });
const open = async () => {
  const p = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await p.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  return p;
};
const pages = {};
const links = async () =>
  JSON.stringify(
    await Promise.all(
      Object.entries(pages).map(async ([name, p]) => {
        const state = await engine(p).catch(() => null);
        return [name, state?.peer, state?.reconnects, state?.links, state?.closed];
      }),
    ),
  );
const linked = (p, peer, state = 'direct', timeout = 60000) =>
  p
    .waitForFunction(
      ([peer, state]) =>
        JSON.parse(window.furyMultiplayer ?? 'null')?.links?.some(l => l.peer === peer && l.state === state),
      [peer, state],
      { timeout },
    )
    .catch(async () => assert.fail(`rider ${peer} never became "${state}": ${await links()}`));
// The host drives for `ms`; what each page's counters gained meanwhile.
async function drive(host, pages, ms) {
  const before = await Promise.all(pages.map(engine));
  await host.bringToFront();
  await host.keyboard.down('KeyW');
  await host.waitForTimeout(ms);
  await host.keyboard.up('KeyW');
  const after = await Promise.all(pages.map(engine));
  return after.map((a, i) =>
    Object.fromEntries(
      ['messages', 'directSent', 'directReceived', 'receivedBytes'].map(k => [k, a.network[k] - before[i].network[k]]),
    ),
  );
}
const fresh = (state, peer) => state.remote.some(r => r.peer === peer && !r.stale);

try {
  const host = (pages.host = await open());
  await host.goto(`${base}/play.html?host=1&venue=env01&room=${code}&name=LinkHost&weather=off`);
  await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  const guest = (pages.guest = await open());
  await guest.goto(`${base}/play.html?room=${code}&dropAfter=9999&name=LinkGuest&weather=off`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, {
    timeout: 180000,
  });
  const hostPeer = (await engine(host)).peer;
  const guestPeer = (await engine(guest)).peer;

  // 1. The link opens on both ends and carries the poses.
  await linked(guest, hostPeer);
  await linked(host, guestPeer);
  await host.waitForTimeout(1500);
  const [h1, g1] = await drive(host, [host, guest], 4000);
  report.linked = { host: h1, guest: g1 };
  assert.ok(h1.directSent >= 25, `the host's poses go over the link: ${JSON.stringify(report)}`);
  assert.ok(g1.directReceived >= 25, `the guest hears the host directly: ${JSON.stringify(report)}`);
  assert.ok(h1.messages <= 3, `the relay only gets presence poses: ${JSON.stringify(report)}`);
  let g = await engine(guest);
  assert.ok(fresh(g, hostPeer) && g.drawn === 1, JSON.stringify(g));
  assert.equal(g.closed, null);

  // 2. A rider without links gets everyone's poses through the relay.
  const third = await open();
  await third.goto(`${base}/play.html?room=${code}&dropAfter=9999&p2p=0&name=RelayOnly&weather=off`);
  await third.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 2, null, {
    timeout: 180000,
  });
  const thirdPeer = (await engine(third)).peer;
  await linked(host, thirdPeer, 'relay');
  await linked(guest, thirdPeer, 'relay');
  await host.waitForFunction(
    peer => JSON.parse(window.furyMultiplayer ?? 'null')?.courier === peer,
    hostPeer,
    { timeout: 30000 },
  );
  await guest.waitForFunction(
    peer => JSON.parse(window.furyMultiplayer ?? 'null')?.courier === peer,
    hostPeer,
    { timeout: 30000 },
  );
  const [h2, g2, t2] = await drive(guest, [host, guest, third], 4000);
  report.mixed = { host: h2, guest: g2, third: t2 };
  assert.ok(g2.messages <= 3, `the driving guest stays off the relay: ${JSON.stringify(report)}`);
  assert.ok(g2.directSent >= 25, `the guest gives the courier its poses: ${JSON.stringify(report)}`);
  assert.ok(h2.messages >= 15, `the courier bundles them for the third rider: ${JSON.stringify(report)}`);
  let t = await engine(third);
  assert.ok(fresh(t, hostPeer) && fresh(t, guestPeer), JSON.stringify(t));
  // It hears who rides the course but links with nobody.
  assert.ok(t.links.length === 2 && t.links.every(l => l.state === 'relay'), JSON.stringify(t.links));
  assert.deepEqual([t.network.directSent, t.network.directReceived], [0, 0]);
  g = await engine(guest);
  let h = await engine(host);
  assert.ok(fresh(g, thirdPeer) && fresh(h, thirdPeer) && fresh(h, guestPeer), JSON.stringify({ g, h }));

  // 3. Once it leaves, the relay is back to presence poses.
  await third.close();
  await host.waitForFunction(
    peer => !JSON.parse(window.furyMultiplayer ?? 'null')?.links?.some(l => l.peer === peer),
    thirdPeer,
    { timeout: 60000 },
  );
  await host.waitForTimeout(1000);
  const [h3] = await drive(host, [host], 4000);
  report.afterLeave = h3;
  // By now the host has driven far from the guest, so its link cadence may
  // have dropped to the 5 Hz distance tier.
  assert.ok(h3.messages <= 3 && h3.directSent >= 15, JSON.stringify(report));

  // 4. A dropped room socket: the slot is reclaimed and the link reopens.
  await guest.goto(`${base}/play.html?room=${code}&dropAfter=4&name=LinkGuest&weather=off`);
  await guest.waitForFunction(
    () => {
      const state = JSON.parse(window.furyMultiplayer ?? 'null');
      return state?.reconnects >= 1 && state.peer != null;
    },
    null,
    { timeout: 180000 },
  );
  const again = (await engine(guest)).peer;
  await linked(guest, hostPeer);
  await linked(host, again);
  await host.waitForTimeout(1500);
  const [h4, g4] = await drive(host, [host, guest], 3000);
  report.reconnected = { host: h4, guest: g4 };
  assert.ok(g4.directReceived >= 15 && h4.messages <= 3, JSON.stringify(report));
  g = await engine(guest);
  h = await engine(host);
  assert.ok(fresh(g, hostPeer) && fresh(h, again), JSON.stringify({ g, h }));
  assert.equal(h.closed, null);
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
