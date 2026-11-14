// Menu preload of the ride's sound bank and the race CPU vehicles (issue #515,
// following #453's course preload in test_engine_menu_preload.mjs).
//
// While the race setup rests on a course, the preload also decodes the ride's
// sound bank and every quad the race's CPU roster can draw, into the decoded
// cache. Fresh browser contexts, alternating:
//   without  `?preload=0`: Start loads the CPU quads and the bank cold;
//   with     the race setup settles and prepares them; Start and the sound
//            bank that follows the first frame miss the cache nowhere.
// Proof that they come from the preload: the preload reports the bank's
// samples and the CPU quads' textures it stored, and from Start until the
// ride's sounds are ready the decoded cache records no miss (the run
// without a preload records the bank's and the CPU textures' misses).
// Also: Select Track (Free Ride) prepares the bank as well, and the sound
// bank is ready sooner after Start.
// Works against production:
//   node tools/run_browser_tests.mjs --only test_engine_menu_preload_ride.mjs --url https://furyremastered.com
// MENU_PRELOAD_RIDE_SHOT=out.png saves the race once its sounds are ready.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, unlockStore, url } from './browser/harness.mjs';

const ROUNDS = 2;
const MEASURE_ONLY = process.env.MENU_PRELOAD_RIDE_MEASURE === '1';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const preload = page => page.evaluate(() => JSON.parse(window.atvPreload || '{}'));

async function until(page, check, what, timeout = 120000) {
  const end = Date.now() + timeout;
  for (;;) {
    const report = await preload(page);
    if (check(report)) return report;
    assert.ok(Date.now() < end, `${what}: ${JSON.stringify(report)}`);
    await page.waitForTimeout(25);
  }
}

async function context(browser, errors) {
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await unlockStore(page);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  page.sounds = [];
  page.on('console', message => {
    const text = message.text();
    if (text.startsWith('Sounds ready')) page.sounds.push({ at: Date.now(), text });
  });
  return page;
}

/** One Start from `front` (race or tracks), with or without the preload. */
async function ride(browser, errors, front, withPreload, shot, selection) {
  const page = await context(browser, errors);
  if (selection) await page.addInitScript(s => localStorage.setItem('atv-player-selection', JSON.stringify(s)), selection);
  await page.goto(url(`/play.html?uiDiagnostics=1&weather=${selection ? 'on' : 'off'}&front=${front}${withPreload ? '' : '&preload=0'}`));
  await page.waitForFunction(p => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === p, front, { timeout: 120000 });
  if (front === 'race') {
    // The cursor goes to Start Race (moving rows keeps the course).
    for (let i = 0; i < 16; i++) {
      const f = (await state(page)).front;
      if (f.choices[f.selection] === 'racestart') break;
      await page.keyboard.press('ArrowDown');
      await page.waitForFunction(at => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.selection !== at, f.selection, { timeout: 10000, polling: 16 });
    }
    const f = (await state(page)).front;
    assert.equal(f.choices[f.selection], 'racestart', JSON.stringify(f.choices));
  }
  let report = {};
  if (withPreload) {
    report = await until(page, r => r.state === 'done' || r.state === 'skipped' || r.state === 'failed', 'preload ends');
    assert.equal(report.state, 'done', `preload finished: ${JSON.stringify(report)}`);
    if (selection) {
      assert.equal((await state(page)).front.quad, selection.quadId);
      assert.equal(report.course, selection.race?.course ?? selection.freestyleEnvironment);
    }
  } else {
    await page.waitForTimeout(1500); // Deliberately wall-clock: the menu art settles as in the preload run.
  }
  await page.waitForFunction(() => {
    // Live counters: an idle menu draws no frames, so #ui-state's copy goes stale.
    const c = JSON.parse(window.atvDecodedCache || 'null');
    return !c || c.pending === 0;
  }, null, { timeout: 60000, polling: 100 });
  const cache = page => page.evaluate(() => JSON.parse(window.atvDecodedCache || 'null'));
  const before = (await cache(page)) ?? { hits: 0, misses: 0 };
  const pressed = Date.now();
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.environment, null, { timeout: 180000, polling: 16 });
  const live = Date.now();
  const loaded = await state(page);
  for (const end = Date.now() + 120000; !page.sounds.length;) {
    assert.ok(Date.now() < end, 'the ride\'s sounds load');
    await page.waitForTimeout(16);
  }
  // Sounds ready is logged before LAST_SOUND_MS is stored and the next
  // diagnostic frame publishes it; wait rather than recording stale zeroes.
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').decodedCache?.soundMs > 0,
    null, { timeout: 10000, polling: 16 });
  const after = { ...(await state(page)).decodedCache, ...(await cache(page)) };
  if (shot) await page.screenshot({ path: shot });
  const sounds = page.sounds[0];
  await page.context().close();
  return {
    racing: Boolean(loaded.ride?.raceLap),
    cpus: (loaded.ride?.raceLap?.field ?? 1) - 1,
    elapsed: live - pressed,
    loadMs: loaded.decodedCache.loadMs,
    soundMs: after.soundMs ?? (sounds ? sounds.at - live : null),
    sound: sounds?.text,
    hits: after.hits - before.hits,
    misses: after.misses - before.misses,
    report,
  };
}

const best = runs => runs.reduce((a, b) => (b.loadMs < a.loadMs ? b : a));
const median = values => values.slice().sort((a, b) => a - b)[Math.floor(values.length / 2)];

const browser = await launch();
try {
  const errors = [];
  if (process.env.MENU_PRELOAD_WORLD !== '0') {
    for (const selection of [
      { quadId: 29, freestyleEnvironment: 'env01' },
      { quadId: 1, freestyleEnvironment: 'env08' },
      { quadId: 1, freestyleEnvironment: 'sx01', race: { course: 'sx01' } },
      { quadId: 1, freestyleEnvironment: 'env03' },
    ]) {
      const runs = [];
      for (const warm of [false, true]) {
        const run = await ride(browser, errors, selection.race ? 'race' : 'tracks', warm, warm && process.env.MENU_PRELOAD_RIDE_SHOT?.replace(/\.png$/, `-${selection.freestyleEnvironment}.png`), selection);
        runs.push(run);
        console.log(`world ${JSON.stringify(selection)} warm=${warm}: ${JSON.stringify(run)}`);
        if (!MEASURE_ONLY && warm) {
          assert.ok(run.report.bankSamples > 0);
          assert.ok(run.report.venueSounds.includes('rain.vag'), 'weather loop is prepared');
          if (selection.freestyleEnvironment === 'env08') assert.ok(run.report.venueSounds.some(n => /train|loco/i.test(n)), JSON.stringify(run.report));
          if (selection.freestyleEnvironment === 'sx01') assert.ok(run.report.venueSounds.some(n => /crowd/i.test(n)), JSON.stringify(run.report));
          if (selection.freestyleEnvironment === 'env03') assert.ok(run.report.venueSounds.some(n => /plane|blimp|heli/i.test(n)), JSON.stringify(run.report));
          assert.equal(run.misses, 0, 'all motorcycle and venue decodes hit after Start');
        }
      }
    }
    assertNoErrors(errors);
    console.log('PASS: motorcycle and venue sounds preload (host only)');
  }
  if (process.env.MENU_PRELOAD_WORLD !== '1') {
  const without = [], withPreload = [];
  for (let round = 0; round < ROUNDS; round++) {
    without.push(await ride(browser, errors, 'race', false));
    withPreload.push(await ride(browser, errors, 'race', true, round === 0 && process.env.MENU_PRELOAD_RIDE_SHOT));
  }
  const free = await ride(browser, errors, 'tracks', true);
  const summary = runs => runs.map(r => `${r.loadMs} ms load, ${r.elapsed} ms wall, sounds +${r.soundMs} ms (${r.sound}), ${r.hits} hits/${r.misses} misses`).join('; ');
  console.log(`race without a preload: ${summary(without)}`);
  console.log(`race after a preload: ${summary(withPreload)}`);
  console.log(`free ride after a preload: ${summary([free])}`);
  console.log(`preload reports: ${withPreload.map(r => JSON.stringify(r.report)).join(' ')} ${JSON.stringify(free.report)}`);
  if (!MEASURE_ONLY) {
    for (const run of [...without, ...withPreload]) {
      assert.ok(run.racing && run.cpus > 0, `Start began a race with CPUs: ${JSON.stringify(run)}`);
    }
    for (const run of without) assert.ok(run.misses > 0, `a cold Start misses the cache: ${JSON.stringify(run)}`);
    for (const run of withPreload) {
      const r = run.report;
      assert.ok(r.bankSamples > 0, `the preload decoded the sound bank: ${JSON.stringify(r)}`);
      assert.ok(r.cpuQuads > 0 && r.cpuTextures > 0, `the preload prepared the CPU quads: ${JSON.stringify(r)}`);
      // Everything Start and the sound bank read from the cache was there.
      assert.equal(run.misses, 0, `Start after a preload missed the cache: ${JSON.stringify(run)}`);
      assert.ok(run.hits > 0, `the bank came from the cache: ${JSON.stringify(run)}`);
    }
    assert.ok(free.report.bankSamples > 0, `Select Track prepares the bank too: ${JSON.stringify(free.report)}`);
    assert.equal(free.misses, 0, `a Free Ride after a preload missed the cache: ${JSON.stringify(free)}`);
    const [slow, fast] = [best(without), best(withPreload)];
    assert.ok(fast.loadMs < slow.loadMs * 0.9, `race Start after a preload (${fast.loadMs} ms) is not measurably faster than without (${slow.loadMs} ms)`);
    const [coldSound, warmSound] = [median(without.map(r => r.soundMs)), median(withPreload.map(r => r.soundMs))];
    assert.ok(warmSound < coldSound, `sounds after a preload (${warmSound} ms) are not ready sooner than without (${coldSound} ms)`);
  }
  assertNoErrors(errors);
  console.log(`PASS: menu preload of the race's sound bank and CPU quads: race load ${without.map(r => r.loadMs).join('/')} ms cold, ${withPreload.map(r => r.loadMs).join('/')} ms after a preload; sounds ready ${without.map(r => r.soundMs).join('/')} ms after the first frame cold, ${withPreload.map(r => r.soundMs).join('/')} ms after a preload. Host measurements.`);
  }
} finally {
  await browser.close();
}
