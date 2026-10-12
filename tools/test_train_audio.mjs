// Actual Rust spatial commands and original train PCM through the browser.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 960, height: 600 } });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    const NativeContext = window.AudioContext || window.webkitAudioContext;
    window.AudioContext = class extends NativeContext {
      constructor(...args) { super(...args); window.trainAudioContext = this; window.trainAnalysers = []; }
      createChannelMerger(...args) {
        const merger = super.createChannelMerger(...args), analyser = this.createAnalyser(), silent = super.createGain();
        analyser.fftSize = 2048; silent.gain.value = 0; merger.connect(analyser); analyser.connect(silent); silent.connect(this.destination);
        window.trainAnalysers.push(analyser); return merger;
      }
    };
    const NativeWorker = Worker;
    window.Worker = class extends NativeWorker {
      constructor(...args) {
        super(...args); this.ops = new Map();
        this.addEventListener('message', ({ data }) => {
          const op = this.ops.get(data.id); this.ops.delete(data.id);
          if (op === 67 && data.bytes && !data.error) window.audioTrainStart = JSON.parse(new TextDecoder().decode(data.bytes));
        });
      }
      postMessage(data, ...args) {
        if (data.op === 23 && window.audioTrainStart) {
          const value = JSON.parse(new TextDecoder().decode(data.bytes)), p = window.audioTrainStart.wheel_matrices[0].slice(12,15);
          value.origin = [p[0]+35,p[1]+2,p[2]+20]; value.yaw = -Math.PI/2;
          data = { ...data, bytes: new TextEncoder().encode(JSON.stringify(value)) };
        }
        if (data.op === 73 && window.audioDistanceOverride) {
          const value = JSON.parse(new TextDecoder().decode(data.bytes));
          const p = value.cars[0].slice(12,15), distance = window.audioDistanceOverride;
          value.camera.position = [p[0]+distance,p[1],p[2]];
          value.camera.target = [p[0]+distance,p[1],p[2]+1]; value.previous_camera = value.camera;
          data = { ...data, bytes: new TextEncoder().encode(JSON.stringify(value)) };
        }
        this.ops.set(data.id,data.op); super.postMessage(data,...args);
      }
    };
    window.trainEnergy = () => {
      const samples = new Float32Array(2048);
      return window.trainAnalysers.map(analyser => {
        analyser.getFloatTimeDomainData(samples); return samples.reduce((sum,v)=>sum+v*v,0)/samples.length;
      });
    };
  });
  try {
    await page.goto(`${base}/play.html?drive=1&environment=env08`);
    await page.waitForFunction(()=>window.atvTerrainStatus?.ready || document.querySelector('#status.error'), null, {timeout:120000});
    assert((await page.evaluate(()=>window.atvTerrainStatus)).ready,await page.locator('#status').textContent());
    await page.locator('#game').click({position:{x:400,y:150}});
    await page.waitForFunction(()=>window.atvTerrainStatus.audio?.spatial.voices===11,null,{timeout:30000});
    const natural = await page.evaluate(()=>window.atvTerrainStatus.audio.spatial);
    assert.equal(natural.samples.length,3); assert(natural.samples.every(s=>s.decoded && s.channels===1 && s.loopStart!==null && s.loopEnd>s.loopStart));
    assert(natural.commands.some(command=>command.active && command.volume>0));
    await page.waitForFunction(()=>window.trainEnergy().some(energy=>energy>1e-7),null,{timeout:10000});
    const actualEnergy=await page.evaluate(()=>window.trainEnergy());
    const first = natural.commands[0];
    await page.waitForFunction(previous=>{const c=window.atvTerrainStatus.audio.spatial.commands[0];return c.distance!==previous.distance && c.pan!==previous.pan;},first,{timeout:30000});
    const moving=await page.evaluate(()=>window.atvTerrainStatus.audio.spatial);
    assert(moving.commands.some(command=>command.doppler!==0));
    await page.evaluate(()=>{window.audioDistanceOverride=30;});
    await page.waitForFunction(()=>{const c=window.atvTerrainStatus.audio.spatial.commands[0];return c?.volume===100 && c.distance===30 && c.pan===270;});
    const near=await page.evaluate(()=>window.atvTerrainStatus.audio.spatial);
    assert.equal(near.commands[0].left,100); assert.equal(near.commands[0].right,0);
    await page.evaluate(()=>{window.audioDistanceOverride=1000;});
    await page.waitForFunction(()=>window.atvTerrainStatus.audio.spatial.commands.every(c=>!c.active && c.left===0 && c.right===0));
    await page.waitForFunction(()=>window.trainEnergy().every(energy=>energy===0),null,{timeout:10000});
    const far=await page.evaluate(()=>window.atvTerrainStatus.audio.spatial);
    assert.equal(far.starts,11);
    await page.evaluate(()=>{window.audioDistanceOverride=30;});
    await page.waitForFunction(()=>{const c=window.atvTerrainStatus.audio.spatial.commands[0];return c?.volume===100 && c.distance===30 && c.pan===270;});
    await page.waitForFunction(()=>window.trainEnergy().some(energy=>energy>1e-7));
    assert.equal(await page.evaluate(()=>window.atvTerrainStatus.audio.spatial.starts),11);
    await page.keyboard.press('Escape'); await page.getByRole('button',{name:'Resume',exact:true}).waitFor();
    await page.waitForFunction(()=>window.atvTerrainStatus.audio.context==='suspended');
    const paused=await page.evaluate(()=>window.trainAudioContext.currentTime); await page.waitForTimeout(300);
    assert.equal(await page.evaluate(()=>window.trainAudioContext.currentTime),paused);
    await page.getByRole('button',{name:'Resume',exact:true}).click();
    await page.waitForFunction(time=>window.trainAudioContext.currentTime>time,paused);
    await page.evaluate(()=>{window.trainAudioLose=document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');window.trainAudioLose.loseContext();});
    await page.waitForFunction(()=>!window.atvTerrainStatus.ready && window.atvTerrainStatus.audio.context==='suspended');
    await page.evaluate(()=>window.trainAudioLose.restoreContext()); await page.waitForFunction(()=>window.atvTerrainStatus.ready && window.atvTerrainStatus.audio.context==='running');
    assert.equal(await page.evaluate(()=>window.atvTerrainStatus.audio.spatial.starts),11);
    assert.deepEqual(errors,[]);assert.equal(await page.evaluate(()=>window.atvTerrainStatus.audioError),undefined);
    if(process.env.ATV_TRAIN_AUDIO_SCREENSHOT)await page.screenshot({path:process.env.ATV_TRAIN_AUDIO_SCREENSHOT});
    console.log(JSON.stringify({natural,actualEnergy,moving:moving.commands.slice(0,3),near:near.commands[0],far:far.commands[0],starts:far.starts}));
    console.log('PASS: three original mono train samples/11 bounded attached loops produce nonzero PCM; recovered near/far stereo/pitch, body/camera motion, pause/resume and graphics restore pass with no duplicate voices. PS2 listening/mixer parity remains unverified.');
  } finally { await page.close(); }
} finally { await browser.close(); }
