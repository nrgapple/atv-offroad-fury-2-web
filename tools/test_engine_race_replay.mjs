// Full-race replay (#542, a port enhancement): a race keeps the whole of
// it, player and CPU riders, from the first drawn frame to a little after
// the finish. The race path driver (`autopilot=1`) rides a one-lap sx01
// race with three CPUs; at the results Replay opens the race at its start
// (the grid, before GO) with the CPUs drawn, plays it, and the scrub bar and
// End reach the finish line. It opens on the Broadcast camera: the
// original's after-finish director cutting between the course's TV cameras.
// The recording's storage is reported for the memory budget. REPLAY_SHOTS=dir saves the start and the finish.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const wait = (page, predicate, arg, timeout = 30000) => page.waitForFunction(predicate, arg, { timeout, polling: 'raf' });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const hit = async (page, id) => (await read(page)).toolbar.hits.find(h => h.id === id);
const distance = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);
const shots = process.env.REPLAY_SHOTS;
const shoot = async (page, name) => { if (shots) await page.screenshot({ path: `${shots}/${name}.png` }); };

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?venue=sx01&race=1&laps=1&cpu=3&seed=11&weather=off&autopilot=1&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await page.mouse.click(640, 400);
  // GO: the step and the player's grid place.
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).ride?.raceLap?.phase === 'racing', null, 120000);
  const go = await read(page);
  // The finish: its step and where the player crossed.
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).ride?.raceLap?.phase === 'finished', null, 300000);
  const finish = await read(page);
  await wait(page, () => (JSON.parse(document.querySelector('#ui-state').textContent).ride?.results?.length ?? 0) > 0, null, 60000);
  if ((await read(page)).ride.results[0] === 36) {
    // A broken record's name entry first.
    await key(page, 'Enter');
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).ride?.results?.[0] === 30);
  }
  assert.ok((await read(page)).ride.results.includes(37), `results offer Replay: ${(await read(page)).ride.results}`);
  const replay = await hit(page, 37);
  await page.mouse.click(replay.x + replay.width / 2, replay.y + replay.height / 2);
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).replay).catch(async e => {
    const s = await read(page);
    console.log('no replay', JSON.stringify({ results: s.ride?.results, hits: s.toolbar?.hits, replay, paused: s.paused, menu: s.toolbar?.menu?.page }));
    throw e;
  });
  let s = await read(page);
  const [first, last] = s.replay.span;
  const memory = `${s.replay.samples} samples every ${s.replay.interval} steps, ${s.replay.stride} matrices each, ${(s.replay.bytes / 1048576).toFixed(1)} MiB`;
  console.log(`recording ${first}..${last} (GO ${go.step}, finish ${finish.step}): ${memory}`);
  assert.equal(s.replay.whole, true);
  assert.equal(s.replay.fromResults, true);
  assert.equal(s.replay.shot, 'Broadcast');
  assert.equal(s.replay.authored, true, 'the course TV cameras direct the broadcast');
  // From the race start: the grid, before GO, with the CPUs.
  assert.ok(first <= go.step - 60, `recording starts before GO (${first} vs ${go.step})`);
  assert.ok(s.replay.time - first < 10, `opens at the start (${s.replay.time})`);
  assert.ok(distance(s.replay.origin, go.origin) < 2, `starts on the grid (${s.replay.origin} vs ${go.origin})`);
  assert.equal(s.replay.cpus, 3, 'CPU riders replayed');
  // Through the finish, stopping a little after race over.
  assert.ok(last >= finish.step && last <= finish.step + 4 * 60, `recording ends after the finish (${last} vs ${finish.step})`);
  assert.ok(s.replay.bytes < 64 * 1048576, memory);
  await nextFrames(page, 20);
  await shoot(page, 'start');
  s = await read(page);
  assert.ok(s.replay.playing && s.replay.time > first + 5, `plays from the start (${s.replay.time})`);
  assert.ok(s.replay.cuts >= 1 && (s.replay.setup === 'Onboard' || s.replay.tvCamera === 1), `the director opens on Camera1 (${s.replay.setup} ${s.replay.tvCamera})`);
  // Mid-race the director cuts as the rider passes the TV cameras.
  {
    const bar = await hit(page, 60);
    await page.mouse.click(bar.x + bar.width * 0.3, bar.y + bar.height / 2);
    await nextFrames(page, 3);
    const seen = new Set();
    let tvEye = null;
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).replay?.cuts >= 3, null, 45000).catch(async e => {
      console.log('broadcast', JSON.stringify((await read(page)).replay));
      throw e;
    });
    s = await read(page);
    seen.add(`${s.replay.setup}${s.replay.tvCamera ?? ''}`);
    // Most of sx01's 13 cameras are TV views (the rest roll follow presets,
    // shown as the recorded ride camera): one shows the rider from its
    // authored trackside position.
    await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).replay?.setup === 'TV', null, 40000);
    s = await read(page);
    seen.add(`${s.replay.setup}${s.replay.tvCamera}`);
    tvEye = s.view.eye;
    assert.ok(distance(tvEye, s.replay.origin) > 5, `the TV camera stands off the rider (${tvEye} vs ${s.replay.origin})`);
    console.log(`broadcast: ${s.replay.cuts} cuts after the scrub, seen ${[...seen]}, TV eye ${tvEye.map(v => v.toFixed(0))}`);
    await shoot(page, 'broadcast');
  }
  // The scrub bar to the finish line: the player crossing it.
  await key(page, 'Space');
  const bar = await hit(page, 60);
  const fraction = (finish.step - first) / (last - first);
  await page.mouse.click(bar.x + bar.width * fraction, bar.y + bar.height / 2);
  await nextFrames(page, 3);
  s = await read(page);
  assert.ok(Math.abs(s.replay.time - finish.step) < 30, `scrubbed to the finish (${s.replay.time} vs ${finish.step})`);
  assert.ok(distance(s.replay.origin, finish.origin) < 8, `at the finish line (${s.replay.origin} vs ${finish.origin})`);
  // Broadcast → Chase → Trackside.
  await key(page, 'KeyC');
  await key(page, 'KeyC');
  assert.equal((await read(page)).replay.shot, 'Trackside');
  await shoot(page, 'finish-trackside');
  // End reaches the end; Back returns to the results.
  await key(page, 'End');
  assert.equal((await read(page)).replay.time, last);
  await key(page, 'Escape');
  await wait(page, () => { const s = JSON.parse(document.querySelector('#ui-state').textContent); return !s.replay && s.ride?.results?.includes(37); });
  assertNoErrors(errors);
  console.log(`PASS: full-race replay from the results: grid start, CPUs, TV broadcast cuts, scrub to the finish, End and Back (${memory})`);
} finally {
  await browser.close();
}
