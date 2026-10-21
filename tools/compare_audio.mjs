// Audio parity for the shared Rust engine. Replays one scripted ride through
// the JavaScript game's own op 50 batches while tapping its Web Audio output
// (every connection to a destination also feeds a recorder) and logging each
// AudioBufferSourceNode start and playback rate, then renders the same script
// offline with `fury --audio` and compares 100 ms levels, the engine sample
// sequence and drive-on pitch on the simulation-step timeline.
//
//   node tools/compare_audio.mjs [venue] [script.json] [out-dir] [original|enhanced] [quad]
// Environment: ATV_TEST_URL (served site with private content), FURY_BIN,
// FURY_CONTENT. Outputs (WAV, JSON) contain game audio: keep them private.
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { assertNoErrors, launch, newPage, url, waitForGame } from './browser/harness.mjs';

const venue = process.argv[2] ?? 'env01';
const DEFAULT_SCRIPT = [
  { frames: 60, input: {} },
  { frames: 180, input: { keyboard_drive: { held: true, axis: 1 } } },
  { frames: 120, input: { steer: -1, keyboard_drive: { held: true, axis: 1 } } },
  { frames: 120, input: { steer: 1, keyboard_drive: { held: true, axis: 1 } } },
  { frames: 120, input: { keyboard_drive: { held: true, axis: -1 } } },
];
const script = process.argv[3] ? JSON.parse(await readFile(process.argv[3], 'utf8')) : DEFAULT_SCRIPT;
const out = path.resolve(process.argv[4] ?? 'private/reports/engine-parity/audio');
const mode = process.argv[5] ?? 'original';
// The Garage selection (27-46 are the motorcycles).
const quad = Number(process.argv[6] ?? 1);
await mkdir(out, { recursive: true });
const scriptFile = path.join(out, 'script.json');
await writeFile(scriptFile, JSON.stringify(script));
const normalized = input => input?.keyboard_drive ? { ...input, keyboard_drive: { generation: 0, ...input.keyboard_drive } } : input ?? {};
const steps = script.flatMap(({ frames, input }) => Array.from({ length: frames }, () => normalized(input)));

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
let js;
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await page.addInitScript(({ steps, mode }) => {
    localStorage.setItem('atv-graphics', 'original');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-audio-v1', JSON.stringify({ sfx: 1, mode, silent: 'play' }));
    // Sound effects only: the soundtrack streams separately and is checked on its own.
    localStorage.setItem('atv-soundtrack-v1', JSON.stringify({ v: 3, enabled: false, volume: 0.45 }));
    // Every context gets its own recorder; the ride's GameAudio context is
    // the one that starts buffer sources (menus and the music element use others).
    const taps = window.audioTaps = [], tapOf = context => taps.find(t => t.context === context);
    const Native = window.AudioContext;
    window.AudioContext = class extends Native {
      constructor(...args) {
        super(...args);
        const tap = { context: this, sampleRate: this.sampleRate, chunks: [], starts: [], steps: [], rates: [], live: new Set() };
        const recorder = this.createScriptProcessor(4096, 2, 2);
        recorder.onaudioprocess = ({ inputBuffer, playbackTime }) => {
          if (tap.starts.length) tap.chunks.push({ time: playbackTime, left: Array.from(inputBuffer.getChannelData(0)), right: Array.from(inputBuffer.getChannelData(1)) });
        };
        recorder.connect(this.destination);
        tap.recorder = recorder; taps.push(tap);
      }
    };
    const connect = AudioNode.prototype.connect;
    AudioNode.prototype.connect = function (target, ...rest) {
      const result = connect.call(this, target, ...rest);
      const tap = tapOf(this.context);
      if (target instanceof AudioDestinationNode && tap && this !== tap.recorder) connect.call(this, tap.recorder);
      return result;
    };
    const start = AudioBufferSourceNode.prototype.start;
    AudioBufferSourceNode.prototype.start = function (...args) {
      const tap = tapOf(this.context);
      if (tap && this.buffer) {
        tap.starts.push({ time: this.context.currentTime, seconds: this.buffer.duration, rate: this.playbackRate.value, loop: this.loop });
        tap.live.add(this); this.addEventListener('ended', () => tap.live.delete(this));
      }
      return start.apply(this, args);
    };
    const trace = window.rideTrace = { cursor: 0, held: false, frames: 0 };
    const post = Worker.prototype.postMessage, pending = new Set();
    Worker.prototype.postMessage = function (message, ...rest) {
      if (message?.op !== 50 || !message.bytes) return post.call(this, message, ...rest);
      if (!this.rideListener) {
        this.rideListener = true;
        this.addEventListener('message', ({ data }) => {
          if (!pending.delete(data.id) || data.error || !data.bytes) return;
          const count = JSON.parse(new TextDecoder().decode(data.bytes)).length;
          trace.frames += count;
          for (const tap of taps) {
            tap.steps.push({ step: trace.frames, time: tap.context.currentTime });
            tap.rates.push({ step: trace.frames, voices: [...tap.live].map(source => [source.buffer.duration, source.playbackRate.value]) });
          }
        });
      }
      if (trace.cursor >= steps.length) { trace.held = true; return; }
      const request = JSON.parse(new TextDecoder().decode(message.bytes));
      const same = JSON.stringify(steps[trace.cursor]);
      let count = 1;
      while (count < request.count && trace.cursor + count < steps.length && JSON.stringify(steps[trace.cursor + count]) === same) count++;
      const keep = Object.fromEntries(['funny_crashes', 'crash_mode', 'flips', 'arcade_flips', 'realistic_train'].map(key => [key, request.input[key]]));
      Object.assign(request, { count, orbit: 0, cycle: false,
        input: { throttle: 0, brake: 0, steer: 0, lean: 0, reverse: false, input_generation: 0, ...keep, ...steps[trace.cursor] } });
      trace.cursor += count; pending.add(message.id);
      return post.call(this, { ...message, bytes: new TextEncoder().encode(JSON.stringify(request)) }, ...rest);
    };
  }, { steps, mode });
  await page.goto(url(`/play.html?drive=1&environment=${venue}&quad=${quad}`));
  await waitForGame(page);
  await page.waitForFunction(() => window.rideTrace.held, null, { timeout: 180000 });
  await page.waitForTimeout(1500);
  js = await page.evaluate(() => {
    const tap = window.audioTaps.reduce((best, t) => (t.starts.length > (best?.starts.length ?? -1) ? t : best), null);
    const { chunks, starts, steps, rates, sampleRate } = tap;
    return { chunks, starts, steps, rates, sampleRate, state: window.atvTerrainStatus.audio ?? null };
  });
  assertNoErrors(errors);
} finally {
  await browser.close();
}

const content = process.env.FURY_CONTENT ? ['--content', process.env.FURY_CONTENT] : [];
const rustWav = path.join(out, `${venue}-${mode}-rust.wav`);
execFileSync(process.env.FURY_BIN, [...content, '--venue', venue, '--quad', String(quad), '--script', scriptFile, '--sound', mode, '--audio', rustWav], { stdio: 'inherit' });
const rust = JSON.parse(await readFile(rustWav.replace(/\.wav$/, '.json'), 'utf8'));

// JavaScript capture as one PCM timeline on its context clock.
const rate = js.sampleRate, first = js.chunks[0]?.time ?? 0;
const total = Math.round(((js.chunks.at(-1)?.time ?? 0) - first) * rate) + 4096;
const pcm = [new Float32Array(total), new Float32Array(total)];
for (const chunk of js.chunks) {
  const at = Math.round((chunk.time - first) * rate);
  pcm[0].set(chunk.left.slice(0, Math.max(0, total - at)), at); pcm[1].set(chunk.right.slice(0, Math.max(0, total - at)), at);
}
const wav = (channels, sampleRate) => {
  const frames = channels[0].length, bytes = Buffer.alloc(44 + frames * 4);
  bytes.write('RIFF', 0); bytes.writeUInt32LE(36 + frames * 4, 4); bytes.write('WAVEfmt ', 8); bytes.writeUInt32LE(16, 16);
  bytes.writeUInt16LE(1, 20); bytes.writeUInt16LE(2, 22); bytes.writeUInt32LE(sampleRate, 24); bytes.writeUInt32LE(sampleRate * 4, 28);
  bytes.writeUInt16LE(4, 32); bytes.writeUInt16LE(16, 34); bytes.write('data', 36); bytes.writeUInt32LE(frames * 4, 40);
  for (let i = 0; i < frames; i++) for (let c = 0; c < 2; c++) bytes.writeInt16LE(Math.round(Math.max(-1, Math.min(1, channels[c][i])) * 32767), 44 + i * 4 + c * 2);
  return bytes;
};
await writeFile(path.join(out, `${venue}-${mode}-javascript.wav`), wav(pcm, rate));
await writeFile(path.join(out, `${venue}-${mode}-javascript.json`), JSON.stringify({ starts: js.starts, steps: js.steps, rates: js.rates }));

// Step -> context time from each delivered batch (linear between batches).
const marks = js.steps.filter(mark => mark.time > 0);
const stepTime = step => {
  const after = marks.findIndex(mark => mark.step >= step);
  if (after <= 0) return (marks[0]?.time ?? 0) - ((marks[0]?.step ?? 0) - step) / 60;
  const a = marks[after - 1], b = marks[after];
  return a.time + (b.time - a.time) * (step - a.step) / Math.max(1, b.step - a.step);
};
const rms = (t0, t1) => {
  const i0 = Math.max(0, Math.round((t0 - first) * rate)), i1 = Math.min(total, Math.round((t1 - first) * rate));
  let sum = 0; for (let i = i0; i < i1; i++) sum += (pcm[0][i] ** 2 + pcm[1][i] ** 2) / 2;
  return i1 > i0 ? Math.sqrt(sum / (i1 - i0)) : 0;
};
const db = value => 20 * Math.log10(Math.max(value, 1e-5));
// The recorder taps the graph through a ScriptProcessorNode, which delays
// what it hears by its buffering; find that constant offset first.
const compare = lag => {
  const windows = [];
  for (let step = 0; step + 6 <= steps.length; step += 6) {
    const j = rms(stepTime(step) + lag, stepTime(step + 6) + lag);
    const levels = rust.levels.slice(step / 3, step / 3 + 2);
    const r = Math.sqrt(levels.reduce((sum, [l, rr]) => sum + (l * l + rr * rr) / 2, 0) / Math.max(1, levels.length));
    windows.push({ step, javascript: db(j), rust: db(r) });
  }
  const audible = windows.filter(w => w.javascript > -60 || w.rust > -60);
  const diffs = audible.map(w => Math.abs(w.javascript - w.rust)).sort((a, b) => a - b);
  return { windows, audible, median: diffs[Math.floor(diffs.length / 2)] ?? 0, p90: diffs[Math.floor(diffs.length * 0.9)] ?? 0,
    mean: diffs.reduce((a, b) => a + b, 0) / Math.max(1, diffs.length) };
};
let best = { lag: 0, ...compare(0) };
for (let lag = -0.4; lag <= 0.4001; lag += 0.005) { const c = compare(lag); if (c.mean < best.mean) best = { lag, ...c }; }
const { windows, audible, median, p90 } = best;
const mean = values => values.reduce((a, b) => a + b, 0) / Math.max(1, values.length);

// Engine samples by length on the step timeline (labels are Rust-side).
// decodeAudioData and the Rust resampler can round a length by one frame.
const engineLengths = rust.voices.filter(v => v.label === 'engine').map(v => v.seconds);
const isEngine = seconds => engineLengths.some(length => Math.abs(length - seconds) < 0.002);
const timeStep = time => { const after = marks.findIndex(mark => mark.time >= time); if (after <= 0) return marks[0]?.step ?? 0; const a = marks[after - 1], b = marks[after]; return a.step + (b.step - a.step) * (time - a.time) / Math.max(1e-9, b.time - a.time); };
const jsEngine = js.starts.filter(s => isEngine(s.seconds)).map(s => ({ step: Math.round(timeStep(s.time)), seconds: Number(s.seconds.toFixed(3)) }));
const rustEngine = rust.voices.filter(v => v.label === 'engine').map(v => ({ step: Math.round(v.time * 60), seconds: Number(v.seconds.toFixed(3)) }));
let sequence = 0;
for (let i = 0; i < Math.min(jsEngine.length, rustEngine.length); i++) if (Math.abs(jsEngine[i].seconds - rustEngine[i].seconds) < 0.002) sequence++; else break;
const offsets = rustEngine.slice(0, sequence).map((r, i) => jsEngine[i].step - r.step);

// Vocals, chassis, body impacts, drive-on and world voices: same clip
// (by length) starting within two steps.
const jsStarts = js.starts.map(s => ({ step: timeStep(s.time), seconds: s.seconds }));
const others = rust.voices.filter(v => v.label !== 'engine').map(v => {
  const step = v.time * 60;
  return { label: v.label, step: Math.round(step), matched: jsStarts.some(s => Math.abs(s.seconds - v.seconds) < 0.002 && Math.abs(s.step - step) <= 2.5) };
});
const byLabel = {};
for (const v of others) { const entry = byLabel[v.label] ??= { rust: 0, matched: 0 }; entry.rust++; entry.matched += Number(v.matched); }
// Drive-on pitch: the live drive voice's playback rate per step.
const driveLength = rust.voices.find(v => v.label === 'drive')?.seconds;
const pitch = [];
for (const sample of js.rates) {
  const voice = sample.voices.find(([seconds]) => driveLength && Math.abs(seconds - driveLength) < 0.002);
  const rustVoice = rust.live[sample.step - 1]?.find(([label]) => label === 'drive');
  if (voice && rustVoice) pitch.push(Math.abs(voice[1] - rustVoice[2]));
}
const report = {
  venue, mode, steps: steps.length, javascriptRate: rate,
  levels: { recorderLagMs: Math.round(best.lag * 1000), windows: windows.length, audible: audible.length, medianDb: median, p90Db: p90, meanAbsDb: best.mean,
    meanJavascriptDb: mean(audible.map(w => w.javascript)), meanRustDb: mean(audible.map(w => w.rust)) },
  engine: { javascript: jsEngine.length, rust: rustEngine.length, matchingPrefix: sequence, stepOffsets: offsets,
    javascriptSequence: jsEngine.map(e => e.seconds), rustSequence: rustEngine.map(e => e.seconds) },
  voices: byLabel,
  drivePitch: { samples: pitch.length, meanAbsoluteRateDifference: mean(pitch), max: Math.max(0, ...pitch) },
  rust: { bankMs: rust.bank_ms, logicMsPerFrame: rust.logic_ms_per_frame, mixMsPerFrame: rust.mix_ms_per_frame },
  windowsDb: windows,
};
await writeFile(path.join(out, `${venue}-${mode}-report.json`), JSON.stringify(report, null, 2));
const { windowsDb, ...summary } = report;
console.log(JSON.stringify(summary, null, 2));
console.log(`PASS: wrote ${venue} ${mode} audio parity to ${out}`);
