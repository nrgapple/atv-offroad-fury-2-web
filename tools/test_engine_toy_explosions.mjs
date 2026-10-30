// Gameplay-started TOY emitters (research/AMBIENT_EFFECTS.md "Triggered
// bursts"): a trigger's `Particles` action (0023f460) starts its explosion.
//
// 1. Treasure Hunt on Prospect Springs (`gameTest=visit` sets the ATV on
//    each standing treasure): the game's own mode TOY (env05th.toy) is the
//    appended emitter TOY, every bomb.mcf animation carries an instance of
//    bomb_explosion.emt, and a collect starts that bomb's one-shot. The
//    burst is CPU type11 streams (UseGpuParticles off, 001a0130) drawn as
//    additive atlas sprites from the bounded stream pool, and it ends.
// 2. A Freestyle target run (Amateur round 1, Hit Every Green Target,
//    `objectiveTest=targets`): the objective's target TOY (env05gcp.toy) is
//    appended after the mode TOY and a hit starts its cherry explosion.
// 3. Hockey (`hockeyGoal=3` tees the first three drops for the human): the
//    third goal is a hat trick, which starts the four `[HatTrickEmitters]`
//    type9 hat emitters after their 0.5 s delay (Coonce's setup keeps them
//    off until then); the next face-off stops them.
// ATV_TOY_SHOTS=<dir> saves captures of the bursts and the hats.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_TOY_SHOTS || '';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name, timeout = 60000) =>
  page.waitForFunction(
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name,
    name,
    { timeout }
  );
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function choose(page, action) {
  const front = (await state(page)).front;
  const at = front.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${front.choices}`);
  for (let i = front.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = front.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
async function shot(page, name) {
  if (!shots) return;
  await page.screenshot({ path: `${shots}/${name}.png` });
}
// Polls the ride state (every frame or two) until `predicate` holds.
async function until(page, predicate, timeout = 60000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    // The ride, with the placed emitters' diagnostics (state.ambient).
    const s = await state(page);
    const ride = s.ride && { ...s.ride, ambient: s.ambient };
    if (ride && predicate(ride)) return ride;
    if (Date.now() > deadline) throw new Error(`timed out: ${predicate} ${JSON.stringify(ride?.ambient)}`);
    await nextFrames(page, 1);
  }
}
// The burst after a start: stream-pool sprites drawn additively, bounded.
async function burst(page, before, name) {
  const ride = await until(page, r => r.ambient?.triggered > before && r.ambient.live[2] > 0, 30000);
  const a = ride.ambient;
  assert.ok(a.live[2] <= 900, `stream pool bounded ${a.live}`);
  const drawn = await until(page, r => (r.ambient?.drawn?.[1] ?? 0) > 0, 5000);
  await shot(page, name);
  console.log(`  ${name}: ${a.live[2]} burst sprites, ${drawn.ambient.drawn[1]} drawn additive, ${a.triggered} starts`);
  // ParticleLife is 1 s and the one-shot lasts 0.1 s: the burst ends.
  await until(page, r => r.ambient.live[2] === 0, 10000);
  return a;
}

if (shots) await mkdir(shots, { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('toy-explosions-test')) return;
    sessionStorage.setItem('toy-explosions-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify({ store: { events: ['th'] } }));
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
  });

  // 1. Treasure Hunt on Prospect Springs.
  await page.goto(url('/play.html?uiDiagnostics=1&weather=off&gameTest=visit'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onPage(page, 'main');
  await page.waitForFunction(
    () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store?.atvs ?? 0) > 20,
    null,
    { timeout: 60000 }
  );
  await choose(page, 'single');
  await onPage(page, 'single');
  await choose(page, 'games');
  await onPage(page, 'games');
  const front = (await state(page)).front;
  assert.equal(front.labels[front.choices.indexOf('gametype')], 'Game: Treasure Hunt');
  assert.equal(front.labels[front.choices.indexOf('gamecourse')], 'Course: Prospect Springs');
  await choose(page, 'gamestart');
  let ride = await until(page, r => r.environment === 'env05' && r.game?.loaded, 300000);
  // The fountains (GPU) plus the bombs' burst pool.
  assert.ok(ride.ambient, 'placed emitters loaded');
  assert.ok(ride.ambient.capacity >= 1500, `capacity ${ride.ambient.capacity}`);
  assert.equal(ride.ambient.triggered, 0, 'no bomb before a collect');
  assert.equal(ride.ambient.live[2], 0);
  ride = await until(page, r => r.game.scores?.[0] >= 1, 60000);
  await burst(page, 0, 'treasure-bomb');
  ride = await until(page, r => r.game.scores?.[0] >= 2, 30000);
  const a = await burst(page, 1, 'treasure-bomb-2');
  assert.ok(a.triggered >= 2);

  // 2. A Freestyle green-target run: the cherry explosions.
  await page.goto(url('/play.html?uiDiagnostics=1&weather=off&objectiveTest=targets'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onPage(page, 'main');
  await choose(page, 'single');
  await onPage(page, 'single');
  await choose(page, 'championship');
  await onPage(page, 'championship');
  await choose(page, 'free:unspstunt');
  await onPage(page, 'objectives');
  await choose(page, 'obj:0');
  ride = await until(page, r => r.objective?.timeLeft != null, 300000);
  assert.equal(ride.objective.targets.length, 13);
  assert.ok(
    ride.objective.targets.every(t => t.particles?.length === 1 && t.particles[0].on),
    'each target starts one explosion'
  );
  const startsBefore = ride.ambient.triggered;
  assert.equal(startsBefore, 0, 'no target hit yet');
  await until(page, r => r.objective.hits?.some(h => h), 60000);
  await burst(page, 0, 'freestyle-cherry');

  // 3. Hockey: a hat trick throws the hats; the next face-off stops them.
  await page.goto(url('/play.html?uiDiagnostics=1&weather=off&hockeyGoal=3'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onPage(page, 'main');
  await choose(page, 'single');
  await onPage(page, 'single');
  await choose(page, 'games');
  await onPage(page, 'games');
  for (let i = 0; i < 4 && !(await state(page)).front.labels[0].startsWith('Game: Hockey'); i++) {
    await key(page, 'ArrowRight');
  }
  assert.equal((await state(page)).front.labels[0], 'Game: Hockey');
  await choose(page, 'gamestart');
  ride = await until(page, r => r.environment === 'game01' && r.hockey, 300000);
  assert.equal(ride.ambient.hatTrick, 4, 'four hat-trick emitters');
  ride = await until(page, r => r.hockey.scores[0] === 2, 120000);
  assert.equal(ride.ambient.emitted, 0, 'no hats before the hat trick');
  ride = await until(page, r => r.hockey.scores[0] === 3, 60000);
  assert.equal(ride.hockey.banner, 'Hat trick!');
  ride = await until(page, r => r.ambient.live[0] >= 4, 10000);
  assert.ok(ride.ambient.triggered >= 4);
  await until(page, r => (r.ambient.drawn?.[0] ?? 0) > 0, 5000);
  await shot(page, 'hockey-hats');
  await nextFrames(page, 60);
  await shot(page, 'hockey-hats-later');
  console.log(`  hats: ${ride.ambient.live[0]} in the air after the hat trick`);
  ride = await until(page, r => r.hockey.state.startsWith('FaceOff'), 20000);
  const thrown = ride.ambient.emitted;
  await nextFrames(page, 90);
  assert.equal((await state(page)).ambient.emitted, thrown, 'the face-off stops the hats');

  assertNoErrors(errors);
  console.log('Triggered TOY emitters: Treasure Hunt bombs, Freestyle cherries and Hockey hats');
} finally {
  await browser.close();
}
