import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MusicStream } from './music.js';
const encode = value => new TextEncoder().encode(JSON.stringify(value));
const decode = value => JSON.parse(new TextDecoder().decode(value));
const flush = async () => { for (let i = 0; i < 30; i++) await new Promise(resolve => setImmediate(resolve)); };
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function fakeElement() {
  const listeners = {};
  return { src: '', paused: true, muted: false, currentTime: 0, duration: NaN, preload: '', plays: 0, reject: null, buffered: { length: 0 },
    addEventListener(type, callback) { (listeners[type] ??= []).push(callback); }, emit(type) { for (const callback of listeners[type] ?? []) callback(); },
    play() { this.plays++; if (this.reject) return Promise.reject(Object.assign(new Error(this.reject), { name: this.reject })); this.paused = false; return Promise.resolve(); },
    pause() { this.paused = true; }, removeAttribute(name) { if (name === 'src') this.src = ''; }, load() {} };
}
function fixture({ songBytes = 65536 * 8, storage = null, element = null, streams = Boolean(element) } = {}) {
  const songs = Array.from({ length: 21 }, (_, index) => ({ filename: `Music/song${index}.vag`, title: `Track ${index}`, artist: `Artist ${index}`, genre: 'Rock', locked: false }));
  const streamRecord = index => ({ url: `/content/${String(index).padStart(64, '0')}.m4a`, bytes: 1000 + index, sha256: String(index).padStart(64, '0'), type: 'audio/mp4', seconds: 180 + index });
  const files = [{ path: '/STARTAPP.RES', lba: 0, bytes: 16 }, ...songs.map((song, index) => ({ path: `/AUDIO/${song.filename}`, lba: (index + 1) * 1000, bytes: 2016 + songBytes, ...(streams && (streams === true || streams(index)) ? { stream: streamRecord(index) } : {}) }))];
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
  const contextListeners = [];
  const context = {
    currentTime: 0, state: 'running', mediaNodes: 0,
    addEventListener(type, callback) { if (type === 'statechange') contextListeners.push(callback); },
    setState(state) { this.state = state; for (const callback of contextListeners) callback(); },
    createMediaElementSource() { this.mediaNodes++; return { connect() {}, disconnect() {} }; },
    createGain: () => ({ gain: { value: 1, cancelScheduledValues() {}, setValueAtTime() {}, linearRampToValueAtTime() {} }, connect() {}, disconnect() {} }),
    createBuffer: (channels, frames, rate) => ({ duration: frames / rate, getChannelData: () => new Float32Array(frames) }),
    createBufferSource() {
      const source = { started: false, stopped: false, connect() {}, disconnect() {}, start(at, offset = 0) { this.at = at; this.offset = offset; this.started = true; }, stop() { this.stopped = true; } };
      sources.push(source); return source;
    },
  };
  const stream = new MusicStream(codec, () => context, () => ({}), message => reports.push(message), storage);
  stream.mediaFactory = () => element;
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

test('streamed songs play through one media element with no VAG reads or PCM decoding', async () => {
  const element = fakeElement(), f = fixture({ element }); await f.load();
  assert.equal(f.stream.media, true);
  await f.stream.update();
  assert.equal(f.reads.length, 1, 'only the catalog archive is read');
  assert(!f.calls.some(call => call.op === 42 || call.op === 43));
  assert.equal(element.src, `/content/${'0'.padStart(64, '0')}.m4a`);
  assert.equal(element.paused, false); assert.equal(f.context.mediaNodes, 1);
  let snapshot = f.stream.snapshot();
  assert.equal(snapshot.mode, 'stream'); assert.equal(snapshot.seekable, true); assert.equal(snapshot.durationSeconds, 180);
  assert.equal(snapshot.title, 'Track 0'); assert.equal(snapshot.genre, 'Rock'); assert.equal(snapshot.voices, 1);
  assert(f.stream.select(7)); await flush();
  assert.equal(element.src, `/content/${'7'.padStart(64, '0')}.m4a`); assert.equal(f.stream.snapshot().selectedIndex, 7);
  assert(f.stream.seek(60)); assert.equal(f.stream.snapshot().positionSeconds, 60);
  f.stream.seek(10_000); assert.equal(element.currentTime, 186.75);
  f.stream.seek(-5); assert.equal(element.currentTime, 0);
  assert.equal(f.context.mediaNodes, 1, 'one element source for the whole session');
  assert.equal(f.reports.length, 0);
});

test('a finished stream continues with the original shuffle; previous returns through history', async () => {
  const element = fakeElement(), f = fixture({ element }); await f.load(); await f.stream.update();
  element.paused = true; element.emit('ended'); await flush();
  assert.equal(f.stream.snapshot().title, 'Track 1'); assert.equal(element.paused, false);
  assert(f.stream.previous()); await flush();
  assert.equal(f.stream.snapshot().title, 'Track 0');
  assert(f.stream.next()); await flush();
  assert.equal(f.stream.snapshot().title, 'Track 1', 'next walks forward history before a new shuffle pick');
});

test('stream on/off, race holds and a suspended context pause the element', async () => {
  const element = fakeElement(), f = fixture({ element }); await f.load(); await f.stream.update();
  const src = element.src;
  f.stream.setEnabled(false); assert.equal(element.paused, true);
  await f.stream.update(); assert.equal(element.paused, true);
  f.stream.setEnabled(true); await flush();
  assert.equal(element.paused, false); assert.equal(element.src, src, 'on resumes the same song');
  f.context.setState('suspended'); assert.equal(element.paused, true);
  f.context.setState('running'); await f.stream.update(); assert.equal(element.paused, false);
  f.stream.hold(0); assert.equal(element.paused, true);
  await f.stream.update(); assert.equal(element.paused, true, 'held music stays silent');
  f.stream.release(); await flush();
  assert.equal(element.paused, false); assert.notEqual(element.src, src, 'GO starts the next song');
});

test('autoplay refusal waits for a gesture; an unplayable stream falls back to the decoder', async () => {
  const element = fakeElement(), f = fixture({ element }); await f.load();
  element.reject = 'NotAllowedError'; await f.stream.update();
  assert.equal(f.stream.media, true); assert.equal(element.paused, true);
  element.reject = null; f.stream.unlock(); await flush();
  assert.equal(element.plays, 2); assert.equal(element.paused, false);
  element.reject = 'NotAllowedError'; f.stream.select(3); await flush();
  f.stream.setEnabled(false); element.reject = null; f.stream.unlock(); await flush();
  assert.equal(element.paused, true, 'a gesture never restarts music the player turned off');
  assert.equal(element.muted, false);
  const other = fakeElement(), g = fixture({ element: other }); await g.load();
  other.reject = 'NotSupportedError'; await g.stream.update(); await flush();
  assert.equal(g.stream.media, false); assert.equal(g.stream.snapshot().mode, 'decoder');
  assert(g.calls.some(call => call.op === 43), 'the same song now decodes from the VAG');
  assert.match(g.reports[0], /decoding the original audio/);
});

test('a package without every stream keeps the decoder for the whole catalog', async () => {
  const element = fakeElement(), f = fixture({ element, streams: index => index !== 3 }); await f.load();
  assert.equal(f.stream.media, false);
  await f.stream.update();
  assert.equal(element.src, ''); assert(f.calls.some(call => call.op === 43));
  assert.equal(f.stream.snapshot().seekable, false);
  assert.equal(f.stream.seek(10), false);
});

test('media session mirrors the song and drives the soundtrack transport', () => {
  const handlers = {}, session = { metadata: null, playbackState: 'none', setActionHandler: (action, fn) => { handlers[action] = fn; } };
  const previous = globalThis.MediaMetadata;
  globalThis.MediaMetadata = class { constructor(init) { Object.assign(this, init); } };
  try {
    const stream = new MusicStream(async () => new Uint8Array(), () => null, () => ({}), () => {}, null);
    const calls = [];
    stream.next = () => calls.push('next'); stream.previous = () => calls.push('previous'); stream.update = async () => calls.push('update');
    stream.bindMediaSession(session);
    assert.deepEqual(Object.keys(handlers).sort(), ['nexttrack', 'pause', 'play', 'previoustrack']);
    assert.equal(session.playbackState, 'paused');
    stream.song = { title: 'Song A', artist: 'Band' }; stream.syncMediaSession();
    assert.equal(session.metadata.title, 'Song A');
    assert.equal(session.metadata.artist, 'Band');
    assert.equal(session.metadata.artwork[0].src, '/fury-remastered-home-512-v1.png');
    assert.equal(session.playbackState, 'playing');
    handlers.pause();
    assert.equal(stream.enabled, false);
    assert.equal(session.playbackState, 'paused');
    handlers.nexttrack();
    assert.equal(stream.enabled, true);
    handlers.previoustrack(); handlers.play();
    assert.deepEqual(calls.filter(call => call !== 'update').slice(0, 2), ['next', 'previous']);
    stream.hold(); assert.equal(session.playbackState, 'paused');
  } finally { globalThis.MediaMetadata = previous; }
});
