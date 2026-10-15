// Original BodyImpact scheduling and PCM from a real touch-driven rider wreck.
// No collision, sound event, sample or physics state is injected.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { captureSimulation } from './capture_simulation.mjs';
import { assertNoErrors, base, button, launch, newPage } from './browser/harness.mjs';
await mkdir('work/verification', { recursive: true });
const browser = await launch();
let page;
try {
  const errors = [];
  page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv-mobile-one-handed', 'false');
    window.bodyAudioEvents = [];
    window.bodyAudioPeaks = [];
    const NativeContext = window.AudioContext || window.webkitAudioContext;
    window.AudioContext = class extends NativeContext {
      constructor(...args) {
        super(...args);
        window.bodyAudioContext = this;
      }
      createChannelMerger(...args) {
        const merger = super.createChannelMerger(...args),
          analyser = this.createAnalyser(),
          silent = super.createGain();
        analyser.fftSize = 2048;
        silent.gain.value = 0;
        merger.connect(analyser);
        analyser.connect(silent);
        silent.connect(this.destination);
        const record = { peak: 0, clock: this.currentTime };
        window.bodyAudioPeaks.push(record);
        const samples = new Float32Array(2048);
        const observe = () => {
          analyser.getFloatTimeDomainData(samples);
          record.peak = Math.max(record.peak, samples.reduce((sum, value) => sum + value * value, 0) / samples.length);
          if (this.state !== 'closed') requestAnimationFrame(observe);
        };
        requestAnimationFrame(observe);
        return merger;
      }
    };
    const post = Worker.prototype.postMessage,
      requests = new WeakMap();
    Worker.prototype.postMessage = function (message, ...rest) {
      if (!requests.has(this)) {
        const pending = new Map();
        requests.set(this, pending);
        this.addEventListener('message', ({ data }) => {
          const request = pending.get(data.id);
          pending.delete(data.id);
          if (!request || data.error || !data.bytes) return;
          const results = JSON.parse(new TextDecoder().decode(data.bytes));
          let playing = request.playing;
          request.frames.forEach((frame, i) => {
            frame.impact.playing = playing;
            window.bodyAudioEvents.push({ request: frame, result: results[i] });
            if (results[i].event?.play) playing = true;
          });
        });
      }
      if (message.op === 77) requests.get(this).set(message.id, JSON.parse(new TextDecoder().decode(message.bytes)));
      return post.call(this, message, ...rest);
    };
  });
  await captureSimulation(page, 'bodyFrames');
  const url = new URL('/play.html', base);
  url.searchParams.set('drive', '1');
  url.searchParams.set('environment', 'env01');
  await page.goto(url.href);
  await page.waitForFunction(() => window.atvTerrainStatus?.simulation, null, { timeout: 90000 });
  assert.equal(await page.evaluate(() => window.atvTerrainStatus.audio.effects.ready), true);
  await page.bringToFront();
  await page.locator('#game').focus();
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) =>
    cdp.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: points.map(point => ({ radiusX: 6, radiusY: 6, force: 1, ...point }))
    });
  const left = { id: 1, x: 60, y: 560 },
    right = { id: 2, x: 310, y: 530 },
    drive = { ...right, y: 460 };
  await touch('touchStart', [left, right]);
  await touch('touchMove', [left, drive]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.preload_armed, null, {
    timeout: 45000,
    polling: 'raf'
  });
  await touch('touchEnd', [drive]);
  await page.waitForFunction(() => window.bodyFrames.some(({ frame }) => frame.preload_active), null, {
    timeout: 5000,
    polling: 'raf'
  });
  await page.waitForFunction(() => !window.atvTerrainStatus.frame.grounded.some(Boolean), null, {
    timeout: 5000,
    polling: 'raf'
  });
  await touch('touchStart', [left, right]);
  await touch('touchMove', [left, { ...right, y: right.y - 70 }]);
  await page.waitForFunction(() => window.atvTerrainStatus.frame.rider_mode === 'wreck', null, {
    timeout: 15000,
    polling: 'raf'
  });
  const resetCount = await page.evaluate(() => window.atvTerrainStatus.frame.reset_count);
  await touch('touchCancel');
  await page.waitForFunction(() => window.atvTerrainStatus.audio.effects.starts > 0, null, {
    timeout: 15000,
    polling: 'raf'
  });
  await page.waitForFunction(() => window.bodyAudioPeaks.some(record => record.peak > 1e-7), null, {
    timeout: 10000,
    polling: 'raf'
  });
  if (process.env.ATV_BODY_AUDIO_SCREENSHOT) await page.screenshot({ path: process.env.ATV_BODY_AUDIO_SCREENSHOT });
  await page.waitForFunction(count => window.atvTerrainStatus.frame.reset_count > count, resetCount, {
    timeout: 15000,
    polling: 'raf'
  });
  const recovered = await page.evaluate(() => window.atvTerrainStatus.frame);
  assert.equal(recovered.wreck.phase, 'Riding');
  await page.waitForFunction(() => window.atvTerrainStatus.audio.effects.voices === 0, null, {
    timeout: 10000,
    polling: 'raf'
  });
  await page.keyboard.press('Escape');
  await button(page, 'Resume').waitFor();
  await page.waitForFunction(() => window.atvTerrainStatus.audio.context === 'suspended');
  const paused = await page.evaluate(() => window.bodyAudioContext.currentTime);
  // Time-based on purpose: proves the suspended audio clock does not advance while paused.
  await page.waitForTimeout(250);
  assert.equal(await page.evaluate(() => window.bodyAudioContext.currentTime), paused);
  await button(page, 'Resume').click();
  await page.waitForFunction(time => window.bodyAudioContext.currentTime > time, paused);
  const result = await page.evaluate(() => ({
    audio: window.atvTerrainStatus.audio,
    events: window.bodyAudioEvents,
    peaks: window.bodyAudioPeaks,
    frames: window.bodyFrames,
    glError: window.atvTerrainStatus.glError,
    audioError: window.atvTerrainStatus.audioError
  }));
  await writeFile(
    process.env.ATV_BODY_AUDIO_TRACE || 'work/verification/body-impact-browser-trace.json',
    JSON.stringify({ ...result, errors }, null, 2)
  );
  assert.equal(result.audio.spatial.sources, 0, 'Caribou analysers must isolate body impacts from train audio');
  assert.equal(result.audio.effects.samples.length, 4);
  assert(result.audio.effects.samples.every(sample => sample.decoded));
  assert(
    result.events.some(
      ({ request, result }) => Math.hypot(...request.impact.sampled_impact_delta) >= 0.3 && result.event?.play
    )
  );
  assert(
    result.events.every(({ request, result }) => !request.impact.playing || !result.event?.play),
    'Native play guard must prevent overlap'
  );
  const sounds = new Set(result.audio.effects.samples.map(sample => sample.name));
  assert(result.events.filter(event => event.result.event).every(event => sounds.has(event.result.event.sample)));
  assert.equal(result.peaks.length, result.audio.effects.starts);
  assert(result.peaks.some(record => record.peak > 1e-7));
  assert(result.frames.every(({ frame }) => frame.rider_audio_position?.length === 3));
  const detached = result.frames.filter(({ frame }) => frame.rider_wreck && frame.rider_wreck.phase !== 'attached');
  assert(detached.length > 0);
  assert(
    detached.every(({ frame }) =>
      frame.rider_audio_position.every((value, i) => value === detached[0].frame.rider_audio_position[i])
    ),
    'Original actor audio cache holds its last attached location during detached motion'
  );
  assert.equal(result.audio.effects.voices, 0);
  assert.equal(result.glError, 0);
  assert.equal(result.audioError, undefined);
  assertNoErrors(errors);
  console.log(
    JSON.stringify({
      samples: result.audio.effects.samples,
      starts: result.audio.effects.starts,
      selections: result.audio.effects.selections,
      peaks: result.peaks,
      triggers: result.events.filter(event => event.result.event)
    })
  );
  console.log(
    'PASS: real touch wreck produces original BodyImpact PCM from native sampled contact deltas, bounded nonoverlapping playback, cached rider position, recovery and pause/resume.'
  );
} finally {
  if (page && !page.isClosed()) {
    const trace = await page.evaluate(() => ({
      status: window.atvTerrainStatus,
      events: window.bodyAudioEvents,
      frames: window.bodyFrames,
      peaks: window.bodyAudioPeaks
    }));
    await writeFile('work/verification/body-impact-last-browser-state.json', JSON.stringify(trace, null, 2));
  }
  await browser.close();
}
