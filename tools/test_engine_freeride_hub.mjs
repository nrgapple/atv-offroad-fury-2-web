// The Free Ride hub (research/FREERIDE_HUB.md) on a local game server:
//   1. Two riders share a Free Ride room; Ann sees Bo in Challenge range and
//      asks (G). Bo gets the Accept prompt with its ring timer and accepts (Y).
//   2. Both race to the server's waypoint; Bo is placed at the goal
//      (hubTest=goal) and wins once the server's speed check believes it.
//   3. A rider on a build without the hub (?hub=0: no HUB bit) joins and
//      hears no hub packets. A pop-up event opens a join circle; Ann and Bo
//      ride into it (hubTest=join); the event runs and both get the results
//      toast with points.
//   4. Map rotation: a minute before the move both hub riders vote; the whole
//      room, the older build included, moves to the voted venue, and every
//      rider sees the other two there and keeps riding.
// Needs fury-server with these hub timers behind tools/serve_multiplayer.mjs
// (first event 75 s after the room opens, after the challenge; the move at
// 300 s, after the event, with no second event in between):
//   FURY_HUB_EVENT_SECS=75 FURY_HUB_JOIN_SECS=12 FURY_HUB_ROTATION_SECS=300 make server-run
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_hub.mjs
// ATV_MP_SCREENSHOT=dir saves the prompt, the race and the event.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const report = {};
const hub = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null')?.hub ?? null);
const engine = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
// Waits until `test(hub, arg)` holds for the page's hub report, polled from
// Node (the page's CSP forbids the eval a string predicate would need).
async function hubIs(page, test, arg = null, timeout = 60000) {
  const until = Date.now() + timeout;
  for (;;) {
    const state = await hub(page);
    if (state && test(state, arg)) return state;
    if (Date.now() > until) throw new Error(`hub wait timed out: ${test}\n${JSON.stringify(state)}`);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
}
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 2);
}
async function shot(page, name) {
  if (shots) await page.screenshot({ path: `${shots}/${name}.png` });
}

async function rider(browser, errors, hooks, viewport, extra = '') {
  const page = await newPage(browser, { viewport }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  await page.goto(
    url(`/play.html?uiDiagnostics=1&mode=multiplayer&room=FREERD01&map=env08&play=1&hubTest=${hooks}${extra}`)
  );
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

  // 3. A rider on an older build: no HUB bit, no hub packets.
  const old = await rider(browser, errors, '', { width: 960, height: 540 }, '&hub=0');
  const oldPeer = (await engine(old)).peer;
  report.riders.old = oldPeer;

  // A pop-up event: the join circle, both ride in, the results.
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
  // Ann stays in the circle and Bo is placed on the goal or hill
  // (hubTest=goal): only a rider who scored takes a podium place, so Bo gets
  // 100 and Ann the 10 for taking part. Nobody lands stunts in a Stunt-Off,
  // so both get 10. (Before the scoring fix, and while the join and goal
  // hooks shared one placement per event, Bo never reached the hill and Ann
  // won a Hill King on slot order with 0 s held.)
  if (open.kind === 'Stunt-Off') {
    assert.deepEqual([annEvent.points, boEvent.points], [10, 10], 'no stunts, no podium');
  } else {
    assert.equal(boEvent.points, 100, 'Bo reached the goal or held the hill');
    assert.equal(annEvent.points, 10, 'Ann took part');
  }
  report.eventResults = { ann: annEvent, bo: boEvent };

  // 4. Map rotation: the vote, the move, everyone on the new venue together.
  await hubIs(ann, h => h.vote && h.vote.maps.length === 3, null, 240000);
  const vote = (await hub(ann)).vote;
  await shot(ann, 'rotation-vote');
  await key(ann, 'Digit1');
  await key(bo, 'Digit1');
  await hubIs(ann, h => h.vote && h.vote.votes[0] === 2, null, 10000);
  const venue = `env0${vote.maps[0]}`;
  report.rotation = { maps: vote.maps, venue };
  for (const page of [ann, bo, old]) {
    await wait(page, venue => JSON.parse(window.furyMultiplayer ?? 'null')?.environment === venue, venue, 150000);
  }
  for (const page of [ann, bo, old]) {
    await wait(
      page,
      () => {
        const e = JSON.parse(window.furyMultiplayer ?? 'null');
        return e && e.closed === null && e.remote.filter(r => !r.stale).length >= 2;
      },
      null,
      120000
    );
  }
  await nextFrames(ann, 10);
  await shot(ann, 'rotation-moved');
  const oldState = await engine(old);
  assert.equal(oldState.closed, null, 'the older build keeps riding');
  assert.equal(oldState.hub.riders, 0, 'the older build heard no hub packets');
  report.rotation.seen = {
    ann: (await engine(ann)).remote.length,
    bo: (await engine(bo)).remote.length,
    old: oldState.remote.length
  };
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
