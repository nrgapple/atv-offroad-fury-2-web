// Settings > Audio > Sound: Enhanced (default) crossfades engine samples and
// feeds one shared echo send per venue type; Original bypasses both.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, holdKey, launch, newPage, waitForGame, waitForScreen } from './browser/harness.mjs';

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
  const samples = new Float32Array(2048);
  setInterval(() => {
    for (const p of window.voiceProbes) { p.analyser.getFloatTimeDomainData(samples); p.peak = Math.max(p.peak, samples.reduce((s, v) => s + v * v, 0) / samples.length); }
  }, 20);
  window.voiceEnergy = label => window.voiceProbes.filter(p => p.merger.atvVoice === label).reduce((m, p) => Math.max(m, p.peak), 0);
};
const audio = page => page.evaluate(() => window.atvTerrainStatus.audio);
async function open(environment) {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 600 } }, errors);
  await page.addInitScript(probe);
  await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
  await waitForGame(page);
  await page.locator('#game').click({ position: { x: 400, y: 150 } });
  await page.waitForFunction(() => window.atvTerrainStatus.audio?.quad?.venue && window.atvTerrainStatus.audio.echoType !== undefined, null, { timeout: 60000 });
  return { page, errors };
}
try {
  {
    const { page, errors } = await open('sx01');
    await holdKey(page, 'KeyW', 300);
    const a = await audio(page);
    assert.equal(a.soundMode, 'enhanced');
    assert.deepEqual(a.echo, { type: 'stadium', nodes: 8 });
    assert(a.crossfades > 0, `engine crossfades ${a.crossfades}`);
    assert(await page.evaluate(() => window.voiceEnergy('echo')) > 1e-9, 'echo send output');
    console.log('stadium', JSON.stringify({ echo: a.echo, crossfades: a.crossfades, commands: a.commands }));
    assertNoErrors(errors);
    await page.close();
  }
  {
    const { page, errors } = await open('nat03');
    const a = await audio(page);
    assert.equal(a.echo?.type, 'canyon', `nat03 enclosure ${a.enclosure}`);
    assertNoErrors(errors);
    await page.close();
  }
  {
    const { page, errors } = await open('env04');
    assert.equal((await audio(page)).echo, null, 'open venues have no echo bus');
    // Switch to Original through the menu; it persists and stops crossfading.
    await page.keyboard.press('Escape');
    await waitForScreen(page, 'pause');
    const activate = async locator => { await locator.focus(); await page.keyboard.press('Enter'); };
    await activate(button(page, 'Settings'));
    await waitForScreen(page, 'settings');
    await activate(button(page, 'Audio'));
    await waitForScreen(page, 'music');
    await activate(page.getByRole('button', { name: 'Sound: Enhanced' }));
    await page.getByRole('button', { name: 'Sound: Original' }).waitFor();
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('atv-audio-v1')).mode), 'original');
    await activate(button(page, 'Back'));
    await waitForScreen(page, 'settings');
    await activate(button(page, 'Back'));
    await waitForScreen(page, 'pause');
    await activate(button(page, 'Resume'));
    await page.waitForFunction(() => document.querySelector('#frontend').hidden);
    await page.locator('#game').focus();
    const before = await audio(page);
    await holdKey(page, 'KeyW', 300);
    const after = await audio(page);
    assert.equal(after.soundMode, 'original');
    assert(after.commands > before.commands, 'engine transitions continue');
    assert.equal(after.crossfades, before.crossfades, 'Original cuts between engine samples');
    assertNoErrors(errors);
    await page.close();
  }
  console.log('PASS: Enhanced engine crossfades and venue echo; Original bypasses them.');
} finally {
  await browser.close();
}
