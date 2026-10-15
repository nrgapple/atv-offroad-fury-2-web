// Opt-in browser check against actual catalog + Rust decoder + hosted content.
// No music payloads, disc contents or screenshots are written to the repository.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage } from './browser/harness.mjs';
const browser = await launch();
const errors = [];
try {
  const page = await newPage(browser, {}, errors);
  await page.goto(base + '/workbench.html'); await page.locator('body').click({ position: { x: 4, y: 4 } });
  await page.evaluate(async () => {
    const [{ MusicStream }, { openHostedContent }] = await Promise.all([import('/music.js'), import('/content.js')]);
    const worker = new Worker('/worker.js', { type: 'module' });
    let id = 0; const pending = new Map(), requests = [], errors = [];
    worker.onmessage = ({ data }) => {
      if (data.startupError) { errors.push(data.startupError); return; }
      const request = pending.get(data.id); if (!request) return;
      pending.delete(data.id); data.error ? request.reject(new Error(data.error)) : request.resolve(data.bytes);
    };
    const codec = (op, bytes, index = 0) => new Promise((resolve, reject) => { const requestId = ++id; pending.set(requestId, { resolve, reject }); requests.push({ op, count: bytes.length }); worker.postMessage({ id: requestId, op, bytes, index }); });
    const content = await openHostedContent(), context = new AudioContext(); await context.resume();
    const stream = new MusicStream(codec, () => context, () => context.destination, message => errors.push(message), null);
    await stream.load({ ...content, read: (file, offset, count) => file.readRange(offset, count), isCurrent: () => true });
    window.musicHarness = { stream, context, worker, requests, errors };
  });
  assert.equal(await page.evaluate(() => window.musicHarness.stream.tracks.length), 21);
  for (let index = 0; index < 21; index++) {
    await page.evaluate(index => { window.musicHarness.stream.select(index); }, index);
    await page.waitForFunction(index => {
      const { stream } = window.musicHarness, state = stream.snapshot();
      return state.failed || (!stream.busy && state.selectedIndex === index && state.voices > 0 && state.queuedSeconds > 2.5);
    }, index, { timeout: 60000 });
    const state = await page.evaluate(() => window.musicHarness.stream.snapshot());
    assert.equal(state.failed, false, `Track ${index} stream failed`);
    assert(state.durationSeconds > 150 && state.durationSeconds < 275);
    assert(state.title && state.artist);
    assert(state.queuedSeconds < 4.5, 'PCM remains bounded');
  }
  const state = await page.evaluate(() => {
    const { stream } = window.musicHarness; stream.setEnabled(false); return stream.snapshot();
  });
  await page.waitForTimeout(150); // Time-based on purpose: prove the disabled stream's position stays frozen.
  const paused = await page.evaluate(() => window.musicHarness.stream.snapshot());
  assert.equal(paused.voices, 0); assert.equal(paused.positionSeconds, state.positionSeconds);
  await page.evaluate(() => window.musicHarness.stream.setEnabled(true));
  await page.waitForFunction(() => window.musicHarness.stream.snapshot().voices > 0);
  await page.waitForTimeout(200); // Time-based on purpose: wall-clock audio playback must advance.
  assert(await page.evaluate(position => window.musicHarness.stream.snapshot().positionSeconds > position, state.positionSeconds));
  const evidence = await page.evaluate(() => ({ tracks: window.musicHarness.stream.tracks.map(({ title, artist }) => ({ title, artist })), state: window.musicHarness.stream.snapshot(), errors: window.musicHarness.errors, maxDecodeBytes: Math.max(...window.musicHarness.requests.filter(item => item.op === 43).map(item => item.count)) }));
  assert.equal(evidence.maxDecodeBytes, 65536); assert.deepEqual(evidence.errors, []); assertNoErrors(errors);
  console.log(JSON.stringify({ passed: true, songs: evidence.tracks.length, maxDecodeBytes: evidence.maxDecodeBytes, state: evidence.state }));
  await page.evaluate(async () => { window.musicHarness.stream.close(); window.musicHarness.worker.terminate(); await window.musicHarness.context.close(); });
} finally { await browser.close(); }
