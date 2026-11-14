// Instant replay (#542): Pause → Replay plays the recorded stretch over the
// paused ride with play/pause, slow motion, skips, the scrub bar and three
// cameras; photo mode opens from a paused replay and returns to it; Back
// returns to Pause with the ride exactly where it stopped. A phone gets
// touch-size controls. REPLAY_SHOTS=dir saves a screenshot per camera.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const wait = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 30000 });
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === name, name);
const centre = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const menuHit = async (page, action) => (await read(page)).toolbar.menu.hits.find(h => h.action === action);
const viewerHit = async (page, id) => (await read(page)).toolbar.hits.find(h => h.id === id);
const distance = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);
const shots = process.env.REPLAY_SHOTS;
const shoot = async (page, name) => { if (shots) await page.screenshot({ path: `${shots}/${name}.png` }); };

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(page, 30);
  await page.mouse.click(640, 400);
  // Ride a few seconds so there is a stretch to replay.
  await page.keyboard.down('KeyW');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).step > 300);
  await page.keyboard.up('KeyW');
  await key(page, 'Escape');
  await onPage(page, 'pause');
  const paused = await read(page);
  assert.ok(paused.toolbar.menu.choices.includes('replay'), `Replay row: ${paused.toolbar.menu.choices}`);
  const { step, origin } = paused;
  let at = centre(await menuHit(page, 'replay'));
  await page.mouse.click(at.x, at.y);
  await wait(page, () => { const s = JSON.parse(document.querySelector('#ui-state').textContent); return s.replay && !s.toolbar.menu; });
  let s = await read(page);
  const [first, last] = s.replay.span;
  assert.ok(last - first >= 240 && last <= step, `recorded span ${first}..${last} (ride at ${step})`);
  assert.equal(s.replay.fromResults, false);
  assert.ok(distance(s.replay.origin, origin) > 2, 'the replay shows an earlier moment');
  assert.deepEqual(s.toolbar.hits.map(h => h.id).sort(), [50, 54, 55, 56, 57, 58, 59, 60]);
  await shoot(page, 'chase');
  // Playing advances in real time; S slows it; Space pauses.
  const t0 = s.replay.time;
  await nextFrames(page, 20);
  const t1 = (await read(page)).replay.time;
  assert.ok(t1 > t0 + 5, `plays (${t0} → ${t1})`);
  await key(page, 'KeyS');
  assert.equal((await read(page)).replay.speed, 0.5);
  await key(page, 'Space');
  const held = (await read(page)).replay.time;
  await nextFrames(page, 10);
  assert.equal((await read(page)).replay.time, held, 'paused');
  // Skips and the scrub bar.
  await key(page, 'ArrowLeft');
  assert.ok(Math.abs((await read(page)).replay.time - Math.max(first, held - 120)) < 1e-6);
  const bar = await viewerHit(page, 60);
  await page.mouse.move(bar.x + bar.width * 0.75, bar.y + bar.height / 2);
  await page.mouse.down(); await nextFrames(page, 2);
  await page.mouse.move(bar.x + bar.width * 0.9, bar.y + bar.height / 2, { steps: 3 }); await nextFrames(page, 2);
  await page.mouse.up(); await nextFrames(page, 2);
  s = await read(page);
  const fraction = (s.replay.time - first) / (last - first);
  assert.ok(fraction > 0.8 && fraction < 0.98, `scrubbed to ${fraction}`);
  // The cameras: chase is the recorded ride camera; trackside, orbit and
  // the broadcast (built-in cuts: a free ride has no TV cameras) look at the
  // replayed rider from elsewhere.
  const chaseEye = s.view.eye;
  for (const shot of ['Trackside', 'Orbit', 'Broadcast']) {
    await key(page, 'KeyC');
    s = await read(page);
    assert.equal(s.replay.shot, shot);
    if (shot === 'Broadcast') assert.ok(!s.replay.authored && s.replay.setup === 'Crane', `built-in broadcast opens wide (${s.replay.setup})`);
    assert.ok(distance(s.view.eye, chaseEye) > 3, `${shot} moves the eye`);
    const toRider = [s.replay.origin[0] - s.view.eye[0], s.replay.origin[2] - s.view.eye[2]];
    const yaw = Math.atan2(toRider[0], toRider[1]);
    const diff = Math.abs(((yaw - s.view.yaw + 3 * Math.PI) % (2 * Math.PI)) - Math.PI);
    assert.ok(diff < 0.2, `${shot} looks at the rider (${diff})`);
    await shoot(page, shot.toLowerCase());
  }
  // Photo mode from the paused replay, and back to the replay.
  await key(page, 'KeyP');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).photo);
  assert.ok((await read(page)).replay, 'replay kept under photo mode');
  assert.ok(distance((await read(page)).photo.target, s.replay.origin) < 3, 'photo centres the replayed rider');
  await key(page, 'Escape');
  await wait(page, () => { const s = JSON.parse(document.querySelector('#ui-state').textContent); return !s.photo && s.replay && !s.toolbar.menu; });
  // Back to Pause: nothing moved the ride.
  await key(page, 'Escape');
  await onPage(page, 'pause');
  s = await read(page);
  assert.equal(s.replay, null);
  assert.equal(s.step, step);
  assert.deepEqual(s.origin, origin);
  await key(page, 'Escape');
  await wait(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).paused);
  assertNoErrors(errors);
  await page.close();

  // Phone: the controls fit and take taps.
  const phone = await newPage(browser, { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true }, errors);
  await phone.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await phone.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await nextFrames(phone, 90);
  const pause = (await read(phone)).toolbar.hits.find(h => h.id === 1);
  await phone.touchscreen.tap(pause.x + pause.width / 2, pause.y + pause.height / 2);
  await onPage(phone, 'pause');
  at = centre(await menuHit(phone, 'replay'));
  await phone.touchscreen.tap(at.x, at.y);
  await wait(phone, () => JSON.parse(document.querySelector('#ui-state').textContent).replay);
  for (const h of (await read(phone)).toolbar.hits) assert.ok(h.height >= 44 && h.x >= 0 && h.x + h.width <= 390, `control ${h.id} on screen`);
  at = centre(await viewerHit(phone, 54));
  await phone.touchscreen.tap(at.x, at.y); await nextFrames(phone, 3);
  assert.equal((await read(phone)).replay.shot, 'Trackside');
  await shoot(phone, 'phone');
  at = centre(await viewerHit(phone, 50));
  await phone.touchscreen.tap(at.x, at.y);
  await onPage(phone, 'pause');
  assertNoErrors(errors);
  console.log('PASS: instant replay playback, slow motion, skips, scrub, four cameras, photo from replay, phone controls');
} finally {
  await browser.close();
}
