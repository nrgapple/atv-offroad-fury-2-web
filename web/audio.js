// Original decoded samples; selection and transition rules run in Rust.
// Browser resampling/mixing is not a bit-exact SPU2 implementation.
import { MusicStream } from './music.js';
export class GameAudio {
  constructor(codec, report = () => {}) {
    this.codec = codec;
    this.report = report;
    this.context = null;
    this.buffers = new Map(); this.oneShotPlays = {}; this.stadiumVoices = new Map(); this.stadiumDefinitions = [];
    this.sources = new Set();
    this.current = null;
    this.ready = false;
    this.active = true;
    this.menuTracks = []; this.menuSource = null; this.menuMusicMode = false;
    this.musicPreview = false;
    this.musicTimer = null;
    this.previewGeneration = 0;
    this.closed = false;
    this.busy = false;
    this.elapsed = 0;
    this.commands = 0;
    this.generation = 0;
    this.lastSound = null;
    this.remoteVoices = new Map(); this.remoteEffects = new Map(); this.remoteLoaded = new Map(); this.remoteStarts = 0;
    this.weatherActive = false; this.weatherSource = null; this.weatherStarts = 0;
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
    if (this.musicPreview && this.menuMusicMode) void this.playMenuMusic();
  }
  async load({ file, files, read, isCurrent, quadId = 1 }) {
    this.stopWeather();this.weatherActive=false;
    const text = new TextDecoder();
    const json = async (op, bytes, index = 0) => JSON.parse(text.decode(await this.codec(op, bytes, index)));
    const archive = async path => {
      const entry = files.find(item => item.path.toLowerCase() === path.toLowerCase());
      if (!entry) throw new Error(`Original audio archive is missing: ${path}`);
      const bytes = await read(file, entry.lba * 2048, entry.bytes);
      return { bytes, entries: await json(2, bytes) };
    };
    const motorcycle = quadId >= 27;
    const selection = motorcycle ? await json(89, new Uint8Array([0]), quadId) : null;
    const size = motorcycle ? Number(selection.quad.match(/BIKE(\d+)\.RES$/)[1]) : null;
    const table = await archive(motorcycle ? selection.quad : '/AUDIOTBL.RES');
    const tableIndex = table.entries.findIndex(entry => motorcycle ? entry.name.replaceAll('\\','/').split('/').at(-1).toLowerCase() === `vehicle_${size}a.aud` : /(^|[\\/])vehicle\.aud$/i.test(entry.name));
    if (tableIndex < 0) throw new Error('Original vehicle audio table is missing.');
    this.engineTable = await this.codec(3, table.bytes, tableIndex);
    this.config = await json(motorcycle ? 101 : 32, this.engineTable, quadId);
    const sounds = motorcycle ? table : await archive('/AUDIO/AUDIO.RES');
    const normal = path => path.replaceAll('\\', '/').toLowerCase().replace(/^(?:audio|aud)\//, '');
    const lookup = new Map(sounds.entries.map((entry, index) => [normal(entry.name), index]));
    // Motorcycles use their own engine bank, but the scene (train, rain) is
    // still ATV2's: samples missing from the bike bank come from AUDIO.RES.
    let sceneBank = null;
    const find = async key => {
      const own = lookup.get(key) ?? lookup.get(`${key}.vag`);
      if (own !== undefined) return { bytes: sounds.bytes, index: own };
      if (!motorcycle) return null;
      sceneBank ??= archive('/AUDIO/AUDIO.RES').then(bank => ({
        bytes: bank.bytes,
        lookup: new Map(bank.entries.map((entry, index) => [normal(entry.name), index]))
      }));
      const bank = await sceneBank;
      const index = bank.lookup.get(key) ?? bank.lookup.get(`${key}.vag`);
      return index === undefined ? null : { bytes: bank.bytes, index };
    };
    this.loadSample = async name => {
      if (this.buffers.has(name)) return;
      const found = await find(normal(name));
      if (!found) throw new Error(`Original sound sample is missing: ${name}`);
      const vag = await this.codec(3, found.bytes, found.index);
      const info = await json(5, vag);
      const wave = await this.codec(4, vag);
      if (!isCurrent() || this.closed) return;
      this.buffers.set(name, { wave, info, buffer: null });
    };
    this.loadWeatherSample = () => this.loadSample('rain.vag');
    // Keep encoded WAV until a user gesture creates/unlocks the audio context.
    for (const name of new Set([...this.config.samples.flat(),...(this.config.impact_samples ?? [])])) {
      if (!isCurrent() || this.closed) return;
      await this.loadSample(name);
    }
    // Race start lamps and the Supercross gate; an older content package
    // without them rides on silently.
    for (const name of ['beep01.vag', 'go.vag', 'gate.vag']) {
      if (!isCurrent() || this.closed) return;
      await this.loadSample(name).catch(() => {});
    }
    const riderIndex = table.entries.findIndex(entry => /(^|[\\/])rider\.aud$/i.test(entry.name));
    if (!motorcycle && riderIndex < 0) throw new Error('Original rider sound table is missing.');
    if (!motorcycle) await this.loadEffects({ bytes: await this.codec(3, table.bytes, riderIndex), isCurrent });
    if (!isCurrent() || this.closed) return;
    await this.music.load({ file, files, read, isCurrent });
    await this.loadMenuMusic({ file, files, read, isCurrent });
    if (!isCurrent() || this.closed) return;
    this.ready = true;
  }
  async loadMenuMusic({ file, files, read, isCurrent }) {
    if (this.menuTracks.length || this.closed) return;
    const menu = files.find(item => item.path === '/UI/RESOURCE/PAGEA/PAGE07.RES');
    if (!menu) return; // Existing manifests remain playable during R2 publication.
    const json = async (op, bytes, index = 0) => JSON.parse(new TextDecoder().decode(await this.codec(op, bytes, index)));
    const bytes = await read(file, menu.lba * 2048, menu.bytes);
    const entries = await json(2, bytes);
    const definition = entries.findIndex(e => /(^|[\\/])uimusic\.aud$/i.test(e.name));
    if (definition < 0) throw new Error('Original menu music definition missing');
    const doc = await json(54, await this.codec(3, bytes, definition));
    const values = Object.fromEntries(doc.sections.find(s => s.name.toLowerCase() === 'uimusic').fields.map(f => [f.key.toLowerCase(), f.value.replaceAll('\\', '/')]));
    if (Number(values.numtracks) !== 4) throw new Error('Original UI expected four tunes');
    const audio = files.find(item => item.path === '/AUDIO/AUDIO.RES');
    if (!audio) throw new Error('Original menu sound archive missing');
    const bank = await read(file, audio.lba * 2048, audio.bytes), samples = await json(2, bank), tracks = [];
    for (let i = 1; i <= 4; i++) {
      const pair = [], info = [];
      for (const channel of ['l', 'r']) {
        const suffix = `${values['track'+i]}${channel}.vag`.toLowerCase();
        const index = samples.findIndex(e => e.name.replaceAll('\\', '/').toLowerCase().endsWith('/'+suffix));
        if (index < 0) throw new Error(`Original UI tune missing: ${suffix}`);
        const vag = await this.codec(3, bank, index);
        info.push(await json(5,vag));
        pair.push(await this.codec(4,vag));
        if (!isCurrent() || this.closed) return;
      }
      tracks.push({name:values['track'+i], waves:pair, info});
    }
    this.menuTracks = tracks;
    if (this.musicPreview && this.menuMusicMode) void this.playMenuMusic();
  }
  stopMenuMusic() {
    if (!this.menuSource) return;
    this.menuSource.stop(); this.menuSource.disconnect(); this.menuSource = null;
  }
  async playMenuMusic() {
    if (this.menuSource || this.menuBusy || !this.menuTracks.length || !this.musicPreview || !this.menuMusicMode || !this.music.enabled || this.context?.state !== 'running') return;
    this.menuBusy = true; const generation = this.previewGeneration;
    try {
      const track = this.menuTracks[0];
      if (!track.buffer) {
        const pair = await Promise.all(track.waves.map(wave => this.context.decodeAudioData(wave.slice().buffer)));
        if (this.closed) return;
        if (pair[0].sampleRate !== pair[1].sampleRate || pair[0].length !== pair[1].length) throw new Error('Menu stereo channels differ');
        track.buffer = this.context.createBuffer(2,pair[0].length,pair[0].sampleRate);
        pair.forEach((buffer,i) => track.buffer.copyToChannel(buffer.getChannelData(0),i));
      }
      if (this.closed || generation !== this.previewGeneration || !this.musicPreview || !this.menuMusicMode || !this.music.enabled) return;
      this.music.pauseQueue();
      this.menuGain ??= this.context.createGain(); this.menuGain.gain.value = this.music.volume;
      this.menuGain.disconnect(); this.menuGain.connect(this.gain);
      const source = this.context.createBufferSource(); source.buffer = track.buffer; source.loop = true; source.loopStart = (track.info[0].loop_start ?? 0) / track.info[0].sample_rate; source.loopEnd = track.info[0].loop_end == null ? track.buffer.duration : track.info[0].loop_end / track.info[0].sample_rate; source.connect(this.menuGain); source.start(); this.menuSource = source;
    } catch (error) { if (!this.closed) this.report(`Menu music: ${error.message}`); }
    finally { this.menuBusy = false; if (generation !== this.previewGeneration && !this.closed) void this.playMenuMusic(); }
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
  async loadWeather({ selection, isCurrent }) {
    this.stopWeather();this.weatherActive=Boolean(selection?.active&&selection.definition?.kind==='rain');
    if(!this.weatherActive)return;
    try {await this.loadWeatherSample();if(!isCurrent()||this.closed)this.weatherActive=false;}
    catch(error){if(isCurrent()&&!this.closed){this.weatherActive=false;this.report(`Rain sound: ${error.message}`);}}
  }
  stopWeather(){if(this.weatherSource){this.weatherSource.stop();this.weatherSource.disconnect();this.weatherSource=null;}}
  playWeather(){
    const sample=this.buffers.get('rain.vag');
    if(!this.weatherActive||this.weatherSource||!sample?.buffer||this.closed)return;
    //00213bf0 requests rain.vag with0x81; 0x80 is the recovered looping bit.
    const source=this.context.createBufferSource();source.buffer=sample.buffer;source.loop=true;
    if(sample.info.loop_start!==null&&sample.info.loop_end!==null){source.loopStart=sample.info.loop_start/sample.info.sample_rate;source.loopEnd=sample.info.loop_end/sample.info.sample_rate;}
    source.connect(this.engineGain);source.start();this.weatherSource=source;this.weatherStarts++;
  }
  // CrowdPresent stadium: the crowd reaction clips (one crowd voice) and the
  // venue's positional Soultree loops through the same stereo commands.
  async loadStadium({ soultree, crowd }) {
    this.stopCrowd(); this.stopSpatial(this.stadiumVoices); this.stadiumDefinitions = [];
    for (const name of new Set([...crowd, ...soultree.map(definition => definition.sample)])) {
      if (this.closed) return;
      await this.loadSample(name).catch(() => {});
    }
    this.stadiumDefinitions = soultree.filter(definition => this.buffers.has(definition.sample));
  }
  crowdCue({ cue, stop_loop }) {
    if (!this.context || this.context.state !== 'running' || this.closed) return;
    if (stop_loop && this.crowdVoice?.looping) this.stopCrowd();
    if (!cue) return;
    // 002cc370: a playing clip keeps the voice; a Stomp loop gives way.
    if (this.crowdVoice && !this.crowdVoice.looping) return;
    const sample = this.buffers.get(cue.sample);
    if (!sample?.buffer) return;
    this.stopCrowd();
    const source = this.context.createBufferSource(); source.buffer = sample.buffer; source.loop = cue.looping;
    source.connect(this.engineGain); source.start();
    const voice = { source, looping: cue.looping }; this.crowdVoice = voice; this.crowdCues = (this.crowdCues ?? 0) + 1;
    source.onended = () => { if (this.crowdVoice === voice) this.crowdVoice = null; source.disconnect(); };
  }
  stopCrowd() { if (this.crowdVoice) { const { source } = this.crowdVoice; this.crowdVoice = null; try { source.stop(); } catch {} source.disconnect(); } }
  async updateStadium(generation, scene) {
    if (!this.stadiumDefinitions?.length || !scene?.camera) return;
    const camera = { position: scene.camera.position, target: scene.camera.target };
    const previous = this.stadiumCamera ?? camera; this.stadiumCamera = camera;
    const payload = { dt: 1 / 60, camera, previous_camera: previous, cars: [], previous_cars: [], definitions: this.stadiumDefinitions };
    const commands = JSON.parse(new TextDecoder().decode(await this.codec(73, new TextEncoder().encode(JSON.stringify(payload)))));
    if (this.closed || !this.active || generation !== this.generation) return;
    commands.forEach((command, index) => {
      const voice = this.stadiumVoices.get(index) ?? this.playSpatial(index, this.stadiumDefinitions[index], this.stadiumVoices);
      if (voice) this.applySpatial(voice, command);
    });
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
  async loadRemoteModels(players) {
    if (!this.engineTable || !this.loadSample || this.closed) return;
    for (const player of players) {
      if (this.remoteLoaded.has(player.atv_model)) { await this.remoteLoaded.get(player.atv_model); continue; }
      const pending = (async () => {
        const config = JSON.parse(new TextDecoder().decode(await this.codec(87,this.engineTable,player.atv_model)));
        for (const name of new Set(config.samples.flat())) { if(this.closed)return; await this.loadSample(name); }
        if (!this.closed) await this.decode();
      })();
      this.remoteLoaded.set(player.atv_model,pending);
      await pending;
    }
  }
  remotePlaying() {
    return [...this.remoteVoices].map(([peer,voice])=>({peer,duration:voice.source.buffer.duration,playhead:Math.max(0,(this.context?.currentTime??0)-voice.started)}));
  }
  stopRemote(map,key) {
    const voice=map.get(key); if(!voice)return;
    voice.source.onended=null;voice.source.stop();voice.source.disconnect();voice.left.disconnect();voice.right.disconnect();voice.merger.disconnect();map.delete(key);
  }
  remoteFrame(peers) {
    const ids=new Set(peers.filter(p=>!p.stale).map(p=>p.peer)), voiced=new Set(peers.filter(p=>!p.stale&&!p.muted).map(p=>p.peer));
    // Rust keeps engine voices only for the nearest riders in a full room.
    for(const key of this.remoteVoices.keys()) if(!voiced.has(key))this.stopRemote(this.remoteVoices,key);
    for(const key of this.remoteEffects.keys()) if(!ids.has(key))this.stopRemote(this.remoteEffects,key);
    if(!this.active || this.context?.state!=='running' || this.closed)return;
    for(const peer of peers) {
      if(peer.stale)continue;
      if(peer.engine && this.buffers.get(peer.engine.sample)?.buffer) {
        this.stopRemote(this.remoteVoices,peer.peer);
        const voice=this.playSpatial(peer.peer,{sample:peer.engine.sample,looping:peer.engine.looping},this.remoteVoices);
        if(voice)this.remoteStarts++;
      }
      const voice=this.remoteVoices.get(peer.peer);
      if(voice)this.applySpatial(voice,peer.spatial);
      if(peer.crash && this.buffers.get(peer.crash.sample)?.buffer) {
        this.stopRemote(this.remoteEffects,peer.peer);
        const impact=this.playSpatial(peer.peer,{sample:peer.crash.sample,looping:false},this.remoteEffects);
        this.applySpatial(impact,peer.crash.spatial);
      }
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
  // Short authored bass/noise accent, only on severe actual contacts.
  crashBoom(severity) {
    if (!this.active || this.closed || !this.context || this.context.state !== 'running') return;
    const now=this.context.currentTime;
    if (now-(this.boomTime ?? -Infinity)<.3) return;
    this.boomTime=now; this.boomStarts=(this.boomStarts ?? 0)+1;
    const envelope=this.context.createGain();
    envelope.gain.setValueAtTime(Math.min(.6,severity/150),now);
    envelope.gain.exponentialRampToValueAtTime(.001,now+.48);
    envelope.connect(this.engineGain);
    const bass=this.context.createOscillator();bass.type='sine';bass.frequency.setValueAtTime(95,now);bass.frequency.exponentialRampToValueAtTime(28,now+.38);bass.connect(envelope);bass.start(now);bass.stop(now+.5);
    const buffer=this.context.createBuffer(1,Math.ceil(this.context.sampleRate*.2),this.context.sampleRate),data=buffer.getChannelData(0);
    let seed=73;for(let i=0;i<data.length;i++){seed=(Math.imul(seed,1664525)+1013904223)>>>0;data[i]=(seed/2147483648-1)*.5;}
    const noise=this.context.createBufferSource();noise.buffer=buffer;
    const filter=this.context.createBiquadFilter();filter.type='lowpass';filter.frequency.value=650;noise.connect(filter);filter.connect(envelope);noise.start(now);
    bass.onended=()=>{bass.disconnect();noise.disconnect();filter.disconnect();envelope.disconnect();};
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
  oneShot(name) {
    const sample=this.buffers.get(name);
    if (!sample?.buffer || !this.context || this.context.state !== 'running') return;
    const source=this.context.createBufferSource();source.buffer=sample.buffer;source.connect(this.engineGain);
    this.oneShotPlays[name]=(this.oneShotPlays[name]??0)+1;
    const voice={source};this.sources.add(voice);
    source.onended=()=>{this.sources.delete(voice);source.disconnect();};source.start();
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
      this.playWeather();
      if (frame.vehicle_kind === 'motorcycle') {
        if (frame.impact_sample && frame.motorcycle_tick !== this.impactTick) {this.impactTick=frame.motorcycle_tick;this.oneShot(frame.impact_sample);}
        if (input.clutch && !this.clutchWasDown) this.oneShot(this.config.samples[9][0]);
        this.clutchWasDown=Boolean(input.clutch);
      }
      await this.updateSpatial(generation);
      await this.updateStadium(generation, scene);
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
  async setMusicPreview(active, originalMenu = false) {
    if (this.closed) return;
    const generation = ++this.previewGeneration;
    this.musicPreview = Boolean(active);
    this.menuMusicMode = originalMenu;
    if (!active || !originalMenu) this.stopMenuMusic();
    if (this.musicTimer !== null) { clearInterval(this.musicTimer); this.musicTimer = null; }
    if (this.musicPreview) {
      await this.resume();
      if (this.closed || !this.musicPreview || generation !== this.previewGeneration) return;
      const tick = () => {
        if (this.menuMusicMode && this.menuTracks.length) {
          this.music.pauseQueue();
          if (!this.music.enabled) this.stopMenuMusic();
          else { if (this.menuGain) this.menuGain.gain.value = this.music.volume; void this.playMenuMusic(); }
        } else void this.music.update();
      };
      this.musicTimer = setInterval(tick, 250); tick();
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
    return { ready: this.ready, context: this.context?.state ?? 'locked', samples: this.buffers.size, oneShots: { ...this.oneShotPlays },
      raceStart: ['beep01.vag', 'go.vag', 'gate.vag'].filter(name => this.buffers.has(name)),
      stadium: { loops: this.stadiumDefinitions?.length ?? 0, voices: this.stadiumVoices?.size ?? 0, crowdCues: this.crowdCues ?? 0, crowd: Boolean(this.crowdVoice) },
      weather: {active:this.weatherActive,muted:!this.active,voices:this.weatherSource?1:0,starts:this.weatherStarts,sample:'rain.vag',decoded:Boolean(this.buffers.get('rain.vag')?.buffer),sampleRate:this.buffers.get('rain.vag')?.info.sample_rate,loopStart:this.buffers.get('rain.vag')?.info.loop_start,loopEnd:this.buffers.get('rain.vag')?.info.loop_end,pcmBytes:this.buffers.get('rain.vag')?.buffer?this.buffers.get('rain.vag').buffer.length*this.buffers.get('rain.vag').buffer.numberOfChannels*4:0},
      menuMusic: {ready:this.menuTracks.length === 4,voices:this.menuSource ? 1 : 0,track:this.menuSource ? this.menuTracks[0].name : null,stereo:Boolean(this.menuTracks[0]?.buffer?.numberOfChannels === 2)},
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
      crashBooms:this.boomStarts ?? 0,
      multiplayer: {voices:this.remoteVoices.size,impacts:this.remoteEffects.size,starts:this.remoteStarts,playing:this.remotePlaying()},
      music: this.music.snapshot() };
  }
  close() {
    this.generation++; this.previewGeneration++;
    this.closed = true;
    if (this.musicTimer !== null) clearInterval(this.musicTimer);
    this.musicTimer = null; this.musicPreview = false;
    this.stopMenuMusic(); this.menuGain?.disconnect(); this.menuTracks = [];
    this.music.close();
    this.ready = false;
    window.removeEventListener('pointerdown', this.unlock);
    window.removeEventListener('keydown', this.unlock);
    this.stopSources();this.stopWeather();this.weatherActive=false;
    this.stopSpatial(this.remoteVoices);this.stopSpatial(this.remoteEffects);
    this.stopSpatial(); this.spatialReady = false; this.spatialPending = this.spatialHistory = null;
    this.stopCrowd(); this.stopSpatial(this.stadiumVoices); this.stadiumDefinitions = [];
    this.stopSpatial(this.effectVoices); this.effectsReady = false; this.effectFrames.length = 0;
    this.context?.close();
    this.buffers.clear();
  }
}
