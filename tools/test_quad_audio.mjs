// Recovered rider vocals, chassis impacts, terrain drive-on and venue world
// sounds through the actual Rust/WASM decisions and original PCM. Output
// energy is measured on each labelled voice's own merger node.
import assert from 'node:assert/strict';
import { assertNoErrors, base, holdKey, launch, newPage, waitForGame } from './browser/harness.mjs';

const browser = await launch();
const probe = () => {
  const NativeContext = window.AudioContext || window.webkitAudioContext;
  window.voiceProbes = [];
  window.AudioContext = class extends NativeContext {
    createChannelMerger(...args) {
      const merger = super.createChannelMerger(...args), analyser = this.createAnalyser(), silent = super.createGain();
      analyser.fftSize = 2048; silent.gain.value = 0;
      merger.connect(analyser); analyser.connect(silent); silent.connect(this.destination);
      window.voiceProbes.push({ merger, analyser, peak: 0 });
      return merger;
    }
  };
  // Peak energy per labelled voice, sampled continuously.
  const samples = new Float32Array(2048);
  setInterval(() => {
    for (const probe of window.voiceProbes) {
      probe.analyser.getFloatTimeDomainData(samples);
      probe.peak = Math.max(probe.peak, samples.reduce((sum, v) => sum + v * v, 0) / samples.length);
    }
  }, 20);
  window.voiceEnergy = label => window.voiceProbes.filter(p => p.merger.atvVoice === label).reduce((m, p) => Math.max(m, p.peak), 0);
  // Optional start override: drop height or an off-map (boundary) start.
  const NativeWorker = Worker;
  window.Worker = class extends NativeWorker {
    postMessage(data, ...args) {
      if (data.op === 23 && window.startOverride) {
        const value = JSON.parse(new TextDecoder().decode(data.bytes));
        window.startOverride(value);
        data = { ...data, bytes: new TextEncoder().encode(JSON.stringify(value)) };
      }
      super.postMessage(data, ...args);
    }
  };
};
const quad = page => page.evaluate(() => window.atvTerrainStatus.audio?.quad);
async function open(environment, override) {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 600 } }, errors);
  await page.addInitScript(probe);
  if (override) await page.addInitScript(override);
  await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
  await waitForGame(page);
  await page.locator('#game').click({ position: { x: 400, y: 150 } });
  await page.waitForFunction(() => window.atvTerrainStatus.audio?.quad?.ready && window.atvTerrainStatus.audio.quad.venue, null, { timeout: 60000 });
  return { page, errors };
}
try {
  // 1. A 70ft drop lands (impact\landing.vag, volume from the impact); a
  // 200ft drop wrecks on impact with a HardWreck vocal.
  {
    const { page, errors } = await open('env04', () => { window.startOverride = value => { value.origin[1] += 70; }; });
    const initial = await quad(page);
    assert(initial.vocalSamples === 51, `selected male bank ${initial.vocalSamples}`);
    await page.waitForFunction(() => window.atvTerrainStatus.audio.quad.chassis > 0, null, { timeout: 30000 });
    await page.waitForTimeout(150);
    const after = await quad(page);
    assert.equal(after.lastChassis.sample, 'impact/landing.vag');
    assert(after.lastChassis.volume > 50 && after.lastChassis.volume <= 100, `landing volume ${after.lastChassis.volume}`);
    const energy = await page.evaluate(() => window.voiceEnergy('chassis'));
    assert(energy > 1e-7, `landing output ${energy}`);
    console.log('landing', JSON.stringify({ cue: after.lastChassis, energy }));
    assertNoErrors(errors);
    await page.close();
  }
  {
    const { page, errors } = await open('env04', () => { window.startOverride = value => { value.origin[1] += 200; }; });
    await page.waitForFunction(() => window.atvTerrainStatus.audio.quad.vocals > 0, null, { timeout: 30000 });
    await page.waitForTimeout(300);
    const after = await quad(page);
    assert.match(after.history[0], /^rider\/mhurt\/mhurt(1[0-9]|20)\.vag$/, `hard wreck vocal ${after.history}`);
    assert(after.decodedVocals >= 1 && after.decodedVocals <= 24);
    const energy = await page.evaluate(() => window.voiceEnergy('vocal'));
    assert(energy > 1e-7, `vocal output ${energy}`);
    console.log('wreck', JSON.stringify({ history: after.history, decoded: after.decodedVocals, energy }));
    assertNoErrors(errors);
    await page.close();
  }
  // 2. An off-map start: the boundary launch wreck plays the Fall yell.
  {
    const { page, errors } = await open('env04', () => { window.startOverride = value => { value.origin[0] = 40; value.origin[2] = 40; value.origin[1] += 10; }; });
    await page.waitForFunction(() => window.atvTerrainStatus.audio.quad.vocals > 0, null, { timeout: 30000 });
    const after = await quad(page);
    assert.match(after.lastVocal, /^rider\/myell\/myell0[1-6]\.vag$/, `fall vocal ${after.lastVocal}`);
    console.log('boundary', after.lastVocal);
    assertNoErrors(errors);
    await page.close();
  }
  // 3. Snow terrain drive-on loops.
  {
    const { page, errors } = await open('env01');
    await holdKey(page, 'KeyW', 240);
    const after = await quad(page);
    assert(after.drive > 0 && after.driveSamples === 9, `drive-on ${JSON.stringify(after)}`);
    assert.match(after.lastDrive.sample, /^impact\/driveon\/drv_/);
    console.log('drive', JSON.stringify(after.lastDrive));
    assertNoErrors(errors);
    await page.close();
  }
  // 4. Coonce arena indoor crowd (2D AmbientSound) and stadium Soultree loops.
  {
    const { page, errors } = await open('game01');
    await page.waitForFunction(() => window.atvTerrainStatus.audio.quad.voices.ambient >= 1, null, { timeout: 30000 });
    await page.waitForTimeout(500);
    const after = await quad(page);
    assert(after.ambientSources === 2 && await page.evaluate(() => window.voiceEnergy('ambient')) > 1e-7, 'indoor crowd output');
    assertNoErrors(errors);
    await page.close();
  }
  {
    const { page, errors } = await open('sx01');
    await page.waitForFunction(() => window.atvTerrainStatus.audio.quad.voices.soultree >= 1, null, { timeout: 30000 });
    await page.waitForTimeout(500);
    const after = await quad(page);
    assert(after.soultreeSources === 8 && after.voices.soultree <= 12);
    assert(await page.evaluate(() => window.voiceEnergy('soultree')) > 1e-7, 'stadium crowd loop output');
    assertNoErrors(errors);
    await page.close();
  }
  console.log('PASS: recovered vocals, chassis, drive-on and world sounds produce bounded original PCM.');
} finally {
  await browser.close();
}
