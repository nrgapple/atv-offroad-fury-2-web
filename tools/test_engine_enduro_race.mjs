// Original Enduro and Short Track races in the browser (research/RACING.md
// "Enduro"): Prospect Springs' short Enduro (E05_ENDS read over E05_NAT)
// races gate by gate from a grid past the finish gate, shows "GATE n of 13"
// and finishes a one-lap race when the player crosses the finish gate; Rockbridge
// Park's Short Track (E06_ST) loads its own TDF with National gate rules.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, optionalScreenshot, url } from './browser/harness.mjs';

const manifest = await (await fetch(url('/content/manifest.json'))).json();
if (!manifest.files.some(f => f.path === '/TRACKS/TRACKSA/ENV05/E05_ENDS.RES')) {
  console.log('SKIP: the content package has no Enduro race archives yet');
  process.exit(0);
}

async function race(browser, venue, course, frames) {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(f => localStorage.setItem('furyScript', JSON.stringify([{ frames: f, input: {} }])), frames);
  await page.goto(url(`/play.html?venue=${venue}&race=1&course=${course}&laps=1&cpu=3&seed=7&weather=off&script=storage&autopilot=1`));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail|missing/i.test(document.querySelector('#status')?.textContent), null, { timeout: 300000 });
  const text = await page.locator('#trace').textContent().catch(() => null);
  assert.ok(text, `trace for ${venue} ${course}: ${await page.locator('#status').textContent()}`);
  const trace = JSON.parse(text);
  return { page, errors, race: (Array.isArray(trace) ? trace.at(-1) : trace).race };
}

const browser = await launch();
try {
  // Mid race: gates count in order and the HUD names the next one.
  let { page, errors, race: r } = await race(browser, 'env05', 'enduro-short', 2400);
  assert.equal(r.phase, 'racing');
  assert.equal(r.enduro_gate?.[1], 13, `env05 short Enduro has 13 gates: ${JSON.stringify(r.enduro_gate)}`);
  assert.ok(r.enduro_gate[0] > 1, `the player crossed gates: ${JSON.stringify(r.enduro_gate)}`);
  assert.equal(r.finish_at, null, 'Enduro ignores the TDF finish box');
  assert.equal(r.off_track, false);
  await optionalScreenshot(page, 'ATV_ENDURO_SCREENSHOT', 'env05-enduro-short');
  assertNoErrors(errors);
  await page.close();
  // The finish gate completes the lap.
  ({ page, errors, race: r } = await race(browser, 'env05', 'enduro-short', 6500));
  const you = r.standings.find(s => s.player);
  assert.equal(r.phase, 'finished', JSON.stringify(r.standings));
  assert.equal(you.laps, 1);
  assert.ok(you.time > 60 && you.time < 150, `short Enduro lap ${you.time}s`);
  assertNoErrors(errors);
  await page.close();
  // Short Track: its own course with the National finish gate.
  ({ page, errors, race: r } = await race(browser, 'env06', 'short-track', 1200));
  assert.equal(r.enduro_gate ?? null, null);
  assert.ok(r.finish_at > 0, `Short Track finish gate ${r.finish_at}`);
  assertNoErrors(errors);
  console.log(`PASS: Rust engine Enduro: env05 short Enduro 13 gates, player lap ${you.time.toFixed(1)}s; env06 Short Track finish gate at ${Math.round(r.finish_at)}ft`);
} finally {
  await browser.close();
}
