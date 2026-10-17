import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MAX_DECODED_VOCALS, QuadSounds, makeChain, setGains, startChain, stopChain } from './quad-sounds.js';
import { loadAudioSettings, saveAudioSettings } from './audio.js';

function fakeContext() {
  const nodes = [];
  const node = extra => { const n = { connected: [], connect(t) { this.connected.push(t); }, disconnect() { this.connected = []; }, ...extra }; nodes.push(n); return n; };
  return { nodes, state: 'running', createGain: () => node({ gain: { value: 1 } }), createChannelMerger: () => node({}),
    createBufferSource: () => node({ playbackRate: { value: 1 }, started: 0, stopped: 0, start() { this.started++; }, stop() { this.stopped++; } }) };
}
const storage = () => { const map = new Map(); return { getItem: k => map.get(k) ?? null, setItem: (k, v) => map.set(k, v) }; };

test('voice chains are persistent and only swap their buffer source', () => {
  const context = fakeContext(), chain = makeChain(context, { connect() {} }, 'vocal');
  assert.equal(chain.merger.atvVoice, 'vocal');
  const sample = { buffer: {}, info: { loop_start: 10, loop_end: 100, sample_rate: 1000 } };
  const first = startChain(context, chain, sample, { loop: true });
  assert(first.loop && first.loopStart === 0.01 && first.loopEnd === 0.1 && !chain.ended);
  const second = startChain(context, chain, { buffer: {}, info: {} }, { loop: true });
  assert.equal(first.stopped, 1);
  assert(!second.loop, 'unmarked clips are not looped');
  setGains(chain, { left: 0.25, right: 0.5, rate: 1.5 });
  assert.deepEqual([chain.left.gain.value, chain.right.gain.value, second.playbackRate.value], [0.25, 0.5, 1.5]);
  setGains(chain, null);
  assert.equal(chain.left.gain.value, 0);
  stopChain(chain);
  assert(chain.ended && chain.source === null);
  assert.equal(context.nodes.filter(n => n.playbackRate).length, 2, 'one source per start');
});

test('vocal clips decode lazily into a bounded cache', async () => {
  const codecCalls = [];
  const audio = { closed: false, report() {}, context: { decodeAudioData: async () => ({ duration: 1 }) },
    findSample: async name => ({ bytes: new Uint8Array([1]), index: name.length }),
    codec: async op => { codecCalls.push(op); return op === 5 ? new TextEncoder().encode('{"sample_rate":44100}') : new Uint8Array(4); } };
  const quad = new QuadSounds(audio);
  for (let i = 0; i < MAX_DECODED_VOCALS + 6; i++) await quad.vocalBuffer(`rider/mgrunt/${i}.vag`);
  assert.equal(quad.decoded.size, MAX_DECODED_VOCALS);
  assert(!quad.decoded.has('rider/mgrunt/0.vag'));
  const before = codecCalls.length;
  await quad.vocalBuffer(`rider/mgrunt/${MAX_DECODED_VOCALS + 5}.vag`);
  assert.equal(codecCalls.length, before, 'a cached clip is reused');
  assert.equal(quad.stats.decodes, MAX_DECODED_VOCALS + 6);
});

test('separate SFX volume persists and clamps', () => {
  const store = storage();
  assert.equal(loadAudioSettings(store).sfx, 1);
  saveAudioSettings({ sfx: 0.25 }, store);
  assert.equal(loadAudioSettings(store).sfx, 0.25);
  store.setItem('atv-audio-v1', '{"sfx":9}');
  assert.equal(loadAudioSettings(store).sfx, 1);
  store.setItem('atv-audio-v1', 'not json');
  assert.equal(loadAudioSettings(store).sfx, 1);
});
