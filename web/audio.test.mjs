import { test } from 'node:test';
import assert from 'node:assert/strict';
import { GameAudio } from './audio.js';

test('paused Music preview advances streaming without starting engine audio; focus exit suspends it', async () => {
  const listeners = new Map();
  const previousWindow = globalThis.window;
  class Context {
    constructor() { this.state = 'suspended'; this.destination = {}; this.currentTime = 0; }
    createGain() { return { gain: { value: 1 }, connect() {} }; }
    async resume() { this.state = 'running'; }
    async suspend() { this.state = 'suspended'; }
    async close() { this.state = 'closed'; }
  }
  globalThis.window = { AudioContext: Context, addEventListener: (event, listener) => listeners.set(event, listener), removeEventListener: event => listeners.delete(event) };
  const audio = new GameAudio(async () => { throw new Error('Engine must not be updated in menu'); });
  let ticks = 0; audio.music.update = async () => { ticks++; };
  try {
    await audio.setActive(false);
    await audio.setMusicPreview(true);
    assert.equal(audio.context.state, 'running'); assert.equal(audio.engineGain.gain.value, 0);
    assert.equal(audio.commands, 0); assert(ticks > 0);
    await audio.setActive(false);
    assert.equal(audio.context.state, 'running', 'Physics pause does not suspend requested music preview');
    await audio.setMusicPreview(false);
    assert.equal(audio.context.state, 'suspended'); assert.equal(audio.musicTimer, null);
    await audio.setActive(true);
    assert.equal(audio.context.state, 'running'); assert.equal(audio.engineGain.gain.value, 1);
    assert.equal(audio.musicPreview, false);
    audio.close(); assert.equal(listeners.size, 0);
  } finally { audio.close(); globalThis.window = previousWindow; }
});

test('body loops use original loop points and channel/pitch commands without duplicating engine voices', async () => {
  const previousWindow = globalThis.window;
  const nodes = [], events = [];
  class Node {
    constructor() { this.gain = { value: 1 }; this.playbackRate = { value: 1 }; nodes.push(this); }
    connect(...args) { (this.connections ??= []).push(args); }
    disconnect() { this.disconnected = true; }
    start() { this.started = true; events.push('start'); }
    stop() { this.stopped = true; events.push('stop'); }
  }
  class Context {
    constructor() { this.state = 'suspended'; this.destination = {}; this.currentTime = 0; }
    createGain() { return new Node(); }
    createBufferSource() { return new Node(); }
    createChannelMerger() { return new Node(); }
    async resume() { this.state = 'running'; }
    async suspend() { this.state = 'suspended'; }
    async close() { this.state = 'closed'; }
  }
  globalThis.window = { AudioContext: Context, addEventListener() {}, removeEventListener() {} };
  const definitions = [0, 1].map(car => ({ car, sample: 'train', min_distance: 150, max_distance: 350, looping: true, start_disabled: false }));
  let commands = [{ active: true, left: 100, right: 0, pitch: 4200 }, { active: true, left: 12, right: 50, pitch: 4096 }];
  const requests = [];
  const audio = new GameAudio(async (op, bytes) => {
    if (op === 72) return new TextEncoder().encode(JSON.stringify(definitions));
    if (op === 73) { requests.push(JSON.parse(new TextDecoder().decode(bytes))); return new TextEncoder().encode(JSON.stringify(commands)); }
    if (op === 33) return new TextEncoder().encode('null');
    if (op === 34) return new Uint8Array();
    throw new Error(`Unexpected op ${op}`);
  });
  audio.music.update = async () => {};
  audio.loadSample = async () => {};
  audio.buffers.set('train', { buffer: { duration: 3 }, info: { channels: 1, sample_rate: 100, loop_start: 30, loop_end: 230 } });
  audio.ready = true; audio.config = { samples: Array.from({ length: 10 }, () => []) };
  const pose = x => [1,0,0,0,0,1,0,0,0,0,1,0,x,0,0,1];
  const scene = x => ({ train: { cars: [0,1].map(car => ({ matrix: pose(x + car) })) }, camera: { position: [0,0,0], target: [0,0,1] } });
  const frame = { grounded: [false], speed_magnitude: 0 };
  try {
    await audio.loadSpatial({ toy: new Uint8Array(), isCurrent: () => true }); await audio.resume();
    await audio.update(1 / 60, { throttle: 0 }, frame, scene(0));
    assert.equal(audio.spatialVoices.size, 2); assert.equal(audio.spatialStarts, 2);
    const voice = audio.spatialVoices.get(0);
    assert(voice.source.loop); assert.equal(voice.source.loopStart, .3); assert.equal(voice.source.loopEnd, 2.3);
    assert.equal(voice.left.gain.value, 1); assert.equal(voice.right.gain.value, 0);
    assert.equal(voice.source.playbackRate.value, 4200 / 4096);
    assert.deepEqual(requests[0].cars, requests[0].previous_cars);
    commands = commands.map(command => ({ ...command, active: false, left: 0, right: 0, pitch: 4096 }));
    await audio.update(1 / 60, { throttle: 0 }, frame, scene(10));
    assert.equal(voice.left.gain.value, 0); assert.equal(voice.right.gain.value, 0);
    assert.equal(audio.spatialStarts, 2); assert.equal(requests[1].previous_cars[0][12], 0);
    audio.stopSources(); assert.equal(audio.spatialVoices.size, 2);
    await audio.reset(); assert.equal(audio.spatialVoices.size, 2);
    await audio.setActive(false); assert.equal(audio.context.state, 'suspended');
    assert.equal(audio.engineGain.gain.value, 0);
    await audio.setMusicPreview(true); assert.equal(audio.engineGain.gain.value, 0);
    await audio.setMusicPreview(false); await audio.setActive(true);
    await audio.update(1 / 60, { throttle: 0 }, frame, scene(20)); assert.equal(audio.spatialStarts, 2);
    audio.close(); assert.equal(audio.spatialVoices.size, 0); assert(voice.source.stopped);
    assert.equal(events.filter(event => event === 'start').length, 2);
    assert(nodes.every(node => !node.started || node.stopped));
  } finally { audio.close(); globalThis.window = previousWindow; }
});

test('a missing train bank leaves the existing engine/music player usable', async () => {
  const previousWindow = globalThis.window;
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  const reports = [];
  const audio = new GameAudio(async () => new TextEncoder().encode(JSON.stringify([{ sample: 'missing' }])), message => reports.push(message));
  audio.ready = true;
  audio.loadSample = async () => { throw new Error('missing original sample'); };
  try {
    await audio.loadSpatial({ toy: new Uint8Array(), isCurrent: () => true });
    assert(audio.ready); assert.equal(audio.spatialReady, false); assert.equal(audio.spatialVoices.size, 0);
    assert.deepEqual(reports, ['Train sound: missing original sample']);
  } finally { audio.close(); globalThis.window = previousWindow; }
});
