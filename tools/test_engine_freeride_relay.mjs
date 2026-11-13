// Host-only gate and measurement: what riders in a shared Free Ride room cost
// the relay, and what the other riders see (research/MULTIPLAYER.md "Free
// Ride relay intervals" and "Free Ride pose batches"). The relay is billed
// per incoming message, so this counts the WebSocket frames each page sends
// its room and compares the drawn riders with where they really were.
//   1. Two Rust pages without links (`?p2p=0`) ride a private room on
//      Arlington: one drives, then rests. A private room keeps the ordinary
//      relay intervals: the baseline.
//   2. Three ride a shared Free Ride room. One drives a long way off, with
//      a few hops: its batches thin out with distance, and the watchers draw
//      it later the farther it is, without freezes or jumps.
//   3. A fourth joins as a build without batches (`?batches=0`): the relay
//      unpacks batches for it, the others shorten theirs, and old and new
//      see each other move.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_relay.mjs
// ATV_RELAY_SECONDS (default 26) is the long drive; ATV_RELAY_REPORT writes
// the JSON report and ATV_RELAY_TRACE the watchers' frame-by-frame traces.
// Local relay only: never point this at the production site.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, writeJson } from './browser/harness.mjs';

assert.ok(/127\.0\.0\.1|localhost/.test(base), 'measurement runs only against a local relay');
const SECONDS = Number(process.env.ATV_RELAY_SECONDS ?? 26);
const KINDS = { 6: 'ping', 9: 'pose', 10: 'pose', 12: 'ready', 17: 'signal', 18: 'links', 19: 'bundle', 20: 'batch' };
// The batch table: ft to the nearest rider who needs the relay, messages/s.
const TIERS = [
  { name: 'under60ft', below: 60, perSecond: 5 },
  { name: '60to300ft', below: 300, perSecond: 1000 / 300 },
  { name: '300to1000ft', below: 1000, perSecond: 1000 / 600 },
  { name: 'beyond1000ft', below: Infinity, perSecond: 1000 / 1500 },
];
const report = {};
const traces = {};
const errors = [];
const browser = await launch();
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

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
const riding = (page, drawn = 0) =>
  page.waitForFunction(
    n => {
      const state = JSON.parse(window.furyMultiplayer ?? 'null');
      return state?.environment === 'env08' && state.drawn >= n;
    },
    drawn,
    { timeout: 180000 },
  );

/**
 * Every frame for `ms`: [wall ms, own position, own state,
 * [[peer, position, stale, delay ms]]].
 */
const trace = (page, ms) =>
  page.evaluate(
    ms =>
      new Promise(resolve => {
        const out = [];
        const end = performance.now() + ms;
        const step = () => {
          const s = JSON.parse(window.furyMultiplayer ?? 'null');
          if (s) {
            out.push([
              Date.now(),
              s.origin,
              s.state,
              s.remote.map(r => [r.peer, r.position, r.stale, r.delayMs]),
            ]);
          }
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
  const poses = (kinds.pose ?? 0) + (kinds.batch ?? 0) + (kinds.bundle ?? 0);
  return { perSecond: +(frames / seconds).toFixed(2), posesPerSecond: +(poses / seconds).toFixed(2), kinds };
}

const distance = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
const round = (value, digits = 2) => +value.toFixed(digits);

/** The driver's true position at a wall time, between its own samples. */
function truth(path, at) {
  const i = path.findIndex(sample => sample[0] >= at);
  if (i <= 0) return null;
  const [t0, p0] = path[i - 1];
  const [t1, p1] = path[i];
  const k = t1 === t0 ? 0 : (at - t0) / (t1 - t0);
  return p0.map((v, axis) => v + (p1[axis] - v) * k);
}

/**
 * How a watcher drew the driver against where the driver was. Each frame is
 * compared with the driver's position one playback delay earlier (the delay
 * the watcher reports for that frame when it does, plus one fitted constant
 * for the rest of the pipeline): the position error left, the longest time
 * the drawn rider stood still while the real one moved, and the largest
 * jump in one frame beyond that frame's real travel. `where` puts a frame
 * into a group (a distance tier, the driver's state); null leaves it out.
 */
function motion(seen, driven, peer, where = () => 'all') {
  const drawn = seen
    .map(([at, own, , remote]) => [at, remote.find(r => r[0] === peer), own])
    .filter(([, r]) => r)
    .map(([at, r, own]) => ({ at, position: r[1], stale: r[2], delay: r[3], own }));
  const path = driven.map(([at, origin, state]) => [at, origin, state]);
  const reported = drawn.every(d => Number.isFinite(d.delay));
  let best = { fit: 0, rms: Infinity };
  for (let fit = 0; fit <= (reported ? 200 : 3000); fit += 10) {
    let sum = 0;
    let count = 0;
    for (const d of drawn) {
      const real = truth(path, d.at - fit - (reported ? d.delay : 0));
      if (!real) continue;
      const error = distance(d.position, real);
      sum += error * error;
      count += 1;
    }
    if (count < drawn.length / 2) continue;
    const rms = Math.sqrt(sum / count);
    if (rms < best.rms) best = { fit, rms };
  }
  const groups = {};
  let still = 0;
  for (let i = 1; i < drawn.length; i++) {
    const d = drawn[i];
    const before = drawn[i - 1];
    const lag = best.fit + (reported ? d.delay : 0);
    const real = truth(path, d.at - lag);
    const realBefore = truth(path, before.at - lag);
    const state = path.findLast(sample => sample[0] <= d.at - lag)?.[2];
    if (!real || !realBefore) continue;
    const name = where({ real, own: d.own, state });
    if (!name) continue;
    const g = (groups[name] ??= { frames: 0, stale: 0, sum: 0, maxFt: 0, frozenMs: 0, jumpFt: 0, delays: [] });
    const moved = distance(d.position, before.position);
    const expected = distance(real, realBefore);
    const error = distance(d.position, real);
    // The real rider covered ground this frame and the drawn one did not.
    still = expected > 0.05 && moved < expected * 0.1 ? still + (d.at - before.at) : 0;
    g.frames += 1;
    g.stale += d.stale ? 1 : 0;
    g.sum += error * error;
    g.maxFt = Math.max(g.maxFt, error);
    g.frozenMs = Math.max(g.frozenMs, still);
    g.jumpFt = Math.max(g.jumpFt, moved - expected);
    g.delays.push(lag);
  }
  for (const [name, g] of Object.entries(groups)) {
    const delays = g.delays.sort((a, b) => a - b);
    groups[name] = {
      frames: g.frames,
      stale: g.stale,
      delayMs: Math.round(delays[Math.floor(delays.length / 2)]),
      rmsFt: round(Math.sqrt(g.sum / g.frames), 3),
      maxFt: round(g.maxFt, 3),
      frozenMs: g.frozenMs,
      jumpFt: round(g.jumpFt, 3),
    };
  }
  return { fitMs: best.fit, ...groups };
}
const tier = feet => TIERS.find(t => feet < t.below).name;

/** Waits until a page's rider has not moved for a second. */
async function rest(page) {
  const started = Date.now();
  for (let last = null, still = 0; still < 4; ) {
    await page.waitForTimeout(250);
    const { origin } = await engine(page);
    still = last && distance(origin, last) < 0.02 ? still + 1 : 0;
    last = origin;
    assert.ok(Date.now() - started < 60000, 'the rider came to rest');
  }
}

/**
 * The driver drives off for `seconds` (hopping when `hops`, in circles when
 * `circle`), then rests.
 */
async function drive(driver, watchers, seconds, { hops = false, circle = false } = {}) {
  await driver.bringToFront();
  const started = Date.now();
  const frames = Promise.all([driver, ...watchers].map(page => trace(page, seconds * 1000 + 2500)));
  await driver.keyboard.down('KeyW');
  if (circle) await driver.keyboard.down('KeyA');
  for (let elapsed = 0; elapsed < seconds * 1000; elapsed += 1000) {
    // A preload hop every four seconds: take-offs and landings.
    const hop = hops && elapsed % 4000 === 2000;
    if (hop) await driver.keyboard.down('Space');
    await driver.waitForTimeout(hop ? 400 : 1000);
    if (hop) {
      await driver.keyboard.up('Space');
      await driver.waitForTimeout(600);
    }
  }
  await driver.keyboard.up('KeyW');
  if (circle) await driver.keyboard.up('KeyA');
  const stopped = Date.now();
  const [driven, ...seen] = await frames;
  await rest(driver);
  await driver.waitForTimeout(2500);
  return { started, stopped, driven, seen };
}

try {
  // 1. A private room: the ordinary relay intervals.
  const code = `FR${String(Date.now()).slice(-6)}`;
  let driver = await open(`host=1&room=${code}&name=Driver`);
  await driver.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  let watcher = await open(`room=${code}&dropAfter=9999&name=Watcher`);
  await riding(watcher, 1);
  await riding(driver, 1);
  await driver.waitForTimeout(4000);
  let peer = (await engine(driver)).peer;
  // Circling beside the watcher: how a rider in contact range is drawn.
  const contact = ({ real, own }) => (distance(real, own) < 25 ? 'under25ft' : distance(real, own) < 60 ? 'under60ft' : null);
  let run = await drive(driver, [watcher], 8, { circle: true });
  const circling = { rate: rate(driver, run.started, run.stopped), seen: motion(run.seen[0], run.driven, peer, contact) };
  run = await drive(driver, [watcher], 5);
  let rested = Date.now();
  await driver.waitForTimeout(12_000);
  report.private = {
    circling,
    driving: rate(driver, run.started, run.stopped),
    resting: rate(driver, rested, Date.now()),
    seen: motion(run.seen[0], run.driven, peer).all,
  };
  await driver.close();
  await watcher.close();

  // 2. A shared Free Ride room, every rider on a build with batches.
  driver = await open('room=FREERD01&dropAfter=9999&name=Driver');
  await riding(driver);
  assert.match((await engine(driver)).code, /^FREERD/);
  const near = await open('room=FREERD01&dropAfter=9999&name=Near');
  await riding(near, 1);
  const third = await open('room=FREERD01&dropAfter=9999&name=Third');
  await riding(third, 2);
  await riding(driver, 2);
  assert.equal((await engine(near)).code, (await engine(driver)).code, 'all ride the same shared room');
  await driver.waitForTimeout(5000);
  peer = (await engine(driver)).peer;
  const counts = async page => {
    const s = await engine(page);
    return { batches: s.batches, batchesReceived: s.batchesReceived };
  };
  run = await drive(driver, [near, third], 8, { circle: true });
  const sharedCircling = {
    rate: rate(driver, run.started, run.stopped),
    seen: motion(run.seen[0], run.driven, peer, contact),
  };
  if (process.env.ATV_RELAY_TRACE) traces.circling = { driven: run.driven, seen: run.seen[0] };
  run = await drive(driver, [near, third], SECONDS, { hops: true });
  rested = Date.now();
  await driver.waitForTimeout(16_000);
  const restedUntil = Date.now();
  // The driver's distance to its nearest watcher (they stay put) picks its
  // tier; group what it sent and what the watchers drew by that.
  const places = run.seen.map(frames => frames[0][1]);
  const nearest = position => Math.min(...places.map(place => distance(position, place)));
  const path = run.driven.map(([at, origin]) => [at, origin]);
  const sent = {};
  for (const name of TIERS.map(t => t.name)) sent[name] = { seconds: 0, messages: 0 };
  for (let i = 1; i < run.driven.length; i++) {
    const [at, origin] = run.driven[i];
    if (at >= run.stopped) break;
    sent[tier(nearest(origin))].seconds += (at - run.driven[i - 1][0]) / 1000;
  }
  for (const [at, kind] of driver.sent) {
    const position = at >= run.started && at < run.stopped && kind !== 6 ? truth(path, at) : null;
    if (position) sent[tier(nearest(position))].messages += 1;
  }
  for (const s of Object.values(sent)) {
    s.seconds = round(s.seconds, 1);
    s.perSecond = s.seconds >= 1 ? round(s.messages / s.seconds) : null;
  }
  const states = {};
  for (const [, , state] of run.driven) states[state] = (states[state] ?? 0) + 1;
  report.shared = {
    circling: sharedCircling,
    driving: rate(driver, run.started, run.stopped),
    byDistance: sent,
    resting: rate(driver, rested, restedUntil),
    watcherResting: rate(near, run.started, run.stopped),
    travelledFt: round(nearest(run.driven.at(-1)[1]), 0),
    driverFrames: states,
    // The watcher beside the start: by the driver's distance, then by its
    // state (hops and wrecks).
    seen: motion(run.seen[0], run.driven, peer, ({ real }) => tier(distance(real, places[0]))),
    seenByState: motion(run.seen[0], run.driven, peer, ({ state }) => state),
    seenByThird: motion(run.seen[1], run.driven, peer).all,
    counts: { driver: await counts(driver), near: await counts(near) },
  };
  if (process.env.ATV_RELAY_TRACE) traces.shared = { driven: run.driven, seen: run.seen[0], sent: driver.sent };

  // 3. A rider on a build without batches joins: old and new see each other.
  const old = await open('room=FREERD01&dropAfter=9999&batches=0&name=Old');
  await riding(old, 3);
  await riding(driver, 3);
  await old.waitForTimeout(5000);
  const oldPeer = (await engine(old)).peer;
  run = await drive(driver, [old, near], 6);
  const mixed = {
    driving: rate(driver, run.started, run.stopped),
    seenByOld: motion(run.seen[0], run.driven, peer).all,
    seenByNear: motion(run.seen[1], run.driven, peer).all,
  };
  rested = Date.now();
  await driver.waitForTimeout(12_000);
  mixed.resting = rate(driver, rested, Date.now());
  run = await drive(old, [near, driver], 5);
  mixed.oldDriving = rate(old, run.started, run.stopped);
  mixed.oldSeenByNear = motion(run.seen[0], run.driven, oldPeer).all;
  mixed.oldSeenByDriver = motion(run.seen[1], run.driven, oldPeer).all;
  mixed.counts = { driver: await counts(driver), near: await counts(near), old: await counts(old) };
  report.mixed = mixed;
  for (const page of [driver, near, third, old]) assert.equal((await engine(page)).closed, null);

  console.log(JSON.stringify(report, null, 2));
  if (process.env.ATV_RELAY_REPORT) await writeJson(process.env.ATV_RELAY_REPORT, report);
  if (process.env.ATV_RELAY_TRACE) await writeJson(process.env.ATV_RELAY_TRACE, traces);

  const { private: before, shared } = report;
  // The baseline is what it was: up to 20 poses a second driving, 1 resting.
  assert.ok(before.driving.posesPerSecond >= 12, JSON.stringify(before.driving));
  assert.ok(before.resting.posesPerSecond >= 0.8 && before.resting.posesPerSecond <= 1.3, JSON.stringify(before.resting));
  // Free Ride: batches, no more often than each distance tier allows (and a
  // change of tier or state now and then), one pose every 5 s at rest.
  assert.ok(shared.counts.driver.batches > 0 && shared.counts.near.batchesReceived > 0, JSON.stringify(shared.counts));
  assert.ok(shared.travelledFt > 300, `the driver got away: ${shared.travelledFt} ft`);
  for (const t of TIERS) {
    const got = shared.byDistance[t.name];
    if (got.seconds < 3) continue;
    assert.ok(got.perSecond <= t.perSecond * 1.25 + 0.6, `${t.name}: ${JSON.stringify(got)}`);
  }
  assert.ok(shared.driving.posesPerSecond <= before.driving.posesPerSecond * 0.35, JSON.stringify(report));
  assert.ok(shared.resting.posesPerSecond <= 0.3, JSON.stringify(shared.resting));
  assert.ok(shared.watcherResting.perSecond <= 0.45, JSON.stringify(shared.watcherResting));
  // The watchers draw the driver all the way: never stale, no freezes, near
  // where it was one playback delay earlier, and later the farther it is.
  for (const [name, seen] of Object.entries(shared.seen)) {
    if (name === 'fitMs' || seen.frames < 60) continue;
    assert.equal(seen.stale, 0, `${name}: never stale: ${JSON.stringify(seen)}`);
    assert.ok(seen.frozenMs <= 150, `${name}: no freeze: ${JSON.stringify(seen)}`);
    assert.ok(seen.rmsFt <= 2.5 && seen.maxFt <= 12, `${name}: near the truth: ${JSON.stringify(seen)}`);
  }
  const delays = TIERS.map(t => shared.seen[t.name]).filter(seen => seen?.frames >= 60).map(seen => seen.delayMs);
  assert.ok(delays.length >= 2 && delays.every((d, i) => i === 0 || d > delays[i - 1]), `delay grows with distance: ${delays}`);
  // The build without batches sends and gets single poses; everyone still
  // sees everyone, and nobody waits longer than that build smooths.
  assert.deepEqual(mixed.counts.old, { batches: 0, batchesReceived: 0 });
  assert.ok(mixed.driving.posesPerSecond >= 2.5, `shorter batches for the old build: ${JSON.stringify(mixed.driving)}`);
  assert.ok(mixed.resting.posesPerSecond >= 0.4 && mixed.resting.posesPerSecond <= 0.65, JSON.stringify(mixed.resting));
  for (const name of ['seenByOld', 'seenByNear', 'oldSeenByNear', 'oldSeenByDriver']) {
    const seen = mixed[name];
    assert.ok(seen.frames > 60 && seen.stale === 0, `${name}: seen, never stale: ${JSON.stringify(seen)}`);
    assert.ok(seen.frozenMs <= 200 && seen.maxFt <= 15, `${name}: moves with the rider: ${JSON.stringify(seen)}`);
  }
  assertNoErrors(errors);
} finally {
  await browser.close();
}
