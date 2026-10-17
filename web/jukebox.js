// Jukebox: a music player over the original 21-song soundtrack, opened from
// the main menu and Pause. Rust supplies the actions, labels and keyboard
// order; this module lays the player out and keeps its live state (progress,
// play state, playing row, spectrum) in step with the music stream.
// Artwork comes from the original Jukebox page (PAGE14.RES): the red snow
// backdrop and its 20-frame spinning CD. The disc ships no album covers.
export const JUKEBOX_SEEK_STEP = 10;
const JUKEBOX = '/UI/RESOURCE/PAGEA/PAGE14.RES';
const BARS = 24;
const base = name => name.replaceAll('\\', '/').split('/').at(-1).toLowerCase();
const clock = seconds => {
  const whole = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
};
const icon = {
  play: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg>',
  pause: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 5h4v14H7zM13 5h4v14h-4z"/></svg>',
  prev: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 5h2.5v14H6zM20 5v14L9.5 12z"/></svg>',
  next: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M15.5 5H18v14h-2.5zM4 5v14l10.5-7z"/></svg>',
  back: '<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M13 3 5 10l8 7z"/></svg>',
  volume: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 9v6h4l5 4V5L8 9zm12.5 3a4.5 4.5 0 0 0-2.5-4v8a4.5 4.5 0 0 0 2.5-4z"/></svg>',
};

function h(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value === undefined || value === null || value === false) continue;
    if (key === 'class') node.className = value;
    else if (key === 'text') node.textContent = value;
    else if (key === 'html') node.innerHTML = value;
    else if (key === 'dataset') Object.assign(node.dataset, value);
    else node.setAttribute(key, value === true ? '' : value);
  }
  node.append(...children.filter(child => child !== null && child !== undefined));
  return node;
}

// Original artwork, decoded once per session. An older content package
// without PAGE14 keeps the drawn disc.
function loadArt(fe) {
  fe.jukeboxArt ??= (async () => {
    const { file, files } = await fe.content, record = files.find(entry => entry.path === JUKEBOX);
    if (!record) return null;
    const bytes = await fe.read(file, record.lba * 2048, record.bytes), entries = await fe.json(2, bytes);
    const member = name => {
      const index = entries.findIndex(entry => base(entry.name) === name);
      if (index < 0) throw new Error(`Original Jukebox member missing: ${name}`);
      return fe.codec(3, bytes, index);
    };
    const picture = async name => { const data = await member(name); return fe.bitmap(await fe.json(7, data), await fe.codec(8, data)); };
    const atm = await member('cd64.atm'), first = await fe.json(56, atm, 0);
    const sheet = document.createElement('canvas'); sheet.width = first.width * first.frames; sheet.height = first.height;
    const context = sheet.getContext('2d');
    for (let frame = 0; frame < first.frames; frame++) {
      const rgba = await fe.codec(57, atm, frame);
      context.putImageData(new ImageData(new Uint8ClampedArray(rgba), first.width, first.height), frame * first.width, 0);
    }
    return { backdrop: await fe.imageUrl(await picture('bk_jukebox.pst')), disc: await fe.imageUrl(sheet), frames: first.frames };
  })().catch(() => null);
  return fe.jukeboxArt;
}

function applyArt(panel, art) {
  if (!art) return;
  panel.style.setProperty('--jb-backdrop', `url("${art.backdrop}")`);
  panel.style.setProperty('--jb-disc', `url("${art.disc}")`);
  panel.style.setProperty('--jb-disc-frames', String(art.frames));
  panel.dataset.art = 'original';
}

export function stopJukebox(fe) {
  if (fe.jukeboxFrame) cancelAnimationFrame(fe.jukeboxFrame);
  fe.jukeboxFrame = 0;
}

function build(fe, panel, music) {
  const hit = action => fe.frame.hits.find(item => item.action === action);
  const select = action => {
    const index = fe.frame.choices.indexOf(action);
    if (fe.restoringPauseFocus || index < 0 || fe.selection === index) return;
    fe.selection = index; fe.layoutKey = '';
  };
  const control = (action, className, content) => {
    if (!hit(action)) return null;
    const button = h('button', { type: 'button', class: className, dataset: { action } }, ...content);
    button.onclick = () => { fe.selection = fe.frame.choices.indexOf(action); void fe.run(action); };
    button.onfocus = () => select(action);
    return button;
  };
  const range = (action, min, max, step) => {
    const input = h('input', { type: 'range', class: 'atv-range', min, max, step, dataset: { setting: action, action } });
    input.onfocus = () => select(action);
    return input;
  };
  const seek = range('jukeseek', 0, 1, 0.1), volume = range('jukevolume', 0, 100, 5);
  seek.setAttribute('aria-label', 'Seek');
  seek.oninput = () => { fe.jukeboxSeeking = true; panel.querySelector('.jb-elapsed').textContent = clock(Number(seek.value)); seek.style.setProperty('--atv-range-fill', `${Number(seek.value) / Number(seek.max) * 100}%`); };
  seek.onchange = () => { music?.seek(Number(seek.value)); fe.jukeboxSeeking = false; };
  seek.onpointerup = seek.onblur = () => { if (fe.jukeboxSeeking) seek.onchange(); };
  volume.setAttribute('aria-label', 'Music volume');
  volume.oninput = () => { music?.setVolume(Number(volume.value) / 100); fe.layoutKey = ''; void fe.refresh(); };

  const tracks = h('ol', { class: 'jb-list' });
  for (const action of fe.frame.choices.filter(choice => choice.startsWith('song:'))) {
    const index = Number(action.slice(5)), song = music?.tracks?.[index];
    const button = control(action, 'jb-track', [
      h('span', { class: 'jb-num', text: String(index + 1).padStart(2, '0') }),
      h('span', { class: 'jb-track-copy' }, h('span', { class: 'jb-track-title', text: song?.title ?? '' }), h('span', { class: 'jb-track-artist', text: song?.artist ?? '' })),
      h('span', { class: 'jb-track-genre', text: song?.genre ?? '' }),
      h('span', { class: 'jb-eq', 'aria-hidden': 'true' }, h('i'), h('i'), h('i')),
    ]);
    if (button) tracks.append(h('li', {}, button));
  }
  const now = h('section', { class: 'jb-now', 'aria-label': 'Now playing' },
    h('div', { class: 'jb-art' },
      h('div', { class: 'jb-art-photo', 'aria-hidden': 'true' }),
      h('div', { class: 'jb-disc', role: 'img', 'aria-label': 'Original Jukebox CD' }),
      h('canvas', { class: 'jb-bars', width: String(BARS * 8), height: '48', 'aria-hidden': 'true' })),
    h('div', { class: 'jb-meta', 'aria-live': 'polite' },
      h('p', { class: 'jb-genre' }), h('h2', { class: 'jb-title' }), h('p', { class: 'jb-artist' })),
    h('div', { class: 'jb-seek' }, h('span', { class: 'jb-time jb-elapsed', text: '0:00' }), seek, h('span', { class: 'jb-time jb-duration', text: '0:00' })),
    h('div', { class: 'jb-controls' },
      h('div', { class: 'jb-transport' },
        control('jukeprev', 'jb-skip', [h('span', { html: icon.prev })]),
        control('jukeplay', 'jb-play', [h('span', { class: 'jb-play-icon', html: icon.play })]),
        control('jukenext', 'jb-skip', [h('span', { html: icon.next })])),
      h('label', { class: 'jb-volume' }, h('span', { class: 'jb-volume-icon', html: icon.volume }), volume, h('output', { class: 'jb-volume-value' }))));
  // Back sits in the header so it never covers the list; only the track list
  // scrolls, the player stays put.
  const header = h('header', { class: 'pause-header jb-header' },
    control('back', 'jb-back', [h('span', { class: 'jb-back-icon', html: icon.back }), h('span', { class: 'jb-back-label', text: hit('back')?.label ?? 'Back' })]),
    h('h1', { id: 'pause-title', text: 'Jukebox' }), h('p', { class: 'pause-status jb-source' }));
  panel.replaceChildren(header, h('div', { class: 'jb-body' }, now, h('section', { class: 'jb-tracks', 'aria-label': 'Original soundtrack' }, tracks)));
}

function drawBars(panel, music, playing) {
  const canvas = panel.querySelector('.jb-bars'), analyser = playing ? music?.analyser?.() : null;
  const context = canvas?.getContext('2d');
  if (!context) return;
  context.clearRect(0, 0, canvas.width, canvas.height);
  const levels = new Uint8Array(analyser?.frequencyBinCount ?? 0);
  analyser?.getByteFrequencyData(levels);
  const gradient = context.createLinearGradient(0, canvas.height, 0, 0);
  gradient.addColorStop(0, '#a99900'); gradient.addColorStop(1, '#fffaa1');
  context.fillStyle = gradient;
  for (let i = 0; i < BARS; i++) {
    const level = levels.length ? levels[Math.min(levels.length - 1, Math.floor(i * levels.length / BARS))] / 255 : 0;
    const height = Math.max(2, level * canvas.height);
    context.fillRect(i * 8 + 1, canvas.height - height, 6, height);
  }
}

// Live values that change without a Rust layout: once per animation frame
// while the Jukebox is on screen, touching only what changed.
function sync(fe, panel) {
  const music = fe.audio?.()?.music, state = music?.snapshot?.();
  if (!state) return;
  const set = (selector, text) => { const node = panel.querySelector(selector); if (node && node.textContent !== text) node.textContent = text; };
  const playing = Boolean(state.playing && state.enabled), song = state.title !== null;
  panel.dataset.playing = String(playing);
  set('.jb-title', state.title ?? 'Choose a song');
  set('.jb-artist', state.artist ?? (state.ready ? `${state.songs} original songs` : 'Loading the soundtrack…'));
  set('.jb-genre', state.genre ?? 'Original soundtrack');
  set('.jb-source', state.failed ? 'Music unavailable' : state.mode === 'stream' ? 'Streaming' : 'Original decoder');
  set('.jb-duration', clock(state.durationSeconds));
  const seek = panel.querySelector('input[data-setting=jukeseek]');
  seek.disabled = !state.seekable || !song;
  seek.max = String(Math.max(1, state.durationSeconds));
  if (!fe.jukeboxSeeking) {
    seek.value = String(state.positionSeconds);
    set('.jb-elapsed', clock(state.positionSeconds));
    seek.style.setProperty('--atv-range-fill', `${Math.min(100, state.positionSeconds / Math.max(1, state.durationSeconds) * 100)}%`);
  }
  seek.setAttribute('aria-valuetext', `${clock(state.positionSeconds)} of ${clock(state.durationSeconds)}`);
  const volume = panel.querySelector('input[data-setting=jukevolume]'), percent = Math.round(state.volume * 100);
  if (document.activeElement !== volume || Number(volume.value) !== percent) volume.value = String(percent);
  volume.style.setProperty('--atv-range-fill', `${percent}%`);
  volume.setAttribute('aria-valuetext', `${percent}%`);
  set('.jb-volume-value', `${percent}%`);
  const play = panel.querySelector('[data-action=jukeplay] .jb-play-icon');
  const glyph = state.enabled ? 'pause' : 'play';
  if (play && play.dataset.glyph !== glyph) { play.dataset.glyph = glyph; play.innerHTML = icon[glyph]; }
  for (const button of panel.querySelectorAll('.jb-track')) {
    const index = Number(button.dataset.action.slice(5));
    const current = String(index === state.playingIndex && song);
    if (button.dataset.playing !== current) button.dataset.playing = current;
  }
  drawBars(panel, music, playing);
}

export function renderJukebox(fe) {
  const panel = fe.pausePanel, music = fe.audio?.()?.music;
  const structure = JSON.stringify(['jukebox', fe.frame.choices]);
  // The shared pause renderer must rebuild after the Jukebox replaced its nodes.
  fe.panelStructure = null;
  const changed = panel.dataset.structure !== structure || panel.dataset.screen !== 'jukebox';
  if (changed) {
    build(fe, panel, music);
    panel.dataset.structure = structure;
    void loadArt(fe).then(art => { if (fe.page === 'jukebox') applyArt(panel, art); });
  }
  panel.dataset.screen = 'jukebox';
  const selected = fe.frame.choices[fe.selection];
  for (const node of panel.querySelectorAll('[data-action]')) {
    const target = fe.frame.hits.find(item => item.action === node.dataset.action);
    if (!target) continue;
    node.setAttribute('aria-label', target.label);
    if (node.tagName === 'BUTTON') node.setAttribute('aria-current', String(node.dataset.action === selected));
    else node.dataset.selected = String(node.dataset.action === selected);
  }
  fe.restoringPauseFocus = true;
  if (fe.resetPauseScroll || changed) panel.scrollTop = 0;
  if (fe.scrollPauseSelection || fe.resetPauseScroll || changed) {
    const target = panel.querySelector(`[data-action="${selected}"]`);
    target?.focus({ preventScroll: true });
    // Scroll only the track list; scrollIntoView would also pan the page.
    const list = panel.querySelector('.jb-tracks');
    if (fe.scrollPauseSelection && target && list?.contains(target)) {
      const row = target.getBoundingClientRect(), box = list.getBoundingClientRect();
      if (row.top < box.top) list.scrollTop -= box.top - row.top;
      else if (row.bottom > box.bottom) list.scrollTop += row.bottom - box.bottom;
    }
  }
  fe.resetPauseScroll = false; fe.scrollPauseSelection = false; fe.restoringPauseFocus = false;
  Object.assign(fe.root.dataset, { selection: String(fe.selection), renderer: 'rust' });
  sync(fe, panel);
  // Open with the playing song in view inside the list.
  if (changed) {
    const list = panel.querySelector('.jb-tracks'), row = panel.querySelector('.jb-track[data-playing=true]');
    if (list && row) { const a = row.getBoundingClientRect(), b = list.getBoundingClientRect(); list.scrollTop += a.top - b.top - (b.height - a.height) / 2; }
  }
  if (!fe.jukeboxFrame) {
    const tick = () => {
      if (fe.page !== 'jukebox' || fe.root.hidden) { fe.jukeboxFrame = 0; return; }
      sync(fe, panel);
      fe.jukeboxFrame = requestAnimationFrame(tick);
    };
    fe.jukeboxFrame = requestAnimationFrame(tick);
  }
}
