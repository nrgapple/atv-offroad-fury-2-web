// Host-only gate and measurement: what riders in a shared Free Ride room cost
// the relay (research/MULTIPLAYER.md "Free Ride relay intervals"). The relay
// is billed per incoming message, so this counts the WebSocket frames each
// page sends its room, and checks what the other rider sees.
//   1. Two Rust pages without links (`?p2p=0`) ride a private room on
//      Arlington: one drives, then rests. A private room keeps the ordinary
//      relay intervals, so this is the baseline.
//   2. The same two ride a shared Free Ride room: fewer messages a second,
//      and the watcher still sees the driver move without freezes, close to
//      where it really was.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_relay.mjs
// ATV_RELAY_SECONDS (default 5) is each drive; ATV_RELAY_REPORT writes the
// JSON report. Local relay only: never point this at the production site.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, writeJson } from './browser/harness.mjs';

assert.ok(/127\.0\.0\.1|localhost/.test(base), 'measurement runs only against a local relay');
const SECONDS = Number(process.env.ATV_RELAY_SECONDS ?? 5);
const KINDS = { 6: 'ping', 9: 'pose', 10: 'pose', 12: 'ready', 17: 'signal', 18: 'links', 19: 'bundle', 20: 'batch' };
const report = {};
const errors = [];
const browser = await launch();
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));

/** A page whose frames to its room socket are logged as [ms, packet kind]. */
async function open(query) {
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  page.sent = [];
  page.on('websocket', socket => {
    if (!socket.url().includes('/multiplayer/rooms/')) return;
    socket.on('framesent', frame =>
      page.sent.push([Date.now(), typeof frame.payload === 'string' ? -1 : frame.payload[5]]),
    );
  });
  await page.goto(`${base}/play.html?${query}&venue=env08&p2p=0&weather=off`);
  return page;
}

/** Every frame for `ms`: [wall ms, own position, [[peer, position, stale]]]. */
const trace = (page, ms) =>
  page.evaluate(
    ms =>
      new Promise(resolve => {
        const out = [];
        const end = performance.now() + ms;
        const step = () => {
          const s = JSON.parse(window.furyMultiplayer ?? 'null');
          if (s) out.push([Date.now(), s.origin, s.remote.map(r => [r.peer, r.position, r.stale])]);
          if (performance.now() < end) requestAnimationFrame(step);
          else resolve(out);
        };
        requestAnimationFrame(step);
      }),
    ms,
  );

/** Frames a page sent between two wall times, per second and by kind. */
function rate(page, from, until) {
  const kinds = {};
  let frames = 0;
  for (const [at, kind] of page.sent) {
    if (at < from || at >= until) continue;
    frames += 1;
    const name = KINDS[kind] ?? `kind${kind}`;
    kinds[name] = (kinds[name] ?? 0) + 1;
  }
  const seconds = (until - from) / 1000;
  return { perSecond: +(frames / seconds).toFixed(2), posesPerSecond: +((kinds.pose ?? 0) / seconds).toFixed(2), kinds };
}

const distance = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

/** The driver's true position at a wall time, between its own samples. */
function truth(path, at) {
  let i = path.findIndex(sample => sample[0] >= at);
  if (i <= 0) return null;
  const [t0, p0] = path[i - 1];
  const [t1, p1] = path[i];
  const k = t1 === t0 ? 0 : (at - t0) / (t1 - t0);
  return p0.map((v, axis) => v + (p1[axis] - v) * k);
}

/**
 * How the watcher drew the driver against where the driver was: the
 * playback delay that fits best, the position error left at that delay, the
 * longest time the drawn rider stood still while the real one moved, and the
 * largest jump between two frames against that frame's expected travel.
 */
function motion(seen, driven, peer) {
  const drawn = seen
    .map(([at, , remote]) => [at, remote.find(r => r[0] === peer)])
    .filter(([, r]) => r)
    .map(([at, r]) => [at, r[1], r[2]]);
  const path = driven.map(([at, origin]) => [at, origin]);
  let best = { delayMs: 0, rmsFt: Infinity };
  for (let delay = 0; delay <= 3000; delay += 10) {
    let sum = 0;
    let count = 0;
    let worst = 0;
    for (const [at, position] of drawn) {
      const real = truth(path, at - delay);
      if (!real) continue;
      const error = distance(position, real);
      sum += error * error;
      worst = Math.max(worst, error);
      count += 1;
    }
    if (count < drawn.length / 2) continue;
    const rms = Math.sqrt(sum / count);
    if (rms < best.rmsFt) best = { delayMs: delay, rmsFt: rms, maxFt: worst };
  }
  let frozenMs = 0;
  let still = 0;
  let jumpFt = 0;
  for (let i = 1; i < drawn.length; i++) {
    const [at, position] = drawn[i];
    const [before, last] = drawn[i - 1];
    const real = truth(path, at - best.delayMs);
    const realBefore = truth(path, before - best.delayMs);
    if (!real || !realBefore) continue;
    const moved = distance(position, last);
    const expected = distance(real, realBefore);
    // The real rider covered ground this frame and the drawn one did not.
    still = expected > 0.05 && moved < expected * 0.1 ? still + (at - before) : 0;
    frozenMs = Math.max(frozenMs, still);
    jumpFt = Math.max(jumpFt, moved - expected);
  }
  return {
    frames: drawn.length,
    stale: drawn.filter(sample => sample[2]).length,
    delayMs: best.delayMs,
    rmsFt: +best.rmsFt.toFixed(3),
    maxFt: +(best.maxFt ?? 0).toFixed(3),
    frozenMs,
    jumpFt: +jumpFt.toFixed(3),
    travelledFt: +distance(path[0][1], path.at(-1)[1]).toFixed(1),
  };
}

/** The driver drives then rests while the watcher watches. */
async function measure(driver, watcher) {
  const peer = (await engine(driver)).peer;
  // Both settle after arriving (contacts wait 2.5 s; first poses go at once).
  await driver.waitForTimeout(4000);
  await driver.bringToFront();
  const started = Date.now();
  const traces = Promise.all([trace(watcher, SECONDS * 1000 + 1500), trace(driver, SECONDS * 1000 + 1500)]);
  await driver.keyboard.down('KeyW');
  await driver.waitForTimeout(SECONDS * 1000);
  await driver.keyboard.up('KeyW');
  const stopped = Date.now();
  const [seen, driven] = await traces;
  // Let it roll to a stop, then rest.
  await driver.waitForTimeout(6000);
  const rested = Date.now();
  await driver.waitForTimeout(12_000);
  const end = Date.now();
  const state = await engine(watcher);
  assert.equal(state.closed, null);
  assert.ok(
    state.remote.some(r => r.peer === peer && !r.stale),
    `the resting driver stays fresh: ${JSON.stringify(state.remote)}`,
  );
  return {
    driving: rate(driver, started, stopped),
    resting: rate(driver, rested, end),
    watcherResting: rate(watcher, started, stopped),
    seen: motion(seen, driven, peer),
  };
}

try {
  // 1. A private room: the ordinary relay intervals.
  const code = `FR${String(Date.now()).slice(-6)}`;
  let driver = await open(`host=1&room=${code}&name=Driver`);
  await driver.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  let watcher = await open(`room=${code}&dropAfter=9999&name=Watcher`);
  await watcher.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 180000 });
  await driver.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });
  report.private = await measure(driver, watcher);
  await driver.close();
  await watcher.close();

  // 2. A shared Free Ride room.
  driver = await open('room=FREERD01&dropAfter=9999&name=Driver');
  await driver.waitForFunction(() => /^FREERD/.test(JSON.parse(window.furyMultiplayer ?? 'null')?.code ?? ''), null, {
    timeout: 180000,
  });
  await driver.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.environment === 'env08', null, {
    timeout: 180000,
  });
  watcher = await open('room=FREERD01&dropAfter=9999&name=Watcher');
  await watcher.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 180000 });
  await driver.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });
  assert.equal((await engine(watcher)).code, (await engine(driver)).code, 'both ride the same shared room');
  report.shared = await measure(driver, watcher);

  const { private: before, shared: after } = report;
  console.log(JSON.stringify(report, null, 2));
  if (process.env.ATV_RELAY_REPORT) writeJson(process.env.ATV_RELAY_REPORT, report);
  // The baseline is what it was: up to 20 poses a second driving, 1 resting.
  assert.ok(before.driving.posesPerSecond >= 12, JSON.stringify(before.driving));
  assert.ok(before.resting.posesPerSecond >= 0.8 && before.resting.posesPerSecond <= 1.3, JSON.stringify(before.resting));
  // Free Ride sends the relay fewer.
  assert.ok(after.driving.posesPerSecond <= 11, JSON.stringify(after.driving));
  assert.ok(after.driving.posesPerSecond <= before.driving.posesPerSecond * 0.7, JSON.stringify(report));
  assert.ok(after.resting.posesPerSecond <= 0.65, JSON.stringify(after.resting));
  // The watcher still sees the driver all the time, moving without freezes
  // or jumps, and near where it was a moment ago.
  for (const [name, { seen }] of Object.entries(report)) {
    assert.ok(seen.travelledFt > 40, `${name}: the driver drove: ${JSON.stringify(seen)}`);
    assert.equal(seen.stale, 0, `${name}: never stale: ${JSON.stringify(seen)}`);
    assert.ok(seen.frozenMs <= 120, `${name}: no freeze: ${JSON.stringify(seen)}`);
    assert.ok(seen.rmsFt <= 1.5 && seen.maxFt <= 6 && seen.jumpFt <= 3, `${name}: close to the truth: ${JSON.stringify(seen)}`);
  }
  assertNoErrors(errors);
} finally {
  await browser.close();
}
