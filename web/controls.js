import { defaultMapping, validMapping, fromGamepad, combine, stick, gesture, clamp } from './input.js';
const $ = id => document.getElementById(id);
let mapping = structuredClone(defaultMapping);
try { const saved = JSON.parse(localStorage.getItem('atv-controls-v1')); if (validMapping(saved)) mapping = saved; } catch { /* Storage can be disabled in private mode. */ }
let active = false; let hasFocus = true; let selected = null; let remap = null;
let keys = {}; let touch = {}; let axisTouch = {}; let lastPad = null;
let lastNav = 0; let lastConfirm = false; let lastCancel = false; let lastPause = false;
let trickDirection = ''; let trickUntil = 0; let paused = false;
const keyActions = { ArrowLeft: ['steer', -1], ArrowRight: ['steer', 1], ArrowUp: ['throttle', 1], ArrowDown: ['brake', 1], KeyA: ['steer', -1], KeyD: ['steer', 1], KeyW: ['throttle', 1], KeyS: ['brake', 1], Space: ['preload', 1], KeyJ: ['jump', 1], KeyT: ['trick', 1], KeyR: ['reset', 1], Escape: ['pause', 1] };
function clear() { keys = {}; touch = {}; axisTouch = {}; trickDirection = ''; trickUntil = 0; remap = null; for (const zone of document.querySelectorAll('.touch-zone')) { delete zone.dataset.pointer; zone.classList.remove('engaged'); } document.querySelectorAll('.touch-button').forEach(b => b.classList.remove('held')); }
function showMapping() {
  $('mapping').replaceChildren();
  for (const [action, binding] of Object.entries(mapping)) {
    const button = document.createElement('button'); const value = 'axis' in binding ? `axis ${binding.axis}` : `button ${binding.button}`;
    button.textContent = `${action} · ${value}`; button.dataset.action = action;
    button.onclick = () => { remap = { action, baseline: lastPad ? [...lastPad.axes] : [], armed: false }; $('controller-status').textContent = `Remap ${action}: release buttons, then ${'axis' in binding ? 'move an axis' : 'press a button'} on the selected controller. Escape cancels.`; };
    $('mapping').append(button);
  }
}
showMapping();
$('reset-mapping').onclick = () => { mapping = structuredClone(defaultMapping); remap = null; try { localStorage.removeItem('atv-controls-v1'); } catch {} showMapping(); };
$('controller-select').onchange = event => { selected = event.target.value === 'auto' ? null : Number(event.target.value); remap = null; };
export function setControlsActive(value) { active = value; clear(); }
window.addEventListener('blur', () => { hasFocus = false; clear(); });
window.addEventListener('focus', () => { hasFocus = true; });
document.addEventListener('visibilitychange', () => { if (document.hidden) clear(); });
window.addEventListener('keydown', event => {
  if (event.code === 'Escape' && remap) { remap = null; $('controller-status').textContent = 'Remapping cancelled.'; return; }
  if (!active || /INPUT|SELECT|TEXTAREA/.test(event.target.tagName) || event.target.isContentEditable || !keyActions[event.code]) return;
  event.preventDefault(); keys[event.code] = true;
});
window.addEventListener('keyup', event => { delete keys[event.code]; });
function bindZone(id, handler) {
  const zone = $(id);
  zone.addEventListener('pointerdown', event => {
    if (zone.dataset.pointer || !active) return;
    event.preventDefault(); zone.setPointerCapture(event.pointerId); zone.dataset.pointer = String(event.pointerId); zone.classList.add('engaged');
    zone.start = { x: event.clientX, y: event.clientY };
    const bounds = zone.getBoundingClientRect(); zone.style.setProperty('--origin-x', `${event.clientX - bounds.left}px`); zone.style.setProperty('--origin-y', `${event.clientY - bounds.top}px`);
    handler(0, 0, false); zone.style.setProperty('--stick-x', '0px'); zone.style.setProperty('--stick-y', '0px');
  });
  zone.addEventListener('pointermove', event => {
    if (zone.dataset.pointer !== String(event.pointerId)) return;
    handler(event.clientX - zone.start.x, event.clientY - zone.start.y, false);
  });
  const release = event => {
    if (zone.dataset.pointer !== String(event.pointerId)) return;
    handler(event.clientX - zone.start.x, event.clientY - zone.start.y, true, event.type !== 'pointerup');
    delete zone.dataset.pointer; zone.classList.remove('engaged');
  };
  for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) zone.addEventListener(type, release);
}
bindZone('drive-zone', (dx, dy, ended) => {
  const p = ended ? { x: 0, y: 0 } : stick(dx, dy);
  axisTouch = { steer: p.x, throttle: Math.max(0, -p.y), brake: Math.max(0, p.y) };
  $('drive-zone').style.setProperty('--stick-x', `${p.x * 54}px`); $('drive-zone').style.setProperty('--stick-y', `${p.y * 54}px`);
});
bindZone('trick-zone', (dx, dy, ended, cancelled) => {
  if (!ended || cancelled) return;
  const direction = gesture(dx, dy);
  if (direction) { trickDirection = direction; trickUntil = performance.now() + 220; $('last-trick').textContent = `Swipe: ${direction}`; }
});
for (const button of document.querySelectorAll('.touch-button')) {
  button.addEventListener('pointerdown', event => {
    if (!active) return; event.preventDefault(); button.setPointerCapture(event.pointerId); button.dataset.pointer = String(event.pointerId); touch[button.dataset.action] = 1; button.classList.add('held');
  });
  const release = event => { if (button.dataset.pointer === String(event.pointerId)) { touch[button.dataset.action] = 0; delete button.dataset.pointer; button.classList.remove('held'); } };
  for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) button.addEventListener(type, release);
}
window.addEventListener('resize', clear);
function gamepadNavigation(pad, now) {
  if (!pad || remap || !hasFocus || document.hidden) return;
  const confirm = (pad.buttons[0]?.value ?? 0) > .5; const cancel = (pad.buttons[1]?.value ?? 0) > .5;
  const direction = pad.buttons[13]?.pressed || (!active && pad.axes[1] > .6) ? 1 : pad.buttons[12]?.pressed || (!active && pad.axes[1] < -.6) ? -1 : 0;
  if (direction && now - lastNav > 200) {
    const options = [...document.querySelectorAll('button:not(:disabled):not(.touch-button), #file, input[type=search], summary')].filter(e => e.getClientRects().length);
    const i = options.indexOf(document.activeElement); const next = options[(i + direction + options.length) % options.length]; next?.focus(); lastNav = now;
  }
  if (confirm && !lastConfirm && ['BUTTON', 'SUMMARY'].includes(document.activeElement?.tagName)) document.activeElement.click();
  if (cancel && !lastCancel) { const crumbs = document.querySelectorAll('#crumbs button'); if (crumbs.length > 1) crumbs[crumbs.length - 2].click(); }
  lastConfirm = confirm; lastCancel = cancel;
}
function tick(now) {
  let pads = [];
  try { pads = [...(navigator.getGamepads?.() ?? [])].filter(p => p?.connected); } catch {}
  const select = $('controller-select');
  const signature = pads.map(p => `${p.index}:${p.id}`).join('|');
  if (select.dataset.signature !== signature) {
    select.dataset.signature = signature;
    select.replaceChildren(new Option('Auto · most recently used', 'auto'));
    for (const p of pads) select.add(new Option(`${p.index + 1}: ${p.id}`, String(p.index)));
    if (selected !== null && !pads.some(p => p.index === selected)) selected = null;
    select.value = selected === null ? 'auto' : String(selected);
  }
  let pad = pads.find(p => p.index === selected);
  if (selected === null) {
    pad = pads.find(p => p.buttons.some(b => b.value > .2) || p.axes.some(a => Math.abs(a) > .3)) ?? pads.find(p => p.index === lastPad?.index) ?? pads[0];
  }
  lastPad = pad;
  if (!remap) $('controller-status').textContent = pad ? `${pad.id} · ${pad.mapping || 'nonstandard mapping — configure below'} · ${pad.axes.length} axes / ${pad.buttons.length} buttons` : 'Connect a Bluetooth or USB controller, then press a button. iPhone: pair in Settings → Bluetooth first.';
  if (remap && pad && hasFocus && active) {
    if (!remap.armed && pad.buttons.every(b => b.value < .2)) remap.armed = true;
    const type = 'axis' in mapping[remap.action] ? 'axis' : 'button';
    const index = type === 'axis' ? pad.axes.findIndex((v, i) => Math.abs(v - (remap.baseline[i] ?? 0)) > .6) : pad.buttons.findIndex(b => b.value > .5);
    if (remap.armed && index >= 0) { mapping[remap.action] = { [type]: index }; remap = null; try { localStorage.setItem('atv-controls-v1', JSON.stringify(mapping)); } catch {} showMapping(); }
  }
  gamepadNavigation(pad, now);
  const keyboard = {};
  for (const code of Object.keys(keys)) { const [action, value] = keyActions[code]; keyboard[action] = clamp((keyboard[action] ?? 0) + value); }
  const frame = active && hasFocus && !document.hidden && !remap ? combine(fromGamepad(pad, mapping, Number($('deadzone').value)), keyboard, touch, axisTouch) : fromGamepad(null);
  if (active && now < trickUntil) frame.trick = 1;
  if (frame.pause > .5 && !lastPause) paused = !paused;
  lastPause = frame.pause > .5;
  if (active) {
    for (const [key, value] of Object.entries(frame)) { const output = $(`value-${key}`); if (output) { output.textContent = value.toFixed(2); output.previousElementSibling.value = value; } }
    $('raw-input').textContent = pad ? `Axes: ${pad.axes.map(v => v.toFixed(2)).join('  ')}\nButtons: ${pad.buttons.map((b, i) => b.value > .05 ? `${i}=${b.value.toFixed(2)}` : '').filter(Boolean).join('  ') || 'released'}` : 'No gamepad data yet. Touch and keyboard work independently.';
    $('input-events').textContent = `Pause: ${paused ? 'on' : 'off'} · Trick: ${now < trickUntil ? trickDirection : frame.trick > .5 ? 'modifier held' : 'none'}`;
    // Stable seam for the future fixed-step Rust simulation; no gameplay is simulated.
    window.dispatchEvent(new CustomEvent('atv-input', { detail: { ...frame, trickDirection: now < trickUntil ? trickDirection : null, timestamp: now } }));
  }
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);
