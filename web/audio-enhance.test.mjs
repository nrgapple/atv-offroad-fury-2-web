import { test } from 'node:test';
import assert from 'node:assert/strict';
import { CROSSFADE_SECONDS, ECHO_PRESETS, createEchoBus, fadeOut } from './audio-enhance.js';
import { loadAudioSettings, saveAudioSettings } from './audio.js';

function param(value = 0) { const p = { value, events: [], setValueAtTime(v, t) { this.events.push(['set', v, t]); }, linearRampToValueAtTime(v, t) { this.events.push(['ramp', v, t]); }, cancelScheduledValues(t) { this.events.push(['cancel', t]); } }; return p; }
function fakeContext() {
  const created = [];
  const node = extra => { const n = { links: [], connect(t) { this.links.push(t); }, disconnect(t) { this.links = t ? this.links.filter(x => x !== t) : []; }, ...extra }; created.push(n); return n; };
  return { created, currentTime: 2, createGain: () => node({ gain: param(1) }), createBiquadFilter: () => node({ frequency: param(), type: '' }),
    createChannelMerger: () => node({}), createDelay: () => node({ delayTime: param() }) };
}

test('one shared echo bus per venue type, none for open venues', () => {
  const context = fakeContext(), input = { links: [], connect(t) { this.links.push(t); }, disconnect(t) { this.links = this.links.filter(x => x !== t); } }, output = {};
  assert.equal(createEchoBus(context, input, output, 'open'), null);
  const bus = createEchoBus(context, input, output, 'stadium');
  assert.equal(input.links.length, 1, 'the SFX bus feeds a single send');
  assert.equal(bus.nodes, context.created.length);
  const delays = context.created.filter(n => n.delayTime).map(n => n.delayTime.value);
  assert.deepEqual(delays, ECHO_PRESETS.stadium.delays);
  bus.dispose();
  assert.equal(input.links.length, 0);
  assert(context.created.every(n => n.links.length === 0));
});

test('crossfade ramps the old voice out and stops it after the fade', () => {
  const context = fakeContext(), stops = [];
  const voice = { fade: { gain: param(1) }, source: { stop: t => stops.push(t), disconnect() {} } };
  fadeOut(context, voice);
  assert.deepEqual(voice.fade.gain.events.at(-1), ['ramp', 0, 2 + CROSSFADE_SECONDS]);
  assert(stops[0] > 2 + CROSSFADE_SECONDS);
  const direct = { source: { stop: t => stops.push(t ?? 'now'), disconnect() {} } };
  fadeOut(context, direct);
  assert.equal(stops.at(-1), 'now', 'voices without a fade gain stop at once');
});

test('Sound mode defaults to Enhanced and persists Original', () => {
  const map = new Map(), store = { getItem: k => map.get(k) ?? null, setItem: (k, v) => map.set(k, v) };
  assert.equal(loadAudioSettings(store).mode, 'enhanced');
  saveAudioSettings({ sfx: 1, mode: 'original' }, store);
  assert.equal(loadAudioSettings(store).mode, 'original');
  store.setItem('atv-audio-v1', '{"mode":"loud"}');
  assert.equal(loadAudioSettings(store).mode, 'enhanced');
});
