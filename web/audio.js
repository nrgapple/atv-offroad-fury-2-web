// Original decoded samples; selection and transition rules run in Rust.
// Browser resampling/mixing is not a bit-exact SPU2 implementation.
import { MusicStream } from './music.js';
export class GameAudio {
  constructor(codec, report = () => {}) {
    this.codec = codec;
    this.report = report;
    this.context = null;
    this.buffers = new Map();
    this.sources = new Set();
    this.current = null;
    this.ready = false;
    this.active = true;
    this.closed = false;
    this.busy = false;
    this.elapsed = 0;
    this.commands = 0;
    this.generation = 0;
    this.lastSound = null;
    this.music = new MusicStream(codec, () => this.context, () => this.gain, report);
    this.unlock = () => this.resume().catch(error => this.report(error.message));
    window.addEventListener('pointerdown', this.unlock);
    window.addEventListener('keydown', this.unlock);
  }
  async resume() {
    if (this.closed || !this.active) return;
    if (!this.context) {
      const AudioContext = window.AudioContext || window.webkitAudioContext;
      if (!AudioContext) throw new Error('This browser does not expose Web Audio.');
      this.context = new AudioContext();
      this.gain = this.context.createGain();
      this.gain.gain.value = 0.65;
      this.gain.connect(this.context.destination);
    }
    await this.context.resume();
  }
  async load({ file, files, read, isCurrent }) {
    const text = new TextDecoder();
    const json = async (op, bytes, index = 0) => JSON.parse(text.decode(await this.codec(op, bytes, index)));
    const archive = async path => {
      const entry = files.find(item => item.path.toLowerCase() === path.toLowerCase());
      if (!entry) throw new Error(`Original audio archive is missing: ${path}`);
      const bytes = await read(file, entry.lba * 2048, entry.bytes);
      return { bytes, entries: await json(2, bytes) };
    };
    const table = await archive('/AUDIOTBL.RES');
    const tableIndex = table.entries.findIndex(entry => /(^|[\\/])vehicle\.aud$/i.test(entry.name));
    if (tableIndex < 0) throw new Error('Original vehicle audio table is missing.');
    this.config = await json(32, await this.codec(3, table.bytes, tableIndex), 1);
    const sounds = await archive('/AUDIO/AUDIO.RES');
    const normal = path => path.replaceAll('\\', '/').toLowerCase().replace(/^audio\//, '');
    const lookup = new Map(sounds.entries.map((entry, index) => [normal(entry.name), index]));
    // Keep encoded WAV until a user gesture creates/unlocks the audio context.
    for (const name of new Set(this.config.samples.flat())) {
      if (!isCurrent() || this.closed) return;
      const index = lookup.get(normal(name));
      if (index === undefined) throw new Error(`Original engine sample is missing: ${name}`);
      const vag = await this.codec(3, sounds.bytes, index);
      const info = await json(5, vag);
      const wave = await this.codec(4, vag);
      this.buffers.set(name, { wave, info, buffer: null });
    }
    if (!isCurrent() || this.closed) return;
    await this.music.load({ file, files, read, isCurrent });
    if (!isCurrent() || this.closed) return;
    this.ready = true;
  }
  async decode() {
    if (!this.context || this.context.state !== 'running' || !this.ready) return false;
    for (const sample of this.buffers.values()) {
      if (!sample.buffer) {
        sample.buffer = await this.context.decodeAudioData(sample.wave.buffer.slice(sample.wave.byteOffset, sample.wave.byteOffset + sample.wave.byteLength));
        sample.wave = null;
      }
      if (this.closed) return false;
    }
    return true;
  }
  play(name, sound, overlap) {
    const sample = this.buffers.get(name);
    if (!sample?.buffer || this.closed) return;
    if (!overlap) this.stopSources();
    const source = this.context.createBufferSource();
    source.buffer = sample.buffer;
    source.loop = sound === 'Idle'; // Original idle load flags include0x80.
    if (source.loop && sample.info.loop_start !== null && sample.info.loop_end !== null) {
      source.loopStart = sample.info.loop_start / sample.info.sample_rate;
      source.loopEnd = sample.info.loop_end / sample.info.sample_rate;
    }
    source.connect(this.gain);
    const voice = { source, started: this.context.currentTime, duration: sample.buffer.duration, loop: source.loop };
    this.sources.add(voice);
    source.onended = () => { this.sources.delete(voice); source.disconnect(); };
    source.start();
    this.current = voice;
    this.commands++;
    this.lastSound = sound;
  }
  stopSources() {
    for (const voice of this.sources) {
      voice.source.onended = null;
      voice.source.stop();
      voice.source.disconnect();
    }
    this.sources.clear();
    this.current = null;
  }
  async update(dt, input, frame) {
    if (!this.ready || !this.active || this.closed) return;
    void this.music.update();
    this.elapsed += dt;
    if (this.busy || !this.context || this.context.state !== 'running') return;
    this.busy = true;
    const generation = this.generation;
    try {
      if (!await this.decode()) return;
      if (generation !== this.generation) return;
      if (!this.current && this.commands === 0 && this.config.samples[1].length) {
        // Original002cae30 loads and selects the last idle sample before updates.
        this.play(this.config.samples[1].at(-1), 'Idle', false);
      }
      const elapsed = this.elapsed;
      this.elapsed = 0;
      const current = this.current;
      const payload = {
        dt: Math.min(elapsed, 0.1), throttle: input.throttle,
        grounded: frame.grounded.some(Boolean), speed: frame.speed_magnitude,
        duration: current?.duration ?? 0,
        playhead: current ? this.context.currentTime - current.started : 0,
      };
      const bytes = await this.codec(33, new TextEncoder().encode(JSON.stringify(payload)));
      if (this.closed || !this.active || generation !== this.generation) return;
      const result = JSON.parse(new TextDecoder().decode(bytes));
      if (result) this.play(result.sample, result.command.sound, result.command.overlap);
    } catch (error) { this.report(error.message); this.ready = false; }
    finally { this.busy = false; }
  }
  async setActive(active) {
    this.active = active;
    this.elapsed = 0;
    if (!this.context) return;
    if (active) await this.resume();
    else await this.context.suspend();
  }
  async reset() {
    if (this.closed) return;
    this.generation++;
    this.stopSources();
    this.elapsed = 0;
    this.commands = 0;
    this.lastSound = null;
    if (this.ready) await this.codec(34, new Uint8Array());
  }
  snapshot() {
    return { ready: this.ready, context: this.context?.state ?? 'locked', samples: this.buffers.size,
      voices: this.sources.size, commands: this.commands, sound: this.lastSound, music: this.music.snapshot() };
  }
  close() {
    this.generation++;
    this.closed = true;
    this.music.close();
    this.ready = false;
    window.removeEventListener('pointerdown', this.unlock);
    window.removeEventListener('keydown', this.unlock);
    this.stopSources();
    this.context?.close();
    this.buffers.clear();
  }
}
