// The Free Ride hub (research/FREERIDE_HUB.md) on a local game server:
//   1. Two riders share a Free Ride room; Ann sees Bo in Challenge range and
//      asks (G). Bo gets the Accept prompt with its ring timer and accepts (Y).
//   2. Both race to the server's waypoint; Bo is placed at the goal
//      (hubTest=goal) and wins once the server's speed check believes it.
//   3. A pop-up event opens a join circle; both ride into it (hubTest=join);
//      the event runs and both get the results toast with points.
// Needs fury-server with short hub timers behind tools/serve_multiplayer.mjs:
//   FURY_HUB_EVENT_SECS=20 FURY_HUB_JOIN_SECS=12 FURY_HUB_ROTATION_SECS=off make server-run
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_hub.mjs
// ATV_MP_SCREENSHOT=dir saves the prompt, the race and the event.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const report = {};
const hub = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null')?.hub ?? null);
const engine = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
// Waits until `test(hub, arg)` holds for the page's hub report. The test is
// sent as source (the page's CSP forbids eval; DevTools evaluation is exempt),
// so it may use only its arguments.
const hubIs = (page, test, arg = null, timeout = 60000) =>
  page.waitForFunction(
    `(() => {
      const hub = JSON.parse(window.furyMultiplayer ?? 'null')?.hub;
      return Boolean(hub && (${test.toString()})(hub, ${JSON.stringify(arg)}));
    })()`,
    null,
    { timeout }
  );
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 2);
}
async function shot(page, name) {
  if (shots) await page.screenshot({ path: `${shots}/${name}.png` });
}

async function rider(browser, errors, hooks, viewport) {
  const page = await newPage(browser, { viewport }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  await page.goto(url(`/play.html?uiDiagnostics=1&mode=multiplayer&room=FREERD01&map=env08&play=1&hubTest=${hooks}`));
  await wait(page, () => JSON.parse(window.furyMultiplayer ?? 'null')?.environment === 'env08', null, 180000);
  const e = await engine(page);
  assert.equal(e.serves.hub, true, 'the server runs the hub');
  return page;
}

const browser = await launch();
const errors = [];
try {
  const ann = await rider(browser, errors, 'join', {
    width: 1280,
    height: 720
  });
  const bo = await rider(browser, errors, 'goal,join', {
    width: 390,
    height: 844
  });
  const annPeer = (await engine(ann)).peer;
  const boPeer = (await engine(bo)).peer;
  assert.notEqual(annPeer, boPeer);
  report.riders = { ann: annPeer, bo: boPeer, met: (await engine(bo)).met };

  // 1. Bo arrives beside Ann: both are hub riders, and Ann can challenge.
  await hubIs(ann, (h, peer) => h.nearby === peer && (h.riders & (1 << peer)) !== 0, boPeer);
  await shot(ann, 'challenge-button');
  await ann.mouse.click(640, 420);
  await key(ann, 'KeyG');
  await hubIs(bo, h => h.duel.state === 'offered', null, 10000);
  const offered = await hub(bo);
  assert.equal(offered.duel.peer, annPeer);
  await shot(bo, 'challenge-offer');
  await bo.mouse.click(195, 600);
  await key(bo, 'KeyY');

  // 2. The race: both see the goal; Bo stands on it and wins.
  await hubIs(ann, h => h.duel.state === 'racing' || h.outcomes.length > 0, null, 10000);
  const racing = await hub(ann);
  if (racing.duel.state === 'racing') {
    report.goal = racing.duel.target;
    await shot(ann, 'challenge-race');
  }
  await hubIs(ann, h => h.outcomes.some(o => o.kind === 'challenge'), null, 30000);
  await hubIs(bo, h => h.outcomes.some(o => o.kind === 'challenge'), null, 10000);
  const annResult = (await hub(ann)).outcomes.find(o => o.kind === 'challenge');
  const boResult = (await hub(bo)).outcomes.find(o => o.kind === 'challenge');
  assert.deepEqual(annResult.winners, [boPeer], 'the server judged Bo first');
  assert.equal(boResult.points, 50);
  assert.equal(annResult.points, 10);
  await shot(bo, 'challenge-win');
  report.challenge = { ann: annResult, bo: boResult };

  // 3. A pop-up event: the join circle, both ride in, the results.
  await hubIs(ann, h => h.event && h.event.phase === 0, null, 120000);
  const open = (await hub(ann)).event;
  await shot(ann, 'event-join');
  await hubIs(ann, (h, mask) => h.event && (h.event.mask & mask) === mask, (1 << annPeer) | (1 << boPeer), 30000);
  report.event = { kind: open.kind, point: open.point };
  await hubIs(ann, h => h.event && h.event.phase === 1, null, 30000);
  await nextFrames(ann, 10);
  await shot(ann, 'event-run');
  await hubIs(ann, h => h.outcomes.some(o => o.kind !== 'challenge'), null, 90000);
  await hubIs(bo, h => h.outcomes.some(o => o.kind !== 'challenge'), null, 10000);
  await shot(ann, 'event-results');
  const annEvent = (await hub(ann)).outcomes.find(o => o.kind !== 'challenge');
  const boEvent = (await hub(bo)).outcomes.find(o => o.kind !== 'challenge');
  assert.equal(annEvent.serial, boEvent.serial);
  assert.deepEqual(annEvent.winners.slice().sort(), [annPeer, boPeer].sort());
  assert.deepEqual(
    [annEvent.points, boEvent.points].sort((a, b) => a - b),
    [60, 100],
    'two joiners: 100 and 60 points'
  );
  if (open.kind !== 'Stunt-Off') assert.equal(boEvent.points, 100, 'Bo stood on the goal or hill');
  report.eventResults = { ann: annEvent, bo: boEvent };
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
