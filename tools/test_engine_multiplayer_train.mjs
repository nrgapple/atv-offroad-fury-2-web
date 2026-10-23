// Host-only gate: Arlington's shared train in a Rust engine room. A Rust
// browser host rides env08; a JavaScript guest joins. Both derive the train
// phase from the relay clock, and a guest standing on the flatbed is drawn at
// the same deck-relative spot by the Rust host.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_train.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, base, jsBase, launch, newPage, waitForSteps } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const report = {};
const errors = [];
const browser = await launch();
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const js = p => p.evaluate(() => window.atvTerrainStatus);
// Deck-local coordinates of point p in car matrix m (column-major).
const local = (m, p) =>
  [0, 1, 2].map(axis => [0, 1, 2].reduce((sum, j) => sum + m[axis * 4 + j] * (p[j] - m[12 + j]), 0));
try {
  // 1. Rust browser host on Arlington.
  const host = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await host.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  await host.goto(`${base}/play.html?host=1&venue=env08&name=RustHost`);
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.code, null, { timeout: 180000 });
  const code = (await engine(host)).code;
  report.code = code;

  // 2. JavaScript guest with the worker test hook (test_multiplayer_features.mjs).
  const guest = await newPage(browser, { viewport: { width: 1280, height: 720 }, hasTouch: true }, errors);
  await guest.addInitScript(() => {
    // Realistic train riding keeps a parked rider on the moving deck; in the
    // Original mode both engines slide it off in the curves
    // (test_train_riding.mjs), which made the deck checks depend on the route
    // phase at placement.
    localStorage.setItem('atv-train-riding', 'realistic');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    Object.defineProperty(document, 'hidden', { get: () => Boolean(window.testFreeze) });
    const Native = Worker;
    let id = -1000;
    window.Worker = class extends Native {
      constructor(...args) {
        super(...args);
        window.testWorker = this;
        this.pending = new Map();
        this.addEventListener('message', ({ data }) => {
          const r = this.pending.get(data.id);
          if (!r) return;
          this.pending.delete(data.id);
          data.error ? r.reject(Error(data.error)) : r.resolve(JSON.parse(new TextDecoder().decode(data.bytes)));
        });
      }
    };
    window.testCommand = (op, value) =>
      new Promise((resolve, reject) => {
        const key = id--;
        testWorker.pending.set(key, { resolve, reject });
        testWorker.postMessage({
          id: key,
          op,
          index: 0,
          bytes: new TextEncoder().encode(value == null ? '' : JSON.stringify(value))
        });
      });
  });
  await guest.goto(`${jsBase}/play.html?room=${code}&player=JsGuest`);
  await guest.waitForFunction(
    () => {
      const s = window.atvTerrainStatus;
      return s?.multiplayerError || (s?.simulation && s.multiplayer?.remote?.some(r => !r.stale) && s.train?.time > 0);
    },
    null,
    { timeout: 180000 }
  );
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });

  // 3. Shared train phase: both follow the relay clock.
  const phases = [];
  for (let i = 0; i < 10; i++) {
    const [r, j] = await Promise.all([engine(host), js(guest)]);
    const duration = j.train.frame?.duration ?? j.train.duration;
    let d = r.trainTime - j.train.time;
    if (duration) d = ((d + duration * 1.5) % duration) - duration * 0.5;
    phases.push(d);
    await host.waitForTimeout(200);
  }
  report.phaseDifferenceS = { max: Math.max(...phases.map(Math.abs)), samples: phases.length };
  assert.ok(report.phaseDifferenceS.max < 0.3, `train phases should match: ${JSON.stringify(phases)}`);

  // 4. The guest stands on the flatbed; the Rust host sees it deck-relative.
  // As test_multiplayer_features.mjs: freeze the guest's clocks while
  // placing it on the moving deck so placement and velocity share a frame.
  const placeOnDeck = async () => {
    await guest.evaluate(() => {
      window.testFreeze = true;
      document.dispatchEvent(new Event('visibilitychange'));
    });
    await guest.waitForTimeout(150); // Time-based on purpose: let in-flight replies land.
    const placed = await guest.evaluate(async () => {
      const previous = await testCommand(68, null);
      const train = await testCommand(68, { dt: 1 / 60 });
      // test_train_riding.mjs's spot: 8 ft above flatbed car 6's centre.
      const car = 6;
      if (!/flatbeda/i.test(train.cars[car].model)) throw Error(`car 6 is ${train.cars[car].model}`);
      const m = train.cars[car].matrix;
      const origin = [0, 1, 2].map(i => m[12 + i] + m[4 + i] * 8);
      const velocity = [0, 1, 2].map(i => (m[12 + i] - previous.cars[car].matrix[12 + i]) * 60);
      await testCommand(23, { origin, velocity, yaw: Math.atan2(m[8], m[10]) });
      return { car, model: train.cars[car].model };
    });
    await guest.evaluate(() => {
      window.testFreeze = false;
      document.dispatchEvent(new Event('visibilitychange'));
    });
    return placed;
  };
  // Landing on the moving deck depends on the route phase and frame timing in
  // both engines: a rider can slide along or off the car after the drop (the
  // JavaScript guest as well as the Rust rider). That is placement, not the
  // deck-relative presentation under test, so each check re-places up to three
  // times until a window of 20 samples has at least 10 on a platform.
  let setup;
  const attempts = [];
  let samples = [];
  for (let attempt = 0; attempt < 3 && samples.length < 10; attempt++) {
    setup = await placeOnDeck();
    const seen = await host
      .waitForFunction(
        car => JSON.parse(window.furyMultiplayer ?? 'null')?.remote?.some(r => r.platform === car),
        setup.car,
        { timeout: 20000 }
      )
      .then(() => true)
      .catch(() => false);
    if (!seen) {
      attempts.push({ seen });
      continue;
    }
    await waitForSteps(guest, 45);
    samples = [];
    const deck = [];
    for (let i = 0; i < 20; i++) {
      const [r, j] = await Promise.all([engine(host), js(guest)]);
      const own = j.train.frame?.cars?.[setup.car];
      if (own) deck.push(local(own.matrix, j.frame.origin).map(v => Math.round(v)));
      const peer = r.remote.find(p => p.platform != null);
      if (peer && j.train.frame?.cars?.[peer.platform]) {
        const owner = local(j.train.frame.cars[peer.platform].matrix, j.frame.origin);
        const seenAt = local(r.trainCars[String(peer.platform)], peer.position);
        samples.push(Math.hypot(...owner.map((v, k) => v - seenAt[k])));
      }
      await host.waitForTimeout(50);
    }
    attempts.push({ seen, samples: samples.length, deck: [deck[0], deck.at(-1)] });
  }
  report.flatbed = { ...setup, attempts };
  report.deckErrorFt = { max: Math.max(...samples), samples: samples.length };
  assert.ok(samples.length >= 10, `platform samples: ${JSON.stringify(attempts)}`);
  assert.ok(report.deckErrorFt.max < 4, `deck-relative error: ${JSON.stringify(samples)}`);
  // 5. A Rust rider stands on a flatbed (?trainPlace test hook, car 3's
  // centre); the JavaScript guest sees it deck-relative.
  let rider;
  let rustOwned = [];
  const riderAttempts = [];
  for (let attempt = 0; attempt < 3 && rustOwned.length < 10; attempt++) {
    if (rider) await rider.close();
    rider = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await rider.addInitScript(() => {
      localStorage.setItem('atv-train-riding', 'realistic');
      Object.defineProperty(document, 'hasFocus', { value: () => true });
    });
    await rider.goto(`${base}/play.html?room=${code}&name=RustRider&trainPlace=6`);
    await rider.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.ownPlatform, null, {
      timeout: 180000
    });
    // One read: the rider may already have slid off by a second one.
    const first = await engine(rider);
    const riderPeer = first.peer;
    const placed = first.ownPlatform?.car;
    if (placed == null) {
      riderAttempts.push({ placed: null });
      continue;
    }
    const seen = await guest
      .waitForFunction(
        peer => window.atvTerrainStatus.multiplayer?.remote?.some(r => r.peer === peer && r.platform != null),
        riderPeer,
        { timeout: 20000 }
      )
      .then(() => true)
      .catch(() => false);
    rustOwned = [];
    const deck = [];
    for (let i = 0; seen && i < 20; i++) {
      const [r, j] = await Promise.all([engine(rider), js(guest)]);
      const car = r.trainCars?.[String(placed)] ?? r.ownPlatform?.matrix;
      if (car) deck.push(local(car, r.origin).map(v => Math.round(v)));
      const peer = j.multiplayer.remote.find(p => p.peer === riderPeer && p.platform != null);
      if (peer && r.ownPlatform && j.train.frame?.cars?.[peer.platform]) {
        const owner = local(r.ownPlatform.matrix, r.origin);
        const seenAt = local(j.train.frame.cars[peer.platform].matrix, peer.position);
        rustOwned.push(Math.hypot(...owner.map((v, k) => v - seenAt[k])));
      }
      await rider.waitForTimeout(50);
    }
    riderAttempts.push({ placed, seen, samples: rustOwned.length, deck: [deck[0], deck.at(-1)] });
  }
  report.rustRider = riderAttempts;
  report.rustOnDeckErrorFt = { max: Math.max(...rustOwned), samples: rustOwned.length };
  assert.ok(rustOwned.length >= 10, `Rust platform samples: ${JSON.stringify(riderAttempts)}`);
  assert.ok(report.rustOnDeckErrorFt.max < 4, `Rust deck-relative error: ${JSON.stringify(rustOwned)}`);
  if (shots) await rider.screenshot({ path: shots.replace('.png', '-rust-rider.png') });
  if (shots) {
    await host.screenshot({ path: shots.replace('.png', '-rust.png') });
    await guest.screenshot({ path: shots.replace('.png', '-js.png') });
  }
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
