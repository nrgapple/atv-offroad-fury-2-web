// Browser-native frontend screens. Rust still supplies the supported actions,
// labels, values and keyboard choice order; this module owns the responsive HD
// layout. The composition follows the original ATV2 pages: a red-tinted scenic
// backdrop, a curved silver stage panel on the right, gold bullet rows with a
// glossy gold selection bar, and a Back control where the hint pill sat. Text is
// limited to the original-style labels; no invented descriptions or subheadings.
export const screenPages = new Set(['title','main','single','multiplayer','create-lobby','join-lobby','lobby','garage','gear','tracks','race','loading']);

// Original event families from tracks.dat ids; labels follow the manual's event types.
export const COURSE_GROUPS = [['all','All'],['env','Enduro'],['free','Stunt'],['nat','Nationals'],['sx','Supercross'],['extra','Arena']];
export function courseGroup(id) {
  const family = id.replace(/\d+$/, '');
  return ['env','free','nat','sx'].includes(family) ? family : 'extra';
}
export const groupLabel = id => COURSE_GROUPS.find(([key]) => key === courseGroup(id))?.[1] ?? '';

const titles = {main:'Main Menu',single:'Single Player',multiplayer:'Multiplayer','create-lobby':'Create Lobby','join-lobby':'Join Lobby',lobby:'Free Ride Lobby',garage:'Select ATV & Rider',gear:'Rider Gear',tracks:'Select Track',race:'Single Race',loading:'Now Loading...'};
const icon = {
  left:'<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M13 3 5 10l8 7z"/></svg>',
  right:'<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7 3l8 7-8 7z"/></svg>',
};

function h(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value === undefined || value === null || value === false) continue;
    if (key === 'class') node.className = value;
    else if (key === 'text') node.textContent = value;
    else if (key === 'html') node.innerHTML = value;
    else if (key.startsWith('on')) node[key] = value;
    else if (key === 'dataset') Object.assign(node.dataset, value);
    else node.setAttribute(key, value === true ? '' : value);
  }
  node.append(...children.flat().filter(child => child !== null && child !== undefined && child !== false));
  return node;
}

// Rebuild only when the screen's structure changes, so focus, scrolling and the
// live model canvases survive selection and label updates.
export function renderScreen(fe) {
  const root = fe.buttons;
  const garageIds = fe.page === 'garage' ? fe.garageCatalog().map(item => item.id).join(',') : '';
  const structure = JSON.stringify([fe.page, fe.frame.choices, fe.frame.hits.map(hit => hit.action), fe.trackFilter, garageIds, (fe.frame.inputs ?? []).map(input => input.action), Boolean(fe.backdropUrl)]);
  if (structure !== root.dataset.structure || !root.firstChild) {
    root.dataset.structure = structure;
    root.replaceChildren(build(fe));
    fe.restoreScreenFocus = true;
  }
  sync(fe);
}

function build(fe) {
  const page = fe.page;
  const screen = h('div', {class:`ms-screen ms-${page}`, dataset:{screen:page}});
  screen.append(h('div', {class:'ms-backdrop', 'aria-hidden':'true'}, h('div', {class:'ms-backdrop-photo'})));
  if (page === 'title') { buildTitle(fe, screen); return screen; }
  const left = h('div', {class:'ms-left'});
  const panel = h('section', {class:'ms-panel'});
  screen.append(h('div', {class:'ms-panel-shape', 'aria-hidden':'true'}), h('header', {class:'ms-header'}, h('h1', {class:'ms-heading', text:titles[page] ?? ''})), left, panel);
  ({main:buildMenu, single:buildMenu, multiplayer:buildMenu, 'create-lobby':buildForm, 'join-lobby':buildForm, lobby:buildLobby, garage:buildGarage, gear:buildGear, tracks:buildTracks, race:buildRace, loading:buildLoading})[page](fe, left, panel);
  if (page !== 'loading') screen.append(buildHints(fe));
  return screen;
}

function hit(fe, action) { return fe.frame.hits.find(item => item.action === action); }

function selectChoice(fe, action) {
  const index = fe.frame.choices.indexOf(action);
  if (fe.restoringScreenFocus || index < 0 || fe.selection === index) return;
  fe.selection = index; fe.layoutKey = ''; void fe.refresh();
}

// A choice button keeps the Rust hit label as its accessible name.
function choice(fe, action, className, ...content) {
  const target = hit(fe, action); if (!target) return null;
  const button = h('button', {type:'button', class:className, 'aria-label':target.label, dataset:{action}}, ...content);
  button.onclick = () => { const index = fe.frame.choices.indexOf(action); if (index >= 0) fe.selection = index; void fe.run(action); };
  button.onfocus = () => selectChoice(fe, action);
  // Only a moving mouse selects: layout changes under a resting pointer must
  // not steal keyboard/controller selection.
  button.onpointermove = event => { if (event.pointerType === 'mouse' && (event.movementX || event.movementY)) selectChoice(fe, action); };
  return button;
}

function row(fe, action, extra = []) {
  const target = hit(fe, action); if (!target) return null;
  return choice(fe, action, 'ms-row', h('span', {class:'ms-bullet', 'aria-hidden':'true'}), h('span', {class:'ms-row-label', text:target.label}), ...extra);
}

// The original pages end with a hint pill; the port keeps one Back control in
// that corner instead of console button glyphs, for touch, mouse and pads alike.
function buildHints(fe) {
  const bar = h('footer', {class:'ms-hints'});
  // The garage's gold Done button is its Back action; keep one accessible target.
  const back = fe.page === 'garage' ? null : choice(fe, 'back', 'ms-hint ms-hint-back', h('span', {class:'ms-back-icon', html:icon.left}), h('span', {text:hit(fe, 'back')?.label ?? 'Back'}));
  if (back) bar.append(back);
  return bar;
}

function buildTitle(fe, screen) {
  const logo = fe.images.get('brand-logo');
  screen.append(h('div', {class:'ms-title-lockup'},
    logo ? h('img', {class:'ms-logo', src:logo.src, alt:'Fury Remastered'}) : h('h1', {class:'ms-logo-text'}, 'Fury ', h('span', {text:'Remastered'}))));
  screen.append(h('div', {class:'ms-title-actions'},
    choice(fe, 'main', 'ms-start', h('span', {text:hit(fe, 'main')?.label ?? 'Press Start'})),
    choice(fe, 'devlog', 'ms-link', h('span', {text:'Dev log'}))));
}

function rideStage(panel, logo) {
  if (logo) panel.append(logo);
  panel.append(h('div', {class:'ms-model ms-model-quad', dataset:{modelSlot:'quad'}}),
    h('div', {class:'ms-plate'}, h('p', {class:'ms-ride-name'})));
}

function brandLogo(fe) {
  const logo = fe.images.get('brand-logo');
  return logo ? h('img', {class:'ms-panel-logo', src:logo.src, alt:''}) : null;
}

function buildMenu(fe, left, panel) {
  const list = h('nav', {class:'ms-rows', 'aria-label':titles[fe.page]});
  for (const action of fe.frame.choices) if (action !== 'back') list.append(row(fe, action));
  left.append(list);
  if (fe.page === 'multiplayer') panel.append(brandLogo(fe) ?? '');
  else rideStage(panel, brandLogo(fe));
}

function buildForm(fe, left, panel) {
  const form = h('div', {class:'ms-fields'});
  for (const field of fe.frame.inputs ?? []) {
    const input = h('input', {type:'text', class:'atv-text-input ms-input', autocomplete:'off', spellcheck:'false', maxlength:String(field.max_length), 'aria-label':field.label, dataset:{setting:field.action}});
    input.value = field.value;
    input.addEventListener('input', () => {
      if (field.action === 'player-name') fe.playerName = input.value;
      else if (field.action === 'create-code') fe.createCode = input.value.toUpperCase().trim();
      else fe.roomCode = input.value.toUpperCase().trim();
      fe.layoutKey = '';
    });
    form.append(h('label', {class:'ms-field'}, h('span', {text:field.label}), input));
  }
  const list = h('nav', {class:'ms-rows', 'aria-label':titles[fe.page]});
  for (const action of fe.frame.choices) if (action !== 'back') list.append(row(fe, action, action === 'garage' ? [h('span', {class:'ms-row-value ms-ride-summary'})] : []));
  left.append(form, list);
  rideStage(panel, null);
}

function trackFigure(className) {
  return h('figure', {class:className}, h('div', {class:'ms-photo'}, h('img', {alt:''})),
    h('figcaption', {}, h('h2', {class:'ms-track-name'})));
}

function buildLobby(fe, left, panel) {
  const list = h('nav', {class:'ms-rows', 'aria-label':'Lobby actions'});
  for (const action of fe.frame.choices) list.append(row(fe, action));
  left.append(h('div', {class:'ms-lobby-info'}, h('p', {class:'ms-code'}), h('p', {class:'ms-status'})), list);
  panel.append(trackFigure('ms-track-hero'));
}

function buildRace(fe, left, panel) {
  const list = h('nav', {class:'ms-rows', 'aria-label':'Race setup'});
  for (const action of fe.frame.choices) {
    const stepper = ['racetrack','racelaps','racecpu'].includes(action);
    const item = row(fe, action, stepper ? [h('span', {class:'ms-row-value'})] : []);
    if (stepper) item.classList.add('ms-row-cycle');
    list.append(item);
  }
  left.append(list);
  panel.append(trackFigure('ms-track-hero'));
}

// The original course loader (LdScreen): an action shot on the left, a black
// "Track:" bar and "Tips:" on the gray panel, and a gold bar in a black pill.
// Original Profile Store categories as left/right steppers beside the live
// quad and rider previews.
function buildGear(fe, left, panel) {
  const list = h('nav', {class:'ms-rows', 'aria-label':'Rider gear'});
  for (const action of fe.frame.choices) {
    if (action === 'back') continue;
    const item = row(fe, action, [h('span', {class:'ms-row-value'})]);
    item?.classList.add('ms-row-cycle');
    list.append(item ?? '');
  }
  left.append(list);
  panel.append(h('div', {class:'ms-models'},
    h('div', {class:'ms-model ms-model-quad', dataset:{modelSlot:'quad'}}),
    h('div', {class:'ms-model ms-model-rider', dataset:{modelSlot:'rider'}})));
}

function buildLoading(fe, left, panel) {
  left.append(h('figure', {class:'ms-shot'}));
  panel.append(h('div', {class:'ms-ld-info'},
    h('p', {class:'ms-ld-track'}, h('span', {text:'Track:'}), ' ', h('span', {class:'ms-track-name'})),
    h('div', {class:'ms-ld-tips'}, h('p', {class:'ms-ld-tips-label', text:'Tips:'}), h('p', {class:'ms-ld-tip'}))));
  left.parentElement.append(h('footer', {class:'ms-ld-bar'},
    h('div', {class:'course-load-meter ms-meter', role:'progressbar', 'aria-valuemin':'0', 'aria-valuemax':'100'}, h('span', {class:'ms-meter-fill'}))));
}

function buildTracks(fe, left, panel) {
  const tabs = h('div', {class:'ms-tabs', role:'tablist', 'aria-label':'Event type'});
  for (const [key, label] of COURSE_GROUPS) {
    const count = key === 'all' ? fe.tracks.length : fe.tracks.filter(track => courseGroup(track.id) === key).length;
    if (!count) continue;
    const tab = h('button', {type:'button', role:'tab', class:'ms-tab', 'aria-selected':String(fe.trackFilter === key), dataset:{filter:key}}, label, h('small', {text:String(count)}));
    tab.onclick = () => fe.setTrackFilter(key);
    tabs.append(tab);
  }
  const list = h('div', {class:'ms-courses ms-scroll', role:'listbox', 'aria-label':'Courses'});
  for (const index of fe.trackOrder()) {
    const track = fe.tracks[index], url = fe.trackPhotoUrl(track);
    const card = h('button', {type:'button', class:'ms-course', role:'option', 'aria-label':track.name, dataset:{trackId:track.id}},
      h('span', {class:'ms-course-thumb'}, url ? h('img', {alt:'', src:url}) : null),
      h('span', {class:'ms-course-copy'}, h('span', {class:'ms-course-name', text:track.name}), h('small', {text:groupLabel(track.id)})));
    card.onclick = () => fe.selectTrack(index);
    list.append(card);
  }
  left.append(tabs, list);
  const hero = trackFigure('ms-track-hero');
  const photo = hero.querySelector('.ms-photo');
  for (const [action, glyph] of [['left', icon.left], ['right', icon.right]]) {
    const arrow = choice(fe, action, `ms-arrow ms-arrow-${action}`, h('span', {html:glyph}));
    if (!arrow) continue;
    arrow.onclick = () => void fe.action(action);
    photo.append(arrow);
  }
  const ride = hit(fe, 'accept');
  if (ride) {
    const button = h('button', {type:'button', class:'ms-pill ms-ride', 'aria-label':ride.label, dataset:{action:'accept'}}, h('span', {text:'Ride'}));
    button.onclick = () => void fe.action('accept');
    hero.querySelector('figcaption').append(button);
  }
  panel.append(hero);
}

// The original Select ATV & Rider page keeps both choices as rows on one page
// beside the live model; the browser adds a direct list of every vehicle.
function buildGarage(fe, left, panel) {
  const rows = h('nav', {class:'ms-rows ms-garage-rows', 'aria-label':'Ride setup'});
  const vehicleRow = h('div', {class:'ms-row ms-garage-row', dataset:{row:'vehicle'}},
    choice(fe, 'quadprev', 'ms-cycle', h('span', {html:icon.left})),
    h('span', {class:'ms-row-stack'}, h('span', {class:'ms-row-label ms-vehicle-kind'}), h('span', {class:'ms-row-value ms-ride-name'})),
    choice(fe, 'quadnext', 'ms-cycle', h('span', {html:icon.right})));
  // One tap toggles the rider; the switch shows which one is riding.
  const rider = row(fe, 'riderchange', [h('span', {class:'ms-switch', 'aria-hidden':'true'}, h('span', {dataset:{rider:'0'}, text:'Male'}), h('span', {dataset:{rider:'1'}, text:'Female'}))]);
  rider?.classList.add('ms-garage-row', 'ms-rider-row');
  const gear = row(fe, 'gear');
  gear?.classList.add('ms-garage-row');
  rows.append(vehicleRow, rider ?? '', gear ?? '');
  const catalog = fe.garageCatalog();
  const list = h('div', {class:'ms-vehicles ms-scroll', role:'listbox', 'aria-label':'Vehicles'});
  for (const group of [...new Set(catalog.map(item => item.group))]) {
    for (const item of catalog.filter(item => item.group === group)) {
      const option = h('button', {type:'button', class:'ms-vehicle', role:'option', 'aria-label':item.label, dataset:{vehicle:String(item.id)}},
        h('span', {class:'ms-bullet', 'aria-hidden':'true'}), h('span', {class:'ms-vehicle-name', text:item.label}), item.class ? h('small', {text:item.class}) : null);
      option.onclick = () => void fe.run(`vehicle:${item.id}`);
      list.append(option);
    }
  }
  left.append(rows, list, choice(fe, 'back', 'ms-pill ms-done', h('span', {text:hit(fe, 'back')?.label ?? 'Done'})));
  panel.append(h('div', {class:'ms-plate ms-garage-plate'}, h('p', {class:'ms-ride-name'})),
    h('div', {class:'ms-models'},
      h('div', {class:'ms-model ms-model-quad', dataset:{modelSlot:'quad'}}),
      h('div', {class:'ms-model ms-model-rider', dataset:{modelSlot:'rider'}})));
}

function setPhoto(figure, url, track) {
  if (!figure) return;
  const img = figure.querySelector('.ms-photo img');
  if (img.getAttribute('src') !== (url || null)) { if (url) img.src = url; else img.removeAttribute('src'); }
  figure.classList.toggle('is-empty', !url);
  figure.querySelector('.ms-track-name').textContent = track?.name ?? '';
}

function sync(fe) {
  const root = fe.buttons, state = fe.presentation, page = fe.page;
  const selected = fe.frame.choices[fe.selection];
  for (const node of root.querySelectorAll('[data-action]')) {
    const target = hit(fe, node.dataset.action);
    if (target && node.tagName === 'BUTTON') node.setAttribute('aria-label', target.label);
    node.setAttribute('aria-current', String(node.dataset.action === selected));
  }
  root.querySelector('[data-row=vehicle]')?.setAttribute('aria-current', String(['quadprev','quadnext'].includes(selected)));
  const focusTrack = page === 'tracks' ? fe.tracks[fe.track] : ['race','lobby','loading'].includes(page) ? fe.presentationTrack : null;
  const backdrop = root.querySelector('.ms-backdrop-photo'), backdropUrl = (focusTrack && fe.trackPhotoUrl(focusTrack)) || fe.backdropUrl;
  if (backdrop) {
    backdrop.style.backgroundImage = backdropUrl ? `url("${backdropUrl}")` : '';
    backdrop.classList.toggle('is-track', Boolean(focusTrack && fe.trackPhotoUrl(focusTrack)));
  }
  const vehicle = fe.vehicleSummary();
  for (const name of root.querySelectorAll('.ms-ride-name')) name.textContent = vehicle.label;
  for (const summary of root.querySelectorAll('.ms-ride-summary')) summary.textContent = vehicle.label;
  for (const button of root.querySelectorAll('.ms-row[data-action]')) {
    const target = hit(fe, button.dataset.action), value = button.querySelector('.ms-row-value:not(.ms-ride-summary):not(.ms-ride-name)');
    if (!target || button.classList.contains('ms-garage-row')) continue;
    const [label, ...rest] = target.label.split(': ');
    button.querySelector('.ms-row-label').textContent = value ? label : target.label;
    if (value) value.textContent = rest.join(': ');
  }
  if (page === 'race' || page === 'lobby') {
    setPhoto(root.querySelector('.ms-track-hero'), fe.trackPhotoUrl(fe.presentationTrack), fe.presentationTrack);
    if (page === 'lobby') {
      root.querySelector('.ms-code').textContent = state.room_code || '--------';
      root.querySelector('.ms-status').textContent = state.lobby_status || 'Connecting…';
    }
  }
  if (page === 'loading') {
    // Original action shots crossfade when available; otherwise the course photo.
    const frame = root.querySelector('.ms-shot'), urls = fe.loadingShots?.length ? fe.loadingShots : [fe.trackPhotoUrl(fe.presentationTrack)].filter(Boolean);
    if (frame.dataset.urls !== urls.join('|')) {
      frame.dataset.urls = urls.join('|');
      frame.replaceChildren(...urls.map(url => h('img', {alt:'', src:url})));
    }
    [...frame.children].forEach((img, i) => img.classList.toggle('is-on', i === (fe.shotFrame ?? 0) % frame.children.length));
    root.querySelector('.ms-track-name').textContent = fe.presentationTrack?.name ?? '';
    const tip = fe.loadingTip ?? [];
    root.querySelector('.ms-ld-tips').hidden = !tip.length;
    root.querySelector('.ms-ld-tip').replaceChildren(...tip.flatMap((line, i) => i ? [document.createElement('br'), line] : [line]));
    const meter = root.querySelector('.ms-meter'), progress = Math.round((fe.loadingProgress ?? 0) * 100);
    meter.style.setProperty('--progress', `${progress}%`);
    meter.setAttribute('aria-valuenow', String(progress)); meter.setAttribute('aria-label', state.loading_status);
  }
  if (page === 'tracks') {
    const track = fe.tracks[fe.track];
    setPhoto(root.querySelector('.ms-track-hero'), fe.trackPhotoUrl(track), track);
    for (const card of root.querySelectorAll('.ms-course')) {
      const current = card.dataset.trackId === track?.id;
      card.setAttribute('aria-selected', String(current));
      if (current && (fe.scrollScreenSelection || fe.restoreScreenFocus)) card.scrollIntoView({block:'nearest', inline:'nearest'});
    }
  }
  if (page === 'garage') {
    root.querySelector('.ms-vehicle-kind').textContent = vehicle.group === 'Motorcycle' ? 'Bike' : 'ATV';
    const riderRow = root.querySelector('.ms-rider-row');
    if (riderRow) {
      riderRow.setAttribute('aria-disabled', String(fe.quadId >= 27));
      riderRow.querySelector('.ms-row-label').textContent = fe.quadId >= 27 ? 'MX rider' : 'Rider';
      for (const option of riderRow.querySelectorAll('[data-rider]')) option.classList.toggle('is-on', fe.quadId < 27 && fe.riderId === Number(option.dataset.rider));
    }
    for (const option of root.querySelectorAll('.ms-vehicle')) {
      const current = Number(option.dataset.vehicle) === fe.quadId;
      option.setAttribute('aria-selected', String(current));
      if (current && (fe.scrollScreenSelection || fe.restoreScreenFocus)) option.scrollIntoView({block:'nearest'});
    }
  }
  fe.restoringScreenFocus = true;
  if (fe.restoreScreenFocus || fe.scrollScreenSelection) {
    const current = root.querySelector(`button[data-action="${selected}"]`);
    if (current && (fe.scrollScreenSelection || !root.contains(document.activeElement))) current.focus({preventScroll:true});
  }
  fe.restoringScreenFocus = false; fe.restoreScreenFocus = false; fe.scrollScreenSelection = false;
}
