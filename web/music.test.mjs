import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MusicStream } from './music.js';
const encode = value => new TextEncoder().encode(JSON.stringify(value));
const decode = value => JSON.parse(new TextDecoder().decode(value));
const flush = async () => { for (let i = 0; i < 30; i++) await new Promise(resolve => setImmediate(resolve)); };
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function fixture({ songBytes = 65536 * 8, storage = null } = {}) {
  const songs = Array.from({ length: 21 }, (_, index) => ({ filename: `Music/song${index}.vag`, title: `Track ${index}`, artist: `Artist ${index}`, genre: 'Rock', locked: false }));
  const files = [{ path: '/STARTAPP.RES', lba: 0, bytes: 16 }, ...songs.map((song, index) => ({ path: `/AUDIO/${song.filename}`, lba: (index + 1) * 1000, bytes: 2016 + songBytes }))];
  const calls = [], reads = [], sources = [], reports = [];
  let next = 0, decoding = null, readHook = null;
  const codec = async (op, bytes, index) => {
    calls.push({ op, bytes, index, decoding });
    if (op === 2) return encode([{ name: 'music.pb' }]);
    if (op === 3) return new Uint8Array([1]);
    if (op === 40) return encode({ songs });
    if (op === 41) { const index = bytes.length ? decode(bytes).select : next++ % songs.length; return encode(songs[index]); }
    if (op === 42) { decoding = bytes[0]; return encode({ channels: 2, sample_rate: 48000, frames_max: songBytes / 32 * 28, data_offset: 2016, data_bytes: songBytes }); }
    if (op === 43) { assert.equal(bytes[0], decoding, 'PCM chunk must belong to the initialized decoder'); return new Uint8Array(bytes.length / 16 * 28 * 2); }
    throw new Error(`Unexpected codec op ${op}`);
  };
  const read = async (_, offset, count) => {
    reads.push({ offset, count });
    if (readHook) await readHook(offset, count);
    const index = Math.floor(offset / (1000 * 2048)) - 1;
    return new Uint8Array(count).fill(Math.max(0, index));
  };
  const context = {
    currentTime: 0, state: 'running',
    createGain: () => ({ gain: { value: 1 }, connect() {}, disconnect() {} }),
    createBuffer: (channels, frames, rate) => ({ duration: frames / rate, getChannelData: () => new Float32Array(frames) }),
    createBufferSource() {
      const source = { started: false, stopped: false, connect() {}, disconnect() {}, start(at, offset = 0) { this.at = at; this.offset = offset; this.started = true; }, stop() { this.stopped = true; } };
      sources.push(source); return source;
    },
  };
  const stream = new MusicStream(codec, () => context, () => ({}), message => reports.push(message), storage);
  const load = () => stream.load({ file: {}, files, read, isCurrent: () => true });
  const advance = dt => { context.currentTime += dt; for (const source of sources) if (!source.stopped && source.started && source.at + source.buffer.duration - source.offset <= context.currentTime) { source.stopped = true; source.onended?.(); } };
  return { stream, codec, context, calls, sources, reads, reports, load, advance, hook: callback => { readHook = callback; } };
}

test('all 21 catalog tracks are selectable; streaming stays bounded to 64 KiB decode chunks', async () => {
  const f = fixture(); await f.load();
  assert.equal(f.stream.tracks.length, 21);
  for (let index = 0; index < 21; index++) {
    assert(f.stream.select(index)); await flush();
    assert.equal(f.stream.snapshot().selectedIndex, index);
    assert(f.stream.snapshot().queuedSeconds >= 3 && f.stream.snapshot().queuedSeconds < 4.4);
    assert(f.stream.snapshot().durationSeconds > 9);
    assert.equal(f.stream.snapshot().artist, `Artist ${index}`);
  }
  assert(f.calls.filter(call => call.op === 43).every(call => call.bytes.length <= 65536));
  assert.equal(f.reports.length, 0);
  assert(!f.stream.select(-1)); assert(!f.stream.select(21)); assert(!f.stream.select(NaN));
});

test('each session seeds the original shuffle with a fresh random 32-bit value', async () => {
  const seeds = [];
  for (let i = 0; i < 4; i++) {
    const f = fixture(); await f.load();
    const seed = f.calls.find(call => call.op === 40).index;
    assert(Number.isInteger(seed) && seed > 0 && seed <= 0xffffffff);
    seeds.push(seed);
  }
  assert(new Set(seeds).size > 1, 'sessions must not share one shuffle seed');
});

test('track ending queues the next song on the same clock and title follows audible track', async () => {
  const f = fixture({ songBytes: 65536 }); await f.load(); await f.stream.update();
  assert(f.calls.filter(call => call.op === 41).length >= 3);
  const first = f.sources[0], second = f.sources[1];
  assert.equal(second.at, first.at + first.buffer.duration);
  assert.equal(f.stream.snapshot().title, 'Track 0');
  f.advance(second.at + 0.01);
  assert.equal(f.stream.snapshot().title, 'Track 1');
  assert.equal(f.stream.snapshot().underruns, 0);
});

test('latest manual selection cancels a blocked old read without sharing decoder history', async () => {
  const f = fixture(); await f.load(); await f.stream.update();
  const blocked = deferred(); let hit = false;
  f.hook(async (offset, count) => { if (count === 65536 && !hit) { hit = true; await blocked.promise; } });
  f.advance(2); const pending = f.stream.update(); await flush(); assert(hit);
  assert(f.stream.select(8)); assert(f.stream.select(20));
  blocked.resolve(); await pending; await flush();
  assert.equal(f.stream.snapshot().title, 'Track 20');
  assert.equal(f.stream.snapshot().selectedIndex, 20);
  assert.equal(f.calls.filter(call => call.op === 41).at(-1).bytes.length > 0, true);
  assert.equal(f.sources.filter(source => !source.stopped).length, f.stream.snapshot().voices);
  assert.equal(f.reports.length, 0);
});

test('music off pauses at a PCM offset and resumes queued music without consuming more data', async () => {
  const f = fixture(); await f.load(); await f.stream.update(); f.advance(0.6);
  const position = f.stream.snapshot().positionSeconds, reads = f.reads.length;
  f.stream.setEnabled(false); assert.equal(f.stream.snapshot().voices, 0);
  f.advance(10); await f.stream.update();
  assert.equal(f.stream.snapshot().positionSeconds, position);
  assert.equal(f.reads.length, reads);
  f.stream.setEnabled(true); await flush();
  assert(f.stream.snapshot().voices > 0);
  assert.equal(f.stream.snapshot().positionSeconds, position);
  assert.equal(f.stream.snapshot().underruns, 0);
});

test('disabling during an in-flight decode preserves that chunk for resume', async () => {
  const f = fixture(); await f.load();
  const blocked = deferred(); let hit = false;
  f.hook(async (_, count) => { if (count === 65536 && !hit) { hit = true; await blocked.promise; } });
  const pending = f.stream.update(); await flush(); assert(hit);
  f.stream.setEnabled(false); blocked.resolve(); await pending;
  assert.equal(f.stream.snapshot().voices, 0);
  assert(f.stream.snapshot().queuedSeconds > 0);
  f.stream.setEnabled(true); await flush();
  assert.equal(f.stream.snapshot().positionSeconds, 0);
  assert.equal(f.reports.length, 0);
});

test('manual next/previous history and playback settings survive a replacement player', async () => {
  const data = new Map(), storage = { getItem: key => data.get(key), setItem: (key, value) => data.set(key, value) };
  const f = fixture({ storage }); await f.load(); f.stream.select(9); await flush(); f.stream.select(14); await flush();
  f.stream.previous(); await flush(); assert.equal(f.stream.snapshot().title, 'Track 9');
  f.stream.next(); await flush(); assert.equal(f.stream.snapshot().title, 'Track 14');
  f.stream.setVolume(.27); f.stream.setEnabled(false); f.stream.close();
  const second = fixture({ storage });
  assert.equal(second.stream.snapshot().volume, .27); assert.equal(second.stream.snapshot().enabled, false);
  assert.equal(second.stream.next(), false); await second.load();
  second.stream.setEnabled(true); await flush(); second.stream.previous(); await flush();
  assert.equal(second.stream.snapshot().title, 'Track 14');
});

test('failed requests are retryable by selecting again; closed loads never schedule sound', async () => {
  const f = fixture(); await f.load(); f.hook(() => { throw new Error('connection interrupted'); });
  await f.stream.update(); assert.equal(f.stream.snapshot().failed, true); assert.match(f.reports[0], /connection interrupted/);
  f.hook(null); f.stream.select(5); await flush(); assert.equal(f.stream.snapshot().failed, false); assert.equal(f.stream.snapshot().title, 'Track 5');
  const blocked = deferred(); f.hook(async () => blocked.promise); f.stream.select(6); await flush(); f.stream.close(); blocked.resolve(); await flush();
  assert.equal(f.stream.snapshot().voices, 0); assert(f.sources.every(source => source.stopped));
});

test('rapid off/on during a fetch resumes each queued buffer only once', async () => {
  const f = fixture(); await f.load(); await f.stream.update(); f.advance(1.5);
  const blocked = deferred(); let hit = false;
  f.hook(async (_, count) => { if (count === 65536 && !hit) { hit = true; await blocked.promise; } });
  const pending = f.stream.update(); await flush(); assert(hit);
  f.stream.setEnabled(false); f.stream.setEnabled(true); blocked.resolve(); await pending; await flush(); await f.stream.update();
  const live = f.sources.filter(source => !source.stopped);
  assert.equal(live.length, f.stream.queue.length);
  assert.equal(new Set(live.map(source => source.buffer)).size, live.length);
  for (let i = 1; i < live.length; i++) assert(Math.abs(live[i].at - (live[i - 1].at + live[i - 1].buffer.duration - live[i - 1].offset)) < 1e-12);
});

test('Next uses the audible song when the following track is already buffered', async () => {
  const f = fixture({ songBytes: 65536 }); await f.load(); await f.stream.update();
  assert.equal(f.stream.snapshot().title, 'Track 0');
  assert(f.stream.snapshot().selectedIndex > 0, 'Short tracks allow multiple queued songs');
  f.stream.next(); await flush();
  assert.equal(f.stream.snapshot().title, 'Track 1', 'Next must not skip the prebuffered track');
});
