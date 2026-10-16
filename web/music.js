// Original soundtrack catalog/shuffle and bounded ADPCM decoding run in Rust.
// Browser controls schedule only a few seconds of PCM, never an entire song.
const SETTINGS_KEY = 'atv-soundtrack-v1';
const EMPTY = new Uint8Array();
const encoder = new TextEncoder(), decoder = new TextDecoder();
const json = bytes => JSON.parse(decoder.decode(bytes));
const clamp = value => Math.max(0, Math.min(1, value));
// Stands in for the original RTC seed so each session opens on a random song.
function sessionSeed() {
  try { return globalThis.crypto.getRandomValues(new Uint32Array(1))[0] || 1; } catch { return 1 + Math.floor(Math.random() * 0xfffffffe); }
}
function storageDefault() { try { return globalThis.window?.localStorage ?? null; } catch { return null; } }

export class MusicStream {
  constructor(codec, context, destination, report = () => {}, storage = storageDefault()) {
    Object.assign(this, { codec, context, destination, report, storage });
    this.ready = false; this.closed = false; this.busy = false; this.failed = false;
    this.enabled = true; this.volume = 0.45; this.generation = 0;
    this.queue = []; this.voices = new Set(); this.scheduledUntil = 0;
    this.chunks = 0; this.underruns = 0; this.history = []; this.historyIndex = -1;
    this.song = null; this.entry = null; this.offset = 0; this.finished = true;
    this.pending = null; this.selectedIndex = -1; this.suspended = false;
    try {
      const saved = JSON.parse(storage?.getItem(SETTINGS_KEY) ?? 'null');
      if (typeof saved?.enabled === 'boolean') this.enabled = saved.enabled;
      if (Number.isFinite(saved?.volume)) this.volume = clamp(saved.volume);
      if (Array.isArray(saved?.history)) this.history = saved.history.filter(item => typeof item === 'string').slice(-64);
      if (Number.isInteger(saved?.historyIndex)) this.historyIndex = Math.max(-1, Math.min(this.history.length - 1, saved.historyIndex));
    } catch { /* Private browsing or corrupt preferences must not block music. */ }
  }
  get tracks() { return (this.library?.songs ?? []).map((song, index) => ({ ...song, index })); }
  save() {
    try { this.storage?.setItem(SETTINGS_KEY, JSON.stringify({ enabled: this.enabled, volume: this.volume, history: this.history, historyIndex: this.historyIndex })); } catch { /* Optional persistence. */ }
  }
  current(generation) { return !this.closed && generation === this.generation && this.isCurrent?.(); }
  async load({ file, files, read, isCurrent }) {
    Object.assign(this, { file, files, read, isCurrent });
    const generation = this.generation;
    const archive = files.find(entry => entry.path === '/STARTAPP.RES');
    if (!archive) throw new Error('Original music catalog archive is missing.');
    const bytes = await read(file, archive.lba * 2048, archive.bytes);
    if (!this.current(generation)) return;
    const entries = json(await this.codec(2, bytes));
    if (!this.current(generation)) return;
    const index = entries.findIndex(entry => /(^|[\\/])music\.pb$/i.test(entry.name));
    if (index < 0) throw new Error('Original music catalog is missing.');
    const catalog = await this.codec(3, bytes, index);
    if (!this.current(generation)) return;
    const library = json(await this.codec(40, catalog, sessionSeed()));
    if (!this.current(generation)) return;
    this.library = library;
    for (const song of library.songs) {
      if (!files.some(item => item.path.toLowerCase() === this.path(song))) throw new Error(`Original music file is missing: ${song.filename}`);
    }
    this.history = this.history.filter(filename => library.songs.some(song => song.filename === filename));
    this.historyIndex = Math.min(this.historyIndex, this.history.length - 1);
    this.ready = true;
  }
  path(song) { return `/AUDIO/${song.filename.replaceAll('\\', '/')}`.toLowerCase(); }
  request(action) {
    if (!this.ready || this.closed) return false;
    this.generation++; this.pending = action; this.failed = false;
    this.clearQueue(); this.finished = true; this.offset = 0;
    if (Number.isInteger(action.index)) this.selectedIndex = action.index;
    void this.update();
    return true;
  }
  select(index) {
    if (!Number.isInteger(index) || index < 0 || index >= (this.library?.songs.length ?? 0)) return false;
    return this.request({ kind: 'select', index });
  }
  next() {
    if (!this.ready || this.closed) return false;
    const cursor = this.playingHistoryIndex() + 1;
    return cursor < this.history.length ? this.request({ kind: 'history', cursor, index: this.library.songs.findIndex(song => song.filename === this.history[cursor]) }) : this.request({ kind: 'next' });
  }
  previous() {
    if (!this.ready || this.closed) return false;
    const cursor = Math.max(0, this.playingHistoryIndex() - 1);
    if (!this.history.length) return this.select(0);
    return this.request({ kind: 'history', cursor, index: this.library.songs.findIndex(song => song.filename === this.history[cursor]) });
  }
  playingSegment() {
    const now = this.context()?.currentTime ?? 0;
    return this.queue.find(segment => segment.source && segment.start <= now && segment.start + segment.buffer.duration - segment.offset > now) ?? this.queue[0];
  }
  playingHistoryIndex() {
    const filename = this.playingSegment()?.song.filename;
    return filename ? this.history.slice(0, this.historyIndex + 1).lastIndexOf(filename) : this.historyIndex;
  }
  setVolume(volume) {
    if (!Number.isFinite(volume)) return;
    this.volume = clamp(volume);
    if (this.gain) this.gain.gain.value = this.volume;
    this.save();
  }
  // Races hold the song: none during the intro and countdown (00217bb0), a
  // 1.5s fade at race over (0014f8d0); release starts the next song (GO).
  hold(fade = 0) {
    if (this.held || this.closed) { this.held = true; return; }
    this.held = true;
    const context = this.context(), stop = () => { if (!this.held) return; this.clearQueue(); this.finished = true; if (this.gain) this.gain.gain.value = this.volume; };
    if (fade > 0 && this.gain && context?.state === 'running' && this.queue.length) {
      const now = context.currentTime, remaining = Math.max(0, this.scheduledUntil - now);
      const seconds = remaining < fade ? remaining * 0.8 : fade;
      this.gain.gain.cancelScheduledValues(now); this.gain.gain.setValueAtTime(this.volume, now); this.gain.gain.linearRampToValueAtTime(0, now + seconds);
      this.holdTimer = setTimeout(stop, seconds * 1000);
    } else stop();
  }
  release() {
    if (!this.held) return;
    this.held = false; clearTimeout(this.holdTimer);
    if (this.gain) { this.gain.gain.cancelScheduledValues(0); this.gain.gain.value = this.volume; }
    void this.update();
  }
  setEnabled(enabled) {
    enabled = Boolean(enabled);
    if (enabled === this.enabled || this.closed) return;
    this.enabled = enabled;
    if (!enabled) this.pauseQueue();
    this.save();
    if (enabled) void this.update();
  }
  stopSource(segment) {
    if (!segment.source) return;
    segment.source.onended = null; segment.source.stop(); segment.source.disconnect();
    this.voices.delete(segment.source); segment.source = null;
  }
  clearQueue() {
    for (const segment of this.queue) this.stopSource(segment);
    this.queue = []; this.scheduledUntil = 0; this.suspended = false;
  }
  pauseQueue() {
    const now = this.context()?.currentTime ?? 0;
    for (const segment of this.queue) {
      if (segment.source) segment.offset += Math.max(0, Math.min(segment.buffer.duration - segment.offset, now - segment.start));
      this.stopSource(segment);
    }
    this.queue = this.queue.filter(segment => segment.offset < segment.buffer.duration);
    this.suspended = true; this.scheduledUntil = 0;
  }
  resumeQueue(context) {
    if (!this.suspended) return;
    this.scheduledUntil = context.currentTime + 0.1;
    for (const segment of this.queue) this.schedule(segment, context);
    this.suspended = false;
  }
  schedule(segment, context) {
    segment.start = this.scheduledUntil;
    const source = context.createBufferSource(); source.buffer = segment.buffer; source.connect(this.gain);
    segment.source = source; this.voices.add(source);
    source.onended = () => {
      this.voices.delete(source); source.disconnect();
      this.queue = this.queue.filter(item => item !== segment);
    };
    source.start(segment.start, segment.offset);
    this.scheduledUntil += segment.buffer.duration - segment.offset;
  }
  async beginSong(action, generation) {
    const request = Number.isInteger(action.index) ? encoder.encode(JSON.stringify({ select: action.index })) : EMPTY;
    let song = json(await this.codec(41, request));
    if (!this.current(generation)) return false;
    // On a fresh page, avoid repeating the persisted last track even if the
    // randomly seeded shuffle happens to start on it.
    if (!this.song && !Number.isInteger(action.index) && this.library.songs.length > 1 && song?.filename === this.history[this.historyIndex]) {
      song = json(await this.codec(41, EMPTY));
      if (!this.current(generation)) return false;
    }
    if (!song) { this.ready = false; return false; }
    const entry = this.files.find(item => item.path.toLowerCase() === this.path(song));
    if (!entry) throw new Error(`Original music file is missing: ${song.filename}`);
    const prefix = await this.read(this.file, entry.lba * 2048, Math.min(2016, entry.bytes));
    if (!this.current(generation)) return false;
    const info = json(await this.codec(42, prefix, entry.bytes));
    if (!this.current(generation)) return false;
    this.info = info; this.song = song; this.entry = entry; this.offset = 0; this.finished = false;
    this.selectedIndex = this.library.songs.findIndex(item => item.filename === song.filename);
    if (action.kind === 'history') this.historyIndex = action.cursor;
    else {
      this.history = this.history.slice(0, this.historyIndex + 1);
      this.history.push(song.filename); this.history = this.history.slice(-64); this.historyIndex = this.history.length - 1;
    }
    this.save();
    return true;
  }
  async update() {
    const context = this.context();
    if (!this.ready || this.closed || !this.enabled || this.held || this.failed || this.busy || context?.state !== 'running') return;
    this.busy = true;
    const generation = this.generation;
    try {
      if (!this.gain) { this.gain = context.createGain(); this.gain.gain.value = this.volume; this.gain.connect(this.destination()); }
      this.resumeQueue(context);
      let iterations = 0;
      while (this.scheduledUntil - context.currentTime < 3 && iterations++ < 8) {
        if (!this.current(generation) || !this.enabled || context.state !== 'running') return;
        if (this.finished) {
          const action = this.pending ?? { kind: 'next' }; this.pending = null;
          if (!await this.beginSong(action, generation)) return;
        }
        const count = Math.min(65536, this.info.data_bytes - this.offset);
        if (count === 0) { this.finished = true; continue; }
        const bytes = await this.read(this.file, this.entry.lba * 2048 + this.info.data_offset + this.offset, count);
        if (!this.current(generation)) return;
        const pcm = await this.codec(43, bytes);
        if (!this.current(generation)) return;
        const trackOffset = this.offset / 16 * 28 / this.info.channels / this.info.sample_rate;
        this.offset += count;
        this.finished = this.offset === this.info.data_bytes || pcm.byteLength === 0;
        if (pcm.byteLength === 0) continue;
        const frames = pcm.byteLength / 2 / this.info.channels;
        const buffer = context.createBuffer(this.info.channels, frames, this.info.sample_rate);
        const values = new DataView(pcm.buffer, pcm.byteOffset, pcm.byteLength);
        for (let channel = 0; channel < this.info.channels; channel++) {
          const target = buffer.getChannelData(channel);
          for (let i = 0; i < frames; i++) target[i] = values.getInt16((i * this.info.channels + channel) * 2, true) / 32768;
        }
        const segment = { buffer, source: null, offset: 0, trackOffset, index: this.selectedIndex, song: this.song, duration: this.info.frames_max / this.info.sample_rate };
        if (!this.enabled) { this.queue.push(segment); this.suspended = true; return; }
        // On/off may have changed while a fetch or decode was pending.
        this.resumeQueue(context);
        this.queue.push(segment);
        if (this.scheduledUntil < context.currentTime || this.scheduledUntil === 0) {
          if (this.chunks > 0 && this.scheduledUntil > 0) this.underruns++;
          this.scheduledUntil = context.currentTime + 0.1;
        }
        this.schedule(segment, context); this.chunks++;
      }
    } catch (error) { if (this.current(generation)) { this.failed = true; this.report(`Music: ${error.message}`); } }
    finally { this.busy = false; if (!this.closed && generation !== this.generation) void this.update(); }
  }
  snapshot() {
    const now = this.context()?.currentTime ?? 0;
    const playing = this.playingSegment();
    const song = playing?.song ?? this.song;
    return { ready: this.ready, failed: this.failed, enabled: this.enabled, volume: this.volume, songs: this.library?.songs.length ?? 0,
      selectedIndex: this.selectedIndex, playingIndex: playing?.index ?? this.selectedIndex, title: song?.title ?? null, artist: song?.artist ?? null,
      positionSeconds: playing ? playing.trackOffset + playing.offset + (playing.source ? Math.max(0, now - playing.start) : 0) : 0,
      durationSeconds: playing?.duration ?? (this.info ? this.info.frames_max / this.info.sample_rate : 0),
      chunks: this.chunks, underruns: this.underruns,
      queuedSeconds: this.suspended ? this.queue.reduce((sum, segment) => sum + segment.buffer.duration - segment.offset, 0) : Math.max(0, this.scheduledUntil - now), voices: this.voices.size };
  }
  close() {
    this.generation++; this.closed = true; this.ready = false;
    this.clearQueue(); this.gain?.disconnect();
  }
}
