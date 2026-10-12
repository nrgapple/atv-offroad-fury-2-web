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
    this.musicPreview = false;
    this.musicTimer = null;
    this.previewGeneration = 0;
    this.closed = false;
    this.busy = false;
    this.elapsed = 0;
    this.commands = 0;
    this.generation = 0;
    this.lastSound = null;
    this.spatialDefinitions = [];
    this.spatialVoices = new Map();
    this.spatialCommands = [];
    this.spatialStarts = 0;
    this.spatialHistory = null;
    this.spatialPending = null;
    this.spatialReady = false;
    this.effectsReady = false;
    this.effectFrames = [];
    this.effectVoices = new Map();
    this.effectStarts = 0;
    this.effectSelections = 0;
    this.effectLast = null;
    this.effectCamera = null;
    this.music = new MusicStream(codec, () => this.context, () => this.gain, report);
    this.unlock = () => this.resume().catch(error => this.report(error.message));
    window.addEventListener('pointerdown', this.unlock);
    window.addEventListener('keydown', this.unlock);
  }
  async resume() {
    if (this.closed || (!this.active && !this.musicPreview)) return;
    if (!this.context) {
      const AudioContext = window.AudioContext || window.webkitAudioContext;
      if (!AudioContext) throw new Error('This browser does not expose Web Audio.');
      this.context = new AudioContext();
      this.gain = this.context.createGain();
      this.gain.gain.value = 0.65;
      this.gain.connect(this.context.destination);
      this.engineGain = this.context.createGain();
      this.engineGain.gain.value = this.active ? 1 : 0;
      this.engineGain.connect(this.gain);
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
    this.loadSample = async name => {
      if (this.buffers.has(name)) return;
      const key = normal(name), index = lookup.get(key) ?? lookup.get(`${key}.vag`);
      if (index === undefined) throw new Error(`Original sound sample is missing: ${name}`);
      const vag = await this.codec(3, sounds.bytes, index);
      const info = await json(5, vag);
      const wave = await this.codec(4, vag);
      if (!isCurrent() || this.closed) return;
      this.buffers.set(name, { wave, info, buffer: null });
    };
    // Keep encoded WAV until a user gesture creates/unlocks the audio context.
    for (const name of new Set(this.config.samples.flat())) {
      if (!isCurrent() || this.closed) return;
      await this.loadSample(name);
    }
    const riderIndex = table.entries.findIndex(entry => /(^|[\\/])rider\.aud$/i.test(entry.name));
    if (riderIndex < 0) throw new Error('Original rider sound table is missing.');
    await this.loadEffects({ bytes: await this.codec(3, table.bytes, riderIndex), isCurrent });
    if (!isCurrent() || this.closed) return;
    await this.music.load({ file, files, read, isCurrent });
    if (!isCurrent() || this.closed) return;
    this.ready = true;
  }
  async loadEffects({ bytes, isCurrent }) {
    try {
      const config = JSON.parse(new TextDecoder().decode(await this.codec(76, bytes)));
      for (const name of new Set(config.body_impact)) {
        if (!isCurrent() || this.closed) return;
        await this.loadSample(name);
      }
      if (!isCurrent() || this.closed) return;
      this.effectsConfig = config; this.effectsReady = true;
    } catch (error) {
      if (isCurrent() && !this.closed) this.report(`Rider effects: ${error.message}`);
    }
  }
  async loadSpatial({ toy, isCurrent }) {
    try {
      const definitions = JSON.parse(new TextDecoder().decode(await this.codec(72, toy)));
      for (const name of new Set(definitions.map(definition => definition.sample))) {
        if (!isCurrent() || this.closed) return;
        await this.loadSample(name);
      }
      if (!isCurrent() || this.closed) return;
      this.spatialDefinitions = definitions;
      this.spatialReady = true;
    } catch (error) {
      // App assets and the separate R2 bank can become available at different
      // times. A missing train sample must leave driving/music usable.
      if (isCurrent() && !this.closed) this.report(`Train sound: ${error.message}`);
    }
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
    source.connect(this.engineGain);
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
  // Separate body voices survive engine sample transitions and vehicle resets.
  // Original stereo uses two integer gains, with its own quantized pitch.
  playSpatial(index, definition, voices = this.spatialVoices) {
    const sample = this.buffers.get(definition.sample);
    if (!sample?.buffer || voices.has(index) || definition.start_disabled || this.closed) return;
    const source = this.context.createBufferSource();
    source.buffer = sample.buffer; source.loop = definition.looping;
    if (source.loop && sample.info.loop_start !== null && sample.info.loop_end !== null) {
      source.loopStart = sample.info.loop_start / sample.info.sample_rate;
      source.loopEnd = sample.info.loop_end / sample.info.sample_rate;
    }
    const left = this.context.createGain(), right = this.context.createGain(), merger = this.context.createChannelMerger(2);
    // Shipped train and body-impact VAGs are mono. Explicit channel count prevents the merger
    // inputs from receiving an accidental speaker-layout upmix.
    left.channelCount = right.channelCount = 1;
    left.channelCountMode = right.channelCountMode = 'explicit';
    left.gain.value = right.gain.value = 0;
    source.connect(left); source.connect(right);
    left.connect(merger, 0, 0); right.connect(merger, 0, 1); merger.connect(this.engineGain);
    const voice = { source, left, right, merger, sample: definition.sample, started: this.context.currentTime };
    voices.set(index, voice);
    source.onended = () => {
      voices.delete(index); source.disconnect(); left.disconnect(); right.disconnect(); merger.disconnect();
    };
    source.start();
    if (voices === this.spatialVoices) this.spatialStarts++;
    else this.effectStarts++;
    return voice;
  }
  stopSpatial(voices = this.spatialVoices) {
    for (const voice of voices.values()) {
      voice.source.onended = null; voice.source.stop(); voice.source.disconnect();
      voice.left.disconnect(); voice.right.disconnect(); voice.merger.disconnect();
    }
    voices.clear();
  }
  applySpatial(voice, command) {
    if (!voice || !command || command.pitch < -1 || command.pitch >= 0x4000) return;
    voice.left.gain.value = command.left / 100;
    voice.right.gain.value = command.right / 100;
    voice.source.playbackRate.value = Math.max(0, command.pitch) / 4096;
  }
  captureSpatial(dt, scene) {
    if (!this.spatialReady || !scene?.train || !scene?.camera) return;
    const current = { camera: { position: scene.camera.position, target: scene.camera.target }, cars: scene.train.cars.map(car => car.matrix) };
    const previous = this.spatialHistory ?? current;
    this.spatialPending = { dt, camera: current.camera, previous_camera: previous.camera,
      cars: current.cars, previous_cars: previous.cars, definitions: this.spatialDefinitions };
    this.spatialHistory = current;
  }
  async updateSpatial(generation) {
    const payload = this.spatialPending;
    if (!payload || !this.spatialReady) return;
    const bytes = await this.codec(73, new TextEncoder().encode(JSON.stringify(payload)));
    if (this.closed || !this.active || generation !== this.generation) return;
    const commands = JSON.parse(new TextDecoder().decode(bytes));
    this.spatialCommands = commands;
    for (let index = 0; index < commands.length; index++) {
      const command = commands[index], definition = this.spatialDefinitions[index];
      const voice = this.spatialVoices.get(index) ?? this.playSpatial(index, definition);
      if (!voice) continue;
      // The original setter does not submit invalid pitch; retain its last
      // audible parameters if the source's startup/discontinuous pose exceeds it.
      this.applySpatial(voice, command);
    }
  }
  captureEffects(dt, frame, scene) {
    if (!this.effectsReady || !scene?.camera) return;
    const camera = { position: scene.camera.position, target: scene.camera.target };
    const previous = this.effectCamera ?? camera; this.effectCamera = camera;
    // Autoplay-locked output cannot play old events later. Seed pose history
    // while locked and retain every completed step once audio is running.
    if (!this.context || this.context.state !== 'running') return;
    const rider = frame.rider_wreck;
    this.effectFrames.push({ dt, camera, previous_camera: previous,
      impact: { sampled_impact_delta: rider?.sampled_impact_delta ?? [0, 0, 0],
        quad_wreck_phase: frame.wreck?.phase === 'Riding' ? 0 : 1 },
      position: frame.rider_audio_position ?? null });
    if (this.effectFrames.length > 256) {
      this.effectFrames.length = 0; this.effectsReady = false;
      this.report('Rider effects exceeded the bounded frame queue.');
    }
  }
  async updateEffects(generation) {
    // Preserve every completed physics step: an impact may last only one
    // subframe inside a render batch. Playback state is sampled before each
    // original state update, including events following a newly started voice.
    if (!this.effectFrames.length) return;
    const frames = this.effectFrames.splice(0, 64);
    const bytes = await this.codec(77, new TextEncoder().encode(JSON.stringify({ frames, playing: this.effectVoices.size > 0 })));
    if (this.closed || !this.active || generation !== this.generation) return;
    for (const result of JSON.parse(new TextDecoder().decode(bytes))) {
      if (result.event) {
        this.effectSelections++; this.effectLast = result.event;
        if (result.event.play) this.playSpatial(0, { sample: result.event.sample, looping: false }, this.effectVoices);
      }
      this.applySpatial(this.effectVoices.get(0), result.spatial);
    }
  }
  async update(dt, input, frame, scene) {
    if (!this.ready || !this.active || this.closed) return;
    this.captureSpatial(dt, scene);
    this.captureEffects(dt, frame, scene);
    void this.music.update();
    this.elapsed += dt;
    if (this.busy || !this.context || this.context.state !== 'running') return;
    this.busy = true;
    const generation = this.generation;
    try {
      if (!await this.decode()) return;
      if (generation !== this.generation) return;
      await this.updateSpatial(generation);
      await this.updateEffects(generation);
      if (this.closed || !this.active || generation !== this.generation) return;
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
    if (!active) this.effectFrames.length = 0;
    if (!this.context) return;
    this.engineGain.gain.value = active ? 1 : 0;
    if (active || this.musicPreview) await this.resume();
    else await this.context.suspend();
  }
  // Music-menu preview keeps decoding independent of physics while engine
  // voices remain silent. The caller clears preview on menu exit/focus loss.
  async setMusicPreview(active) {
    if (this.closed) return;
    const generation = ++this.previewGeneration;
    this.musicPreview = Boolean(active);
    if (this.musicTimer !== null) { clearInterval(this.musicTimer); this.musicTimer = null; }
    if (this.musicPreview) {
      await this.resume();
      if (this.closed || !this.musicPreview || generation !== this.previewGeneration) return;
      this.musicTimer = setInterval(() => { void this.music.update(); }, 250);
      void this.music.update();
    } else if (!this.active && this.context) await this.context.suspend();
  }
  async reset() {
    if (this.closed) return;
    this.generation++;
    this.stopSources();
    this.elapsed = 0;
    this.commands = 0;
    this.lastSound = null;
    if (this.ready) await this.codec(34, new Uint8Array());
    this.effectFrames.length = 0;
    this.stopSpatial(this.effectVoices);
    if (this.effectsReady) await this.codec(78, new Uint8Array());
  }
  snapshot() {
    return { ready: this.ready, context: this.context?.state ?? 'locked', samples: this.buffers.size,
      musicPreview: this.musicPreview, voices: this.sources.size, commands: this.commands, sound: this.lastSound,
      spatial: { ready: this.spatialReady, sources: this.spatialDefinitions.length, voices: this.spatialVoices.size,
        starts: this.spatialStarts, commands: this.spatialCommands,
        clock: this.context?.currentTime ?? 0, active: this.active,
        samples: [...new Set(this.spatialDefinitions.map(definition => definition.sample))].map(name => {
          const sample = this.buffers.get(name);
          return { name, channels: sample?.info.channels, sampleRate: sample?.info.sample_rate,
            loopStart: sample?.info.loop_start, loopEnd: sample?.info.loop_end, decoded: Boolean(sample?.buffer) };
        }) }, effects: { ready: this.effectsReady, voices: this.effectVoices.size, starts: this.effectStarts,
          selections: this.effectSelections, last: this.effectLast, queuedFrames: this.effectFrames.length,
          samples: this.effectsConfig?.body_impact.map(name => ({ name, decoded: Boolean(this.buffers.get(name)?.buffer) })) ?? [] },
      music: this.music.snapshot() };
  }
  close() {
    this.generation++; this.previewGeneration++;
    this.closed = true;
    if (this.musicTimer !== null) clearInterval(this.musicTimer);
    this.musicTimer = null; this.musicPreview = false;
    this.music.close();
    this.ready = false;
    window.removeEventListener('pointerdown', this.unlock);
    window.removeEventListener('keydown', this.unlock);
    this.stopSources();
    this.stopSpatial(); this.spatialReady = false; this.spatialPending = this.spatialHistory = null;
    this.stopSpatial(this.effectVoices); this.effectsReady = false; this.effectFrames.length = 0;
    this.context?.close();
    this.buffers.clear();
  }
}
