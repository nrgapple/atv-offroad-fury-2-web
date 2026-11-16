// Free Ride spawn spots (issue #710, research/FREERIDE_HUB.md "Spawn spots")
// on a local game server:
//   1. Ann arrives in a shared Free Ride room and starts at one of the
//      venue's validated spots (`spawn` in window.furyMultiplayer), not
//      beside anyone.
//   2. Bo arrives: a different spot, at least 40 ft (spots::RIDER_CLEAR)
//      from Ann, and Ann sees Bo there.
//   3. With ATV_SPAWN_ROTATIONS=n and a short rotation, the room moves n
//      times; after each move both start at their slots' own spots on the
//      new venue ("slot"), apart.
//   make server-run   (or FURY_HUB_ROTATION_SECS=90 make server-run for step 3)
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_spawns.mjs
// ATV_MP_SCREENSHOT=dir saves each rider's view after every arrival.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const rotations = Number(process.env.ATV_SPAWN_ROTATIONS ?? 0);
const report = { arrivals: [] };
const engine = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);
// Polled from Node: the page's CSP forbids string predicates.
async function until(page, test, what, timeout = 180000) {
  const end = Date.now() + timeout;
  for (;;) {
    const state = await engine(page);
    if (state && test(state)) return state;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}: ${JSON.stringify(state?.spawn)}`);
    await new Promise(resolve => setTimeout(resolve, 200));
  }
}

async function rider(browser, errors, name, viewport) {
  const page = await newPage(browser, { viewport }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  await page.goto(url(`/play.html?uiDiagnostics=1&mode=multiplayer&room=FREERD01&map=env08&play=1&name=${name}`));
  return page;
}

/** Both riders on `environment` with their spawns and each seeing the other. */
async function arrival(ann, bo, environment, how) {
  const states = [];
  for (const page of [ann, bo]) {
    states.push(
      await until(
        page,
        s => s.environment === environment && s.spawn && s.drawn >= 1 && s.remote.some(r => !r.stale),
        `${environment} spawn`
      )
    );
  }
  // The second rider's recheck (once the first one's poses are in) settles.
  await nextFrames(bo, 90);
  const [a, b] = [await engine(ann), await engine(bo)];
  for (const s of [a, b]) {
    assert.ok(s.spawn.spots >= 20, `spots on ${environment}: ${s.spawn.spots}`);
    assert.ok(how.includes(s.spawn.how), `spawn chosen by ${s.spawn.how}`);
    assert.equal(s.met, null, 'nobody arrives beside a rider');
  }
  const apart = flat(a.spawn.origin, b.spawn.origin);
  assert.ok(apart >= 40, `spots ${apart.toFixed(1)} ft apart`);
  assert.notEqual(a.spawn.index, b.spawn.index);
  // Each sees the other where it started (it has not moved: no input).
  const boSeen = a.remote.find(r => r.peer === b.peer && !r.stale);
  assert.ok(boSeen && flat(boSeen.position, b.spawn.origin) < 12, `Bo seen at ${boSeen?.position}`);
  const entry = { environment, apartFt: Math.round(apart), ann: a.spawn, bo: b.spawn };
  report.arrivals.push(entry);
  if (shots) {
    await ann.screenshot({ path: `${shots}/${environment}-ann.png` });
    await bo.screenshot({ path: `${shots}/${environment}-bo.png` });
  }
  return entry;
}

const browser = await launch();
const errors = [];
try {
  const ann = await rider(browser, errors, 'Ann', { width: 1280, height: 720 });
  const first = await until(ann, s => s.environment && s.spawn, 'the first arrival');
  report.first = { environment: first.environment, spawn: first.spawn };
  assert.equal(first.spawn.how, 'random');
  assert.equal(first.met, null);
  const bo = await rider(browser, errors, 'Bo', { width: 960, height: 540 });
  let venue = first.environment;
  await arrival(ann, bo, venue, ['random', 'recheck']);
  for (let n = 0; n < rotations; n++) {
    const moved = await until(ann, s => s.environment && s.environment !== venue, `rotation ${n + 1}`, 600000);
    venue = moved.environment;
    await arrival(ann, bo, venue, ['slot', 'recheck']);
  }
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
