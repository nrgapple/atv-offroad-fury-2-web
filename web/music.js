// Bounded local-disc soundtrack streaming. Rust retains both ADPCM predictors;
// Web Audio schedules small PCM buffers on one continuous sample clock.
export class MusicStream {
  constructor(codec, context, destination, report) {
    Object.assign(this, { codec, context, destination, report });
    this.ready = false; this.closed = false; this.busy = false; this.failed = false;
    this.voices = new Set(); this.scheduledUntil = 0; this.chunks = 0; this.underruns = 0;
    this.song = null; this.entry = null; this.offset = 0; this.finished = true;
  }
  async load({ file, files, read, isCurrent }) {
    Object.assign(this, { file, files, read, isCurrent });
    const archive = files.find(entry => entry.path === '/STARTAPP.RES');
    if (!archive) throw new Error('Original music catalog archive is missing.');
    const bytes = await read(file, archive.lba * 2048, archive.bytes);
    const entries = JSON.parse(new TextDecoder().decode(await this.codec(2, bytes)));
    const index = entries.findIndex(entry => /(^|[\\/])music\.pb$/i.test(entry.name));
    if (index < 0) throw new Error('Original music catalog is missing.');
    const catalog = await this.codec(3, bytes, index);
    if (this.closed || !isCurrent()) return;
    this.library = JSON.parse(new TextDecoder().decode(await this.codec(40, catalog)));
    this.ready = !this.closed && isCurrent();
  }
  async nextSong() {
    const song = JSON.parse(new TextDecoder().decode(await this.codec(41, new Uint8Array())));
    if (this.closed || !this.isCurrent()) return false;
    if (!song) { this.ready = false; return false; }
    const path = `/AUDIO/${song.filename.replaceAll('\\', '/')}`.toLowerCase();
    const entry = this.files.find(item => item.path.toLowerCase() === path);
    if (!entry) throw new Error(`Original music file is missing: ${song.filename}`);
    const prefix = await this.read(this.file, entry.lba * 2048, Math.min(2016, entry.bytes));
    if (this.closed || !this.isCurrent()) return false;
    this.info = JSON.parse(new TextDecoder().decode(await this.codec(42, prefix, entry.bytes)));
    if (this.closed || !this.isCurrent()) return false;
    this.song = song; this.entry = entry; this.offset = 0; this.finished = false;
    return true;
  }
  async update() {
    const context = this.context();
    if (!this.ready || this.closed || this.failed || this.busy || context?.state !== 'running') return;
    this.busy = true;
    try {
      if (!this.gain) {
        this.gain = context.createGain(); this.gain.gain.value = 0.45;
        this.gain.connect(this.destination());
      }
      let iterations = 0;
      while (this.scheduledUntil - context.currentTime < 3 && iterations++ < 8) {
        if (this.closed || context.state !== 'running' || !this.isCurrent()) return;
        if (this.finished && !await this.nextSong()) return;
        const count = Math.min(65536, this.info.data_bytes - this.offset);
        if (count === 0) { this.finished = true; continue; }
        const bytes = await this.read(this.file, this.entry.lba * 2048 + this.info.data_offset + this.offset, count);
        if (this.closed || !this.isCurrent()) return;
        const pcm = await this.codec(43, bytes);
        if (this.closed || !this.isCurrent()) return;
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
        const source = context.createBufferSource(); source.buffer = buffer; source.connect(this.gain);
        if (this.scheduledUntil < context.currentTime) {
          if (this.chunks > 0) this.underruns++;
          this.scheduledUntil = context.currentTime + 0.1;
        }
        this.voices.add(source);
        source.onended = () => { this.voices.delete(source); source.disconnect(); };
        source.start(this.scheduledUntil);
        this.scheduledUntil += frames / this.info.sample_rate;
        this.chunks++;
      }
    } catch (error) { if (!this.closed) { this.failed = true; this.report(`Music: ${error.message}`); } }
    finally { this.busy = false; }
  }
  snapshot() {
    return { ready: this.ready, failed: this.failed, songs: this.library?.songs.length ?? 0,
      title: this.song?.title ?? null, chunks: this.chunks, underruns: this.underruns,
      queuedSeconds: Math.max(0, this.scheduledUntil - (this.context()?.currentTime ?? 0)), voices: this.voices.size };
  }
  close() {
    this.closed = true; this.ready = false;
    for (const source of this.voices) { source.onended = null; source.stop(); source.disconnect(); }
    this.voices.clear(); this.gain?.disconnect();
  }
}
