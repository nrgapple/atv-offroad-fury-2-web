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
