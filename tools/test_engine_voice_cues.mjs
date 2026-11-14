// Disc-backed Training speech and race-load IntOut scheduling (/play.html).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const audio = page => page.evaluate(() => JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}'));
async function key(page, code) {
  await page.keyboard.down(code); await nextFrames(page, 2);
  await page.keyboard.up(code); await nextFrames(page, 3);
}
async function wait(page, predicate, arg, timeout = 180000) {
  try { await page.waitForFunction(predicate, arg, { timeout }); }
  catch (error) { console.error(JSON.stringify({ state: await read(page), audio: await audio(page) })); throw error; }
}
const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-soundtrack-v1', JSON.stringify({ v: 3, enabled: true, volume: 0.45, history: [], historyIndex: -1 }));
  });
  await page.goto(url('/play.html?uiDiagnostics=1&audioDiagnostics=1&weather=off'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title');
  await page.mouse.click(8, 8); await key(page, 'Enter');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.choices?.includes('training'));
  await wait(page, () => (JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.store?.atvs ?? 0) > 20);
  const front = (await read(page)).front;
  for (let i = front.selection; i < front.choices.indexOf('training'); i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await wait(page, () => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    const a = JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}');
    return s.ride?.trainingSpeech?.lines === 46 && a.speech?.last === 1 && a.speech?.playing;
  }, null, 30000);
  const first = await audio(page);
  // Restart in the first introduction line used to reuse serial 1, leaving
  // the first voice running instead of scheduling the restarted line.
  await key(page, 'Escape');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause');
  await key(page, 'ArrowDown'); await key(page, 'Enter');
  await wait(page, count => {
    const a = JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}');
    return a.speech?.started > count && a.speech.last === 1 && a.speech.playing;
  }, first.speech.started, 30000);
  await wait(page, () => {
    const s = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    const a = JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}');
    return s.ride?.training?.state === 'Demonstration' && a.speech?.last === 3 && a.speech.playing;
  });
  await page.waitForTimeout(700); await key(page, 'Enter');
  await wait(page, () => {
    const a = JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}');
    return a.speech?.last === 4 && a.speech.playing;
  }, null, 30000);
  assert.equal((await read(page)).ride.training.line, 4);
  await page.goto(url('/play.html?mode=single&activity=race&map=env01&laps=1&cpu=0&play=1&uiDiagnostics=1&audioDiagnostics=1'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').race?.phase === 'intro');
  await page.mouse.click(640, 430);
  await wait(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}').intOut === 1);
  assert.match((await audio(page)).intOutCues[0], /^intout\/int00[1-4]\.vag$/);
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').race?.phase === 'racing');
  await wait(page, () => JSON.parse(document.documentElement.getAttribute('data-engine-audio') || '{}').intOut === 0);
  assertNoErrors(errors);
  console.log('PASS: Training disc lines 1/3/4, restart interruption; disc race cue scheduled and cut at GO');
} finally { await browser.close(); }
