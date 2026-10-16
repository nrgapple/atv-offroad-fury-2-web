// Recovered rider vocals, chassis impacts, terrain drive-on loops and venue
// world sounds. Rust (ops 171-174) chooses every sample, volume and pitch;
// this module only owns a bounded set of Web Audio voices.
const encoder = new TextEncoder(), decoder = new TextDecoder();
const json = bytes => JSON.parse(decoder.decode(bytes));
// Decoded vocal clips kept for reuse; each gender bank has about 50.
export const MAX_DECODED_VOCALS = 24;
const QUEUE = 64;

// Persistent left/right gain pair per voice; only the buffer source changes.
export function makeChain(context, destination, label = 'quad') {
  const left = context.createGain(), right = context.createGain(), merger = context.createChannelMerger(2);
  merger.atvVoice = label; // lets instrumented tests attribute output
  left.channelCount = right.channelCount = 1;
  left.channelCountMode = right.channelCountMode = 'explicit';
  left.gain.value = right.gain.value = 0;
  left.connect(merger, 0, 0); right.connect(merger, 0, 1); merger.connect(destination);
  return { left, right, merger, source: null, ended: true, sample: null, starts: 0 };
}
export function setGains(chain, gains) {
  if (!chain) return;
  chain.left.gain.value = gains ? gains.left : 0;
  chain.right.gain.value = gains ? gains.right : 0;
  if (chain.source && gains && Number.isFinite(gains.rate) && gains.rate > 0) chain.source.playbackRate.value = Math.min(16, gains.rate);
}
export function stopChain(chain) {
  if (!chain?.source) return;
  const source = chain.source; chain.source = null; chain.ended = true;
  source.onended = null; try { source.stop(); } catch { /* already stopped */ }
  source.disconnect();
}
export function startChain(context, chain, sample, { loop = false, rate = 1 } = {}) {
  stopChain(chain);
  const source = context.createBufferSource();
  source.buffer = sample.buffer;
  // Original looping requests use the VAG loop markers; unmarked clips end.
  if (loop && sample.info?.loop_start != null && sample.info?.loop_end != null) {
    source.loop = true;
    source.loopStart = sample.info.loop_start / sample.info.sample_rate;
    source.loopEnd = sample.info.loop_end / sample.info.sample_rate;
  }
  source.playbackRate.value = rate;
  source.connect(chain.left); source.connect(chain.right);
  chain.source = source; chain.ended = false; chain.starts++;
  source.onended = () => { if (chain.source === source) { chain.source = null; chain.ended = true; source.disconnect(); } };
  source.start();
  return source;
}
const playing = chain => Boolean(chain && !chain.ended);

export class QuadSounds {
  constructor(audio) {
    this.audio = audio;
    this.frames = []; this.busy = false; this.ready = false; this.venueReady = false;
    this.vocalNames = new Set(); this.decoded = new Map(); this.vocalPending = null;
    this.chains = null; this.drive = [null, null]; this.driveSamples = [];
    this.ambient = []; this.ambientVoices = new Map(); this.soultree = []; this.soultreeVoices = new Map();
    this.camera = null; this.cueToken = 0;
    this.stats = { vocals: 0, chassis: 0, drive: 0, ambient: 0, soultree: 0, decodes: 0, dropped: 0, lastVocal: null, lastChassis: null, lastDrive: null, history: [] };
  }
  get context() { return this.audio.context; }
  ensureChains() {
    if (this.chains || !this.context) return this.chains;
    const bus = this.audio.engineGain;
    this.chains = { vocal: makeChain(this.context, bus, 'vocal'), chassis: makeChain(this.context, bus, 'chassis'), drive: [makeChain(this.context, bus, 'drive'), makeChain(this.context, bus, 'drive')] };
    return this.chains;
  }
  // rider.aud and vehicle.aud from AUDIOTBL.RES; an older content package
  // without the vocal/chassis members stays silent.
  async configure({ rider, vehicle, female, isCurrent }) {
    try {
      const config = json(await this.audio.codec(171, encoder.encode(JSON.stringify({ rider: decoder.decode(rider), vehicle: decoder.decode(vehicle), female }))));
      for (const name of config.chassis) { if (!isCurrent() || this.audio.closed) return; await this.audio.loadSample(name).catch(() => {}); }
      for (const name of config.vocals) if (await this.audio.findSample?.(name)) this.vocalNames.add(name);
      if (!isCurrent() || this.audio.closed) return;
      this.ready = true;
    } catch (error) { if (isCurrent() && !this.audio.closed) this.audio.report(`Rider sounds: ${error.message}`); }
  }
  async loadVenue(aud) {
    this.stopWorld(); this.venueReady = false; this.ambient = []; this.soultree = []; this.driveSamples = [];
    try {
      const venue = json(await this.audio.codec(172, encoder.encode(JSON.stringify({ aud: aud ?? null }))));
      const names = new Set([...venue.drive, ...venue.ambient.map(a => a.sample), ...venue.soultree.map(s => s.sample)]);
      for (const name of names) { if (this.audio.closed) return; await this.audio.loadSample(name).catch(() => {}); }
      const rates = {};
      for (const name of names) { const sample = this.audio.buffers.get(name); if (sample) rates[name] = sample.info.sample_rate; }
      // Sources whose sample an older content package lacks are dropped.
      const kept = json(await this.audio.codec(172, encoder.encode(JSON.stringify({ aud: null, rates })), 1));
      Object.assign(this, { ambient: kept.ambient, soultree: kept.soultree, driveSamples: venue.drive.filter(name => rates[name]), venueReady: true });
    } catch (error) { if (!this.audio.closed) this.audio.report(`Venue sounds: ${error.message}`); }
  }
  capture(dt, frame, scene) {
    if ((!this.ready && !this.venueReady) || !scene?.camera || !this.context || this.context.state !== 'running') return;
    const camera = { position: scene.camera.position, target: scene.camera.target };
    const previous = this.camera ?? camera; this.camera = camera;
    const rider = frame.rider_wreck;
    this.frames.push({ dt, camera, previous_camera: previous, rider_position: frame.rider_audio_position ?? null,
      rider_impact: rider?.sampled_impact_delta ?? [0, 0, 0], rider_water: Boolean(rider?.water_touching),
      quad: frame.vehicle_kind === 'motorcycle' ? null : frame.quad_audio ?? null });
    if (this.frames.length > QUEUE) { this.frames.shift(); this.stats.dropped++; }
  }
  async update(generation) {
    if (this.busy || !this.frames.length) return;
    this.busy = true;
    try {
      const frames = this.frames.splice(0, QUEUE), chains = this.ensureChains();
      const world_dt = frames.reduce((sum, f) => sum + f.dt, 0);
      const request = { frames, world_dt, vocal_playing: playing(chains?.vocal) || Boolean(this.vocalPending),
        chassis_playing: playing(chains?.chassis), drive_playing: [0, 1].map(side => playing(chains?.drive[side])),
        ambient_playing: this.ambient.map((_, i) => playing(this.ambientVoices.get(i))),
        soultree_playing: this.soultree.map((_, i) => playing(this.soultreeVoices.get(i))) };
      const result = json(await this.audio.codec(173, encoder.encode(JSON.stringify(request))));
      if (this.audio.closed || generation !== this.audio.generation || !chains) return;
      for (const frame of result.frames) this.apply(frame);
      setGains(chains.vocal, result.spatial.vocal);
      setGains(chains.chassis, result.spatial.chassis);
      for (const side of [0, 1]) setGains(chains.drive[side], result.spatial.drive?.[side]);
      if (result.world) this.applyWorld(result.world);
    } catch (error) { if (!this.audio.closed) this.audio.report(`Quad sounds: ${error.message}`); }
    finally { this.busy = false; }
  }
  apply(frame) {
    const { chains, context } = this;
    if (frame.vocal) this.playVocal(frame.vocal);
    if (frame.chassis) {
      const sample = this.audio.buffers.get(frame.chassis.sample);
      if (sample?.buffer) { startChain(context, chains.chassis, sample); this.stats.chassis++; this.stats.lastChassis = frame.chassis; this.stats.history = [...this.stats.history.slice(-7), frame.chassis.sample]; }
    }
    for (const side of [0, 1]) for (const command of frame.drive?.[side] ?? []) {
      if (command.action === 'stop') stopChain(chains.drive[side]);
      else if (command.action === 'start') {
        const sample = this.audio.buffers.get(command.sample);
        if (sample?.buffer) { startChain(context, chains.drive[side], sample, { loop: true }); this.stats.drive++; this.stats.lastDrive = command; }
      }
    }
  }
  // A Fall yell is stopped by the first detached contact (002ccc60).
  playVocal(cue) {
    if (cue.stop_current) { stopChain(this.chains.vocal); this.vocalPending = null; }
    if (!this.vocalNames.has(cue.sample)) return;
    const token = ++this.cueToken; this.vocalPending = cue.sample;
    void this.vocalBuffer(cue.sample).then(sample => {
      if (token !== this.cueToken || this.audio.closed || !sample) return;
      startChain(this.context, this.chains.vocal, sample);
      this.stats.vocals++; this.stats.lastVocal = cue.sample; this.stats.history = [...this.stats.history.slice(-7), cue.sample];
    }).catch(error => this.audio.report(`Rider vocal: ${error.message}`))
      .finally(() => { if (token === this.cueToken) this.vocalPending = null; });
  }
  // Vocal banks stay encoded until a clip is first chosen; a small LRU keeps
  // decoded clips so mobile memory does not hold the whole bank.
  async vocalBuffer(name) {
    let sample = this.decoded.get(name);
    if (!sample) {
      const found = await this.audio.findSample(name);
      if (!found) return null;
      const vag = await this.audio.codec(3, found.bytes, found.index);
      const info = json(await this.audio.codec(5, vag)), wave = await this.audio.codec(4, vag);
      const buffer = await this.context.decodeAudioData(wave.buffer.slice(wave.byteOffset, wave.byteOffset + wave.byteLength));
      sample = { buffer, info }; this.stats.decodes++;
    }
    this.decoded.delete(name); this.decoded.set(name, sample);
    while (this.decoded.size > MAX_DECODED_VOCALS) this.decoded.delete(this.decoded.keys().next().value);
    return sample;
  }
  applyWorld(world) {
    const { context } = this, bus = this.audio.engineGain;
    for (const command of world.ambient) {
      const sample = this.audio.buffers.get(command.sample);
      if (!sample?.buffer) continue;
      // Ambient sounds are unregistered 2D emitters at their full volume.
      let voice = this.ambientVoices.get(command.source);
      if (!voice) { voice = makeChain(context, bus, 'ambient'); voice.left.gain.value = voice.right.gain.value = 1; this.ambientVoices.set(command.source, voice); }
      startChain(context, voice, sample, { loop: command.looping }); this.stats.ambient++;
    }
    for (const command of world.soultree) {
      if (command.action === 'stop') { const voice = this.soultreeVoices.get(command.source); stopChain(voice); voice?.merger.disconnect(); this.soultreeVoices.delete(command.source); continue; }
      const sample = this.audio.buffers.get(command.sample);
      if (!sample?.buffer) continue;
      const voice = this.soultreeVoices.get(command.source) ?? makeChain(context, bus, 'soultree');
      this.soultreeVoices.set(command.source, voice);
      startChain(context, voice, sample, { loop: command.looping }); this.stats.soultree++;
    }
    for (const [index, command] of world.spatial) {
      const voice = this.soultreeVoices.get(index);
      if (voice) setGains(voice, command.pitch < -1 || command.pitch >= 0x4000 ? null : { left: command.left / 100, right: command.right / 100, rate: Math.max(0, command.pitch) / 4096 });
    }
  }
  stopWorld() {
    for (const voices of [this.ambientVoices, this.soultreeVoices]) { for (const voice of voices.values()) { stopChain(voice); voice.merger.disconnect(); } voices.clear(); }
  }
  async reset() {
    this.frames.length = 0; this.cueToken++; this.vocalPending = null;
    if (this.chains) { stopChain(this.chains.chassis); for (const chain of this.chains.drive) stopChain(chain); }
    if (this.ready) await this.audio.codec(174, new Uint8Array());
  }
  close() {
    this.frames.length = 0; this.cueToken++; this.stopWorld();
    if (this.chains) for (const chain of [this.chains.vocal, this.chains.chassis, ...this.chains.drive]) { stopChain(chain); chain.merger.disconnect(); }
    this.chains = null; this.decoded.clear();
  }
  snapshot() {
    const c = this.chains;
    return { ready: this.ready, venue: this.venueReady, vocalSamples: this.vocalNames.size, decodedVocals: this.decoded.size, queued: this.frames.length,
      voices: { vocal: playing(c?.vocal), chassis: playing(c?.chassis), drive: [0, 1].map(s => playing(c?.drive[s])),
        ambient: [...this.ambientVoices.values()].filter(playing).length, soultree: [...this.soultreeVoices.values()].filter(playing).length },
      ambientSources: this.ambient.length, soultreeSources: this.soultree.length, driveSamples: this.driveSamples.length, ...this.stats, history: [...this.stats.history] };
  }
}
