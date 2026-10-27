// The original Training Course (research/TRAINING.md): Main Menu → Training
// Course rides the six lessons on the Training venue. `trainingTest=fast`
// ends each spoken line (shown as a caption) at once and `trainingPass=1`
// passes lessons 2-6 as they start, so the first lesson is ridden for real:
// holding reverse for two seconds fails it ("Lesson Failed", the ATV back at
// its start), then driving to the far circle and stopping passes it. The
// ending grants the four Ravage Talons and the ATV2 gear to the loaded
// profile (0021f440): they are owned in the saved profile and shown Unlocked
// in Unlock Items. A second visit skips the prize line, Pause → Restart
// Training returns to the introduction, and Quit to Main Menu leaves
// mid-course. ATV_TRAINING_SHOTS=<dir> saves HUD captures.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_TRAINING_SHOTS || '';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const training = async page => (await state(page)).ride?.training;
const onPage = (page, name, timeout = 60000) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const until = (page, predicate, arg, timeout = 60000) => page.waitForFunction(([source, arg]) => {
  const t = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.training;
  return t && new Function('t', 'arg', `return (${source})(t, arg)`)(t, arg);
}, [predicate.toString(), arg], { timeout });
async function shot(page, name) {
  if (!shots) return;
  await nextFrames(page, 3);
  await page.screenshot({ path: `${shots}/${name}.png` });
}
async function choose(page, action) {
  const front = (await state(page)).front;
  const at = front.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${front.choices}`);
  for (let i = front.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = front.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
async function chooseLabel(page, label) {
  const front = (await state(page)).front;
  const at = front.labels.findIndex(l => l === label || l.startsWith(label));
  assert.ok(at >= 0, `${label} in ${front.labels}`);
  await choose(page, front.choices[at]);
}
async function toMain(page, query) {
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=off&${query}`));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await page.waitForFunction(() => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store?.atvs ?? 0) > 20, null, { timeout: 60000 });
}
async function startTraining(page) {
  const front = (await state(page)).front;
  const at = front.choices.indexOf('training');
  assert.ok(at > 0, `Training Course row in ${front.choices}`);
  assert.equal(front.labels[at], 'Training Course');
  assert.equal(front.choices[at + 1], 'profile', 'Training Course precedes the Profile Editor');
  await choose(page, 'training');
  await page.waitForFunction(() => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return s.ride?.environment === 'training' && s.ride?.training;
  }, null, { timeout: 300000 });
}
const near = (a, b, d) => Math.hypot(a[0] - b[0], a[2] - b[2]) < d;
const START = [130.0, 90.75, 646.54];

if (shots) await mkdir(shots, { recursive: true });
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    if (sessionStorage.getItem('training-test')) return;
    sessionStorage.setItem('training-test', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', '{}');
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 1, riderId: 0 }));
  });

  await toMain(page, 'trainingTest=fast&trainingPass=1');
  await startTraining(page);
  let t = await training(page);
  assert.equal(t.completedBefore, false, 'a new profile has not completed it');
  // The introduction, then lesson 1's demonstration; the ATV waits at the start.
  await until(page, t => t.phase === 'Lessons' && t.state === 'Active' && !t.locked, null, 120000);
  t = await training(page);
  assert.equal(t.name, 'Gas and Brake');
  assert.ok(near(t.origin, START, 3), `lesson 1 start ${t.origin}`);
  assert.ok(t.distance > 190 && t.distance < 230, `goal distance ${t.distance}`);
  await shot(page, 'lesson1-active');

  // Reversing two seconds fails it and the ATV returns to the start.
  await page.keyboard.down('ArrowDown');
  await until(page, t => t.failed >= 1, null, 20000);
  await page.keyboard.up('ArrowDown');
  t = await training(page);
  assert.equal(t.banner, 'Lesson Failed');
  assert.equal(t.lastLine, 46, 'the reverse failure line');
  await shot(page, 'lesson1-failed');
  await until(page, t => t.state === 'Active' && !t.locked, null, 30000);
  t = await training(page);
  assert.ok(near(t.origin, START, 3), `retried at the start ${t.origin}`);
  assert.equal(t.lesson, 0);

  // Ride to the far circle and stop in it.
  await page.keyboard.down('ArrowUp');
  await until(page, t => t.distance < 30, null, 60000);
  await page.keyboard.up('ArrowUp');
  await page.keyboard.down('ArrowDown');
  await until(page, t => t.passed >= 1, null, 30000);
  await page.keyboard.up('ArrowDown');
  t = await training(page);
  assert.equal(t.banner, 'Lesson Success');
  await shot(page, 'lesson1-success');

  // Lessons 2-6 pass as they start; the ending returns to the main menu.
  await until(page, t => t.lesson === 5, null, 120000);
  await onPage(page, 'main', 180000);
  let front = (await state(page)).front;
  assert.match(front.notice ?? '', /Training Course complete! Unlocked the Ravage Talons/);
  const saved = JSON.parse(await page.evaluate(() => localStorage.getItem('fury-profile')));
  assert.equal(saved.store.atvs.length, 4, `Talons ${saved.store.atvs}`);
  assert.deepEqual(saved.store.gear.helmet, [19, 20, 21, 22]);
  for (const slot of ['shirt', 'sleeve', 'pant']) assert.deepEqual(saved.store.gear[slot], [18, 19, 20, 21]);

  // Unlock Items shows the Talons unlocked.
  await choose(page, 'profile'); await onPage(page, 'profile');
  await chooseLabel(page, 'Unlock Items'); await onPage(page, 'store');
  await key(page, 'Enter'); await onPage(page, 'store-atv');
  for (let i = 0; i < 30 && !(await state(page)).front.labels[0].includes('Ravage Talon Blue'); i++) await key(page, 'ArrowRight');
  front = (await state(page)).front;
  assert.match(front.labels[0], /Unlocked: Ravage Talon Blue$/);
  await shot(page, 'store-talon');
  await key(page, 'Escape'); await onPage(page, 'store');
  await key(page, 'Escape'); await onPage(page, 'profile');
  await key(page, 'Escape'); await onPage(page, 'main');

  // A second visit: completed before; Restart Training and Quit mid-course.
  await toMain(page, 'trainingTest=fast');
  await startTraining(page);
  assert.equal((await training(page)).completedBefore, true);
  await until(page, t => t.state === 'Active' && !t.locked, null, 120000);
  await key(page, 'Escape');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const menu = (await state(page)).toolbar.menu;
  assert.equal(menu.choices[1], 'reset');
  assert.equal(menu.labels?.[1] ?? 'Restart Training', 'Restart Training');
  await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await until(page, t => t.lesson === 0 && t.passed === 0, null, 30000);
  await until(page, t => t.state === 'Active' && !t.locked, null, 120000);
  await key(page, 'Escape');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause', null, { timeout: 30000 });
  const choices = (await state(page)).toolbar.menu.choices;
  const quit = choices.indexOf('main');
  assert.ok(quit > 0, `Quit to Main Menu in ${choices}`);
  for (let i = 0; i < quit; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onPage(page, 'main', 60000);
  assert.equal((await state(page)).ride?.training ?? null, null);
  assertNoErrors(errors);
  console.log('Training Course: lesson 1 failed and passed, all six lessons, Talons unlocked, restart and quit');
} finally {
  await browser.close();
}
