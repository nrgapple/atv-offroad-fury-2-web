// Original soundtrack catalog/shuffle run in Rust. Songs normally stream as
// AAC through one media element (ranged reads, native decoding off the main
// thread, seekable). Without streams, bounded Rust ADPCM decoding schedules
// only a few seconds of PCM at a time, never an entire song.
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
function mediaDefault() {
  try {
    const element = typeof globalThis.Audio === 'function' ? new globalThis.Audio() : null;
    return element?.canPlayType?.('audio/mp4; codecs="mp4a.40.2"') ? element : null;
  } catch { return null; }
}
// A tiny silent WAV, played inside the first gesture so mobile browsers allow
// later song changes on the same element without another tap.
function silentUrl() {
  try {
    const bytes = new Uint8Array(44 + 800), view = new DataView(bytes.buffer);
    [0x46464952, 36 + 800, 0x45564157, 0x20746d66, 16].forEach((value, i) => view.setUint32(i * 4, value, true));
    view.setUint16(20, 1, true); view.setUint16(22, 1, true); view.setUint32(24, 8000, true); view.setUint32(28, 16000, true);
    view.setUint16(32, 2, true); view.setUint16(34, 16, true); view.setUint32(36, 0x61746164, true); view.setUint32(40, 800, true);
    return URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }));
  } catch { return null; }
}

export class MusicStream {
  constructor(codec, context, destination, report = () => {}, storage = storageDefault()) {
    Object.assign(this, { codec, context, destination, report, storage });
    this.ready = false; this.closed = false; this.busy = false; this.failed = false;
    this.enabled = true; this.volume = 0.45; this.generation = 0;
    this.queue = []; this.voices = new Set(); this.scheduledUntil = 0;
    this.chunks = 0; this.underruns = 0; this.history = []; this.historyIndex = -1;
    this.song = null; this.entry = null; this.offset = 0; this.finished = true;
    this.pending = null; this.selectedIndex = -1; this.suspended = false;
    this.mediaFactory = mediaDefault; this.media = false; this.element = null; this.streams = [];
    try {
      const saved = JSON.parse(storage?.getItem(SETTINGS_KEY) ?? 'null');
      if (typeof saved?.enabled === 'boolean') this.enabled = saved.enabled;
      if (Number.isFinite(saved?.volume)) this.volume = clamp(saved.volume);
      if (Array.isArray(saved?.history)) this.history = saved.history.filter(item => typeof item === 'string').slice(-64);
      if (Number.isInteger(saved?.historyIndex)) this.historyIndex = Math.max(-1, Math.min(this.history.length - 1, saved.historyIndex));
    } catch { /* Private browsing or corrupt preferences must not block music. */ }
  }
  // Control Center, the lock screen, headphone buttons and keyboard media keys
  // drive the soundtrack like the Jukebox transport (play/pause = Music on/off).
  bindMediaSession(session = globalThis.navigator?.mediaSession) {
    if (!session || this.session) return;
    this.session = session;
    const handlers = {
      play: () => { this.setEnabled(true); void this.update(); },
      pause: () => this.setEnabled(false),
      nexttrack: () => { this.setEnabled(true); this.next(); },
      previoustrack: () => { this.setEnabled(true); this.previous(); },
    };
    for (const [action, handler] of Object.entries(handlers)) {
      try { session.setActionHandler(action, handler); } catch { /* Unsupported action. */ }
    }
    this.syncMediaSession();
  }
  syncMediaSession() {
    const session = this.session, song = this.song;
    if (!session) return;
    try {
      if (song && song !== this.sessionSong && typeof globalThis.MediaMetadata === 'function') {
        this.sessionSong = song;
        session.metadata = new globalThis.MediaMetadata({ title: song.title ?? 'Fury Remastered', artist: song.artist ?? '', album: 'Fury Remastered soundtrack',
          artwork: [{ src: '/fury-remastered-home-512-v1.png', sizes: '512x512', type: 'image/png' }] });
      }
      session.playbackState = song && this.enabled && !this.held && !this.closed ? 'playing' : 'paused';
    } catch { /* Media Session is optional. */ }
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
    // Stream only when every song has one; an older content package keeps
    // the decoder for the whole catalog.
    this.streams = library.songs.map(song => files.find(item => item.path.toLowerCase() === this.path(song))?.stream ?? null);
    if (!this.element && this.streams.every(Boolean)) this.element = this.mediaFactory();
    this.media = Boolean(this.element);
    this.ready = true;
  }
  // Streaming element lifecycle. The element feeds the music gain, so volume,
  // race fades and the master bus behave exactly as for decoded PCM.
  attachElement(context) {
    if (this.mediaNode || !this.element) return;
    const element = this.element;
    element.preload = 'auto';
    element.addEventListener('ended', () => { if (this.closed || !this.media) return; this.finished = true; void this.update(); });
    element.addEventListener('error', () => { if (!this.closed && this.media && this.mediaWanted && element.src !== this.silent) this.fallback('stream error'); });
    element.addEventListener('waiting', () => { if (this.mediaWanted) this.underruns++; });
    this.mediaNode = context.createMediaElementSource(element);
    this.mediaNode.connect(this.gain);
    // A suspended context would leave the element running silently ahead.
    context.addEventListener?.('statechange', () => { if (context.state !== 'running') this.pauseQueue(); });
  }
  // Called synchronously from a user gesture.
  unlock() {
    if (!this.media || this.primed || this.closed || !this.element) return;
    const element = this.element, wanted = () => this.mediaWanted && this.enabled && !this.held;
    const real = Boolean(element.src) && element.src !== this.silent;
    this.primed = true;
    if (real && !element.paused) return;
    if (!real) {
      this.silent ??= silentUrl();
      if (!this.silent) return;
      element.src = this.silent;
    }
    // A song that should stay quiet (music off, race hold) is primed muted.
    const quiet = real && !wanted();
    element.muted = quiet;
    const playing = element.play();
    const settle = () => { if (element.src === this.silent || (quiet && !wanted())) element.pause(); element.muted = false; };
    playing?.then?.(settle, () => { element.muted = false; this.primed = false; });
  }
  // Switch the whole session to decoded VAG streaming and restart the song.
  fallback(reason) {
    if (!this.media) return;
    this.media = false; this.mediaWanted = false;
    try { this.element.pause(); this.element.removeAttribute('src'); this.element.load?.(); } catch { /* Element already gone. */ }
    if (globalThis.navigator?.onLine !== false) this.report(`Music stream unavailable (${reason}); decoding the original audio instead.`);
    const index = this.selectedIndex;
    this.finished = true; this.offset = 0;
    if (index >= 0) this.request({ kind: 'select', index }); else void this.update();
  }
  // Live spectrum for the Jukebox display: a passive tap on the music gain.
  analyser() {
    const context = this.context();
    if (!this.gain || !context?.createAnalyser) return null;
    if (!this.analyserNode) {
      this.analyserNode = context.createAnalyser();
      this.analyserNode.fftSize = 64; this.analyserNode.smoothingTimeConstant = 0.78;
      this.gain.connect(this.analyserNode);
    }
    return this.analyserNode;
  }
  seek(seconds) {
    if (!this.media || !this.element || !this.song || !Number.isFinite(seconds)) return false;
    const duration = this.streams[this.selectedIndex]?.seconds ?? this.element.duration;
    this.element.currentTime = Math.max(0, Math.min(seconds, Math.max(0, duration - 0.25)));
    return true;
  }
  path(song) { return `/AUDIO/${song.filename.replaceAll('\\', '/')}`.toLowerCase(); }
  request(action) {
    if (!this.ready || this.closed) return false;
    this.generation++; this.pending = action; this.failed = false;
    this.clearQueue(); this.finished = true; this.offset = 0;
    if (this.media) { this.mediaWanted = false; this.element.pause(); }
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
    if (this.media) return null;
    const now = this.context()?.currentTime ?? 0;
    return this.queue.find(segment => segment.source && segment.start <= now && segment.start + segment.buffer.duration - segment.offset > now) ?? this.queue[0];
  }
  playingHistoryIndex() {
    const filename = this.media ? this.song?.filename : this.playingSegment()?.song.filename;
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
    this.held = true; this.syncMediaSession();
    const context = this.context(), stop = () => { if (!this.held) return; this.clearQueue(); if (this.media) { this.mediaWanted = false; this.element.pause(); } this.finished = true; if (this.gain) this.gain.gain.value = this.volume; };
    const sounding = this.media ? this.mediaWanted && !this.element.paused : this.queue.length > 0;
    if (fade > 0 && this.gain && context?.state === 'running' && sounding) {
      const now = context.currentTime, remaining = this.media ? Math.max(0, (this.element.duration || Infinity) - this.element.currentTime) : Math.max(0, this.scheduledUntil - now);
      const seconds = remaining < fade ? remaining * 0.8 : fade;
      this.gain.gain.cancelScheduledValues(now); this.gain.gain.setValueAtTime(this.volume, now); this.gain.gain.linearRampToValueAtTime(0, now + seconds);
      this.holdTimer = setTimeout(stop, seconds * 1000);
    } else stop();
  }
  release() {
    if (!this.held) return;
    this.held = false; clearTimeout(this.holdTimer); this.syncMediaSession();
    if (this.gain) { this.gain.gain.cancelScheduledValues(0); this.gain.gain.value = this.volume; }
    void this.update();
  }
  setEnabled(enabled) {
    enabled = Boolean(enabled);
    if (enabled === this.enabled || this.closed) return;
    this.enabled = enabled;
    if (!enabled) this.pauseQueue();
    this.save(); this.syncMediaSession();
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
    if (this.media) { if (this.element && !this.element.paused) this.element.pause(); this.suspended = true; return; }
    const now = this.context()?.currentTime ?? 0;
    for (const segment of this.queue) {
      if (segment.source) segment.offset += Math.max(0, Math.min(segment.buffer.duration - segment.offset, now - segment.start));
      this.stopSource(segment);
    }
    this.queue = this.queue.filter(segment => segment.offset < segment.buffer.duration);
    this.suspended = true; this.scheduledUntil = 0;
  }
  resumeQueue(context) {
    if (this.media) { this.suspended = false; return; }
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
    if (this.media) {
      this.element.src = entry.stream.url;
      this.info = null;
    } else {
      const prefix = await this.read(this.file, entry.lba * 2048, Math.min(2016, entry.bytes));
      if (!this.current(generation)) return false;
      this.info = json(await this.codec(42, prefix, entry.bytes));
      if (!this.current(generation)) return false;
    }
    this.song = song; this.entry = entry; this.offset = 0; this.finished = false;
    this.selectedIndex = this.library.songs.findIndex(item => item.filename === song.filename);
    if (action.kind === 'history') this.historyIndex = action.cursor;
    else {
      this.history = this.history.slice(0, this.historyIndex + 1);
      this.history.push(song.filename); this.history = this.history.slice(-64); this.historyIndex = this.history.length - 1;
    }
    this.save(); this.syncMediaSession();
    return true;
  }
  async update() {
    const context = this.context();
    if (!this.ready || this.closed || !this.enabled || this.held || this.failed || this.busy || context?.state !== 'running') return;
    this.busy = true;
    const generation = this.generation;
    try {
      if (!this.gain) { this.gain = context.createGain(); this.gain.gain.value = this.volume; this.gain.connect(this.destination()); }
      if (this.media) { await this.updateMedia(context, generation); return; }
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
    // Offline, the soundtrack (never cached; see research/OFFLINE.md) stays silent without an error.
    } catch (error) { if (this.current(generation)) { this.failed = true; if (globalThis.navigator?.onLine !== false) this.report(`Music: ${error.message}`); } }
    finally { this.busy = false; if (!this.closed && generation !== this.generation) void this.update(); }
  }
  async updateMedia(context, generation) {
    this.attachElement(context);
    this.suspended = false;
    if (this.finished) {
      const action = this.pending ?? { kind: 'next' }; this.pending = null;
      if (!await this.beginSong(action, generation)) return;
    }
    if (!this.current(generation) || !this.enabled || this.held || context.state !== 'running') return;
    this.mediaWanted = true;
    if (!this.element.paused) return;
    try { await this.element.play(); this.chunks++; }
    catch (error) {
      // Autoplay policy: wait for the next gesture (unlock) and tick. Other
      // failures mean this browser cannot stream; decode the VAG instead.
      if (error?.name === 'NotAllowedError') { this.primed = false; return; }
      if (error?.name !== 'AbortError' && this.current(generation)) this.fallback(error?.name ?? 'play failed');
    }
  }
  snapshot() {
    if (this.media) {
      const element = this.element, stream = this.streams[this.selectedIndex], song = this.song;
      let buffered = 0;
      try { for (let i = 0; i < element.buffered.length; i++) if (element.buffered.start(i) <= element.currentTime && element.buffered.end(i) >= element.currentTime) buffered = element.buffered.end(i) - element.currentTime; } catch { /* No buffered ranges yet. */ }
      return { ready: this.ready, failed: this.failed, enabled: this.enabled, volume: this.volume, songs: this.library?.songs.length ?? 0,
        selectedIndex: this.selectedIndex, playingIndex: this.selectedIndex, title: song?.title ?? null, artist: song?.artist ?? null, genre: song?.genre ?? null,
        positionSeconds: song ? element.currentTime || 0 : 0, durationSeconds: song ? stream?.seconds ?? (Number.isFinite(element.duration) ? element.duration : 0) : 0,
        chunks: this.chunks, underruns: this.underruns, queuedSeconds: buffered, voices: song && !element.paused ? 1 : 0,
        mode: 'stream', seekable: Boolean(song), playing: Boolean(song) && !element.paused, streamBytes: stream?.bytes ?? 0 };
    }
    const now = this.context()?.currentTime ?? 0;
    const playing = this.playingSegment();
    const song = playing?.song ?? this.song;
    return { ready: this.ready, failed: this.failed, enabled: this.enabled, volume: this.volume, songs: this.library?.songs.length ?? 0,
      selectedIndex: this.selectedIndex, playingIndex: playing?.index ?? this.selectedIndex, title: song?.title ?? null, artist: song?.artist ?? null,
      positionSeconds: playing ? playing.trackOffset + playing.offset + (playing.source ? Math.max(0, now - playing.start) : 0) : 0,
      durationSeconds: playing?.duration ?? (this.info ? this.info.frames_max / this.info.sample_rate : 0),
      chunks: this.chunks, underruns: this.underruns,
      queuedSeconds: this.suspended ? this.queue.reduce((sum, segment) => sum + segment.buffer.duration - segment.offset, 0) : Math.max(0, this.scheduledUntil - now), voices: this.voices.size,
      genre: song?.genre ?? null, mode: 'decoder', seekable: false, playing: this.voices.size > 0 && !this.suspended, streamBytes: 0 };
  }
  close() {
    this.generation++; this.closed = true; this.ready = false; this.syncMediaSession();
    this.clearQueue(); this.gain?.disconnect(); this.analyserNode?.disconnect();
    if (this.element) { this.element.pause(); this.element.removeAttribute('src'); this.element.load?.(); this.mediaNode?.disconnect(); }
    if (this.silent) URL.revokeObjectURL(this.silent);
  }
}
