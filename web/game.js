import { SpeedRush, speedRushFocus, speedRushBounds } from './speed-rush.js';
// Original content and vehicle component simulation run in the Rust worker.
import { sceneViewProjection, boundsReach, vertexBounds, worldBounds, viewPlanes, outsideView, UphillSightline, chasePresentation, cinematicPresentation, RideEffectsSettings, ImpactEvents, severedIndices, meshNodeBounds, crashSubjects, crashPresentation, shadowGlState, observedSize, RenderScale, PAINT_PALETTE, paintGlsl, paintable } from './render-support.js';
import { MAX_REMOTE_MODELS, Multiplayer, PlayerIndicators } from './multiplayer.js';
import { Frontend } from './frontend.js';
import { courseResource, parseLaunch } from './launch.js';
import { UsageAnalytics } from './analytics.js';
import { RaceOverlay } from './race.js';
import { mountTouchControls, TouchLeanIntent } from './touch.js';
import { GameAudio } from './audio.js';
import { openHostedContent } from './content.js';
import { gameplayBindings } from './bindings.js';
import { HeadUpDisplay, RaceOverlaySettings } from './hud.js';
import { FreestyleMinimap, MinimapSettings } from './minimap.js';
import { DiagnosticsOverlay } from './diagnostics.js';
import { WaterRenderer } from './water.js';
import { fogVertexShader, fogFragmentShader } from './fog.js';
import { loadTrain, TrainRenderer } from './train.js';
import { loadAmbientEffects, AmbientEffectsRenderer } from './ambient-effects.js';
import { loadWheelEffects, loadMotorcycleEffects, WheelEffectsRenderer, BloodRenderer, bloodCount } from './wheel-effects.js';
import {loadSnow,SnowRenderer} from './weather.js';
import {materialPassKey} from './scenery-materials.js';
import { MotorcyclePresentation, AtvPresentation } from './motorcycle-presentation.js';
import { GraphicsSettings, VibrationSettings, Rumble, DirtBuildup, parseDirtZones, wheelReprojection } from './graphics.js';
import { ContactShadows, shadowFootprint } from './vehicle-shadows.js';
import { TireTrackRenderer } from './tire-tracks.js';
import { LingeringDustRenderer } from './lingering-dust.js';
import { VehicleShadowMap, lightMatrices, shadowReceiverGlsl } from './shadow-map.js';
import { KeyboardHelp } from './keyboard-help.js';
const $ = id => document.getElementById(id);
const canvas = $('game');
// Stencil marks the player's own ATV/rider so the speed pass leaves it crisp.
const gl = canvas.getContext('webgl2', { antialias: true, alpha: false, stencil: true, powerPreference: 'high-performance' });
const glShadow = gl ? shadowGlState(gl) : null;
const canvasSize = observedSize(canvas, el => ({ width: el.clientWidth, height: el.clientHeight }));
// The race HUD clears the toolbar. Measure it again only after the toolbar or
// viewport changes; a per-frame read forced layout after race text updates.
let toolbarEdge = null;
const toolbarBottom = () => toolbarEdge ??= $('toolbar').getBoundingClientRect().bottom;
const remeasureToolbar = () => { toolbarEdge = null; };
if (typeof ResizeObserver === 'function') new ResizeObserver(remeasureToolbar).observe($('toolbar'));
window.addEventListener('resize', remeasureToolbar);
const renderScale = new RenderScale();
const status = (text, error = false) => { if (!error) frontend?.loadingStage(text); else frontend?.setLoading(false); $('status').textContent = text; $('status').classList.toggle('error', error); };
const decoder = new TextDecoder();
const encoder = new TextEncoder();
const worker = new Worker('worker.js', { type: 'module' });
const pending = new Map();
let requestId = 0, generation = 0, terrain, vehicle, program, uniforms, chunks = [], vehicleChunks = [], gpuTextures = [], fallbackTexture, terrainAtlas, terrainTextureMap, terrainDetailMap, frame = 0, lastTime = 0, lost = false;
let pageSuspended = false;
let focused = document.hasFocus(), gamepadsAvailable = true, menuOpen = true;
let frontend, hostedContent, hud, multiplayer;
const remoteModels = new Map();
const minimapSettings = new MinimapSettings();
const raceOverlays = new RaceOverlaySettings();
const minimap = new FreestyleMinimap(minimapSettings);
const playerIndicators = new PlayerIndicators();
const crashSettings = new RideEffectsSettings(undefined, matchMedia('(prefers-reduced-motion: reduce)').matches);
let funnyCrashes = crashSettings.enabled;
const impactEvents = new ImpactEvents();
const coarsePointer = () => { try { return matchMedia('(pointer: coarse)').matches; } catch { return false; } };
let speedRush = new SpeedRush(gl, { coarse: coarsePointer() });
crashSettings.onSpeedChange = () => { speedRush.reset(); scheduleRender(); };
crashSettings.onChaseChange = () => { uphillSightline.update(null); if (cameraFrame) presentCameraFrame(); scheduleRender(); };
crashSettings.onChange = () => {
  funnyCrashes = crashSettings.enabled;
  if(crashSettings.mode==='gorn' && terrain && !lost && !bloodRenderer)bloodRenderer=new BloodRenderer(gl);
  if (simFrame) acceptFrame({...simFrame,funny_crash:null},false);
};
// Display → Graphics. Original keeps the recovered renderer exactly; Enhanced
// and High add only presentation (see research/GRAPHICS_PRESETS.md).
const graphicsSettings = new GraphicsSettings();
let graphics = graphicsSettings.features;
const vibrationSettings = new VibrationSettings();
const rumble = new Rumble({ pad: () => focused ? gamepad() : null, touch: coarsePointer });
const atvPresentation = new AtvPresentation();
atvPresentation.quadPose = frame => (graphics.wheelReprojection && visualQuad.get(frame)) || frame.quad_node_matrices;
// The quad pose the renderer draws for a simulation frame.
const drawnQuadPose = frame => funnyCrashes && frame.funny_crash ? frame.funny_crash.quad_node_matrices : atvPresentation.quadPose(frame);
let quadLinkage = null, visualQuad = new WeakMap(), wheelLift = 0, tireTracks, shadowMap, shadowDummy, lingeringDust;
let contactShadows, dirtBuildup = new DirtBuildup(), anisotropyExtension, anisotropicTextures = new Map();
graphicsSettings.onChange = () => {
  graphics = graphicsSettings.features; atvPresentation.reset(); presentedFrame = presentedCamera = undefined;
  // Re-derive the drawn quad pose at once (re-projected wheels on or off).
  if (vehicle?.nodeMatrices && simFrame?.quad_node_matrices?.length === vehicle.nodes.length) drawnQuadPose(simFrame).forEach((m, i) => vehicle.nodeMatrices.set(m, i * 16));
  if (!graphics.contactShadows) { contactShadows?.dispose(); contactShadows = undefined; }
  if (lingeringDust && lingeringDust.emitter.capacity !== graphics.dust) { if (!lost) lingeringDust.dispose(); lingeringDust = undefined; }
  if (shadowMap && shadowMap.size !== graphics.shadowMap) { if (!lost) shadowMap.dispose(); shadowMap = undefined; }
  if (tireTracks && (tireTracks.builder.capacity !== graphics.tireTracks?.capacity)) { if (!lost) tireTracks.dispose(); tireTracks = undefined; }
  if (!lost) applyAnisotropy();
  scheduleRender();
};
// Requested anisotropy clamped to the device; 0 (Original) never sets it.
function anisotropyLevel() {
  if (!graphics.anisotropy || !gl) return 0;
  if (anisotropyExtension === undefined) {
    anisotropyExtension = gl.getExtension('EXT_texture_filter_anisotropic') ?? null;
    if (anisotropyExtension) anisotropyExtension.max = gl.getParameter(anisotropyExtension.MAX_TEXTURE_MAX_ANISOTROPY_EXT) || 1;
  }
  if (!anisotropyExtension) return 0;
  return Math.max(1, Math.min(graphics.anisotropy, anisotropyExtension.max));
}
function anisotropic(texture, target = gl.TEXTURE_2D) {
  anisotropicTextures.set(texture, target);
  const level = anisotropyLevel();
  if (level) { gl.bindTexture(target, texture); gl.texParameterf(target, anisotropyExtension.TEXTURE_MAX_ANISOTROPY_EXT, level); }
  return texture;
}
function applyAnisotropy() {
  const level = anisotropyLevel();
  if (!anisotropyExtension) return;
  gl.activeTexture(gl.TEXTURE0);
  for (const [texture, target] of anisotropicTextures) { gl.bindTexture(target, texture); gl.texParameterf(target, anisotropyExtension.TEXTURE_MAX_ANISOTROPY_EXT, level || 1); }
  gl.bindTexture(gl.TEXTURE_2D, fallbackTexture);
  diagnostics.anisotropy = level;
}

// Restart Race goes straight to the countdown; a new race plays the intro.
let raceRestarting = false;
// Any key, tap or controller button skips the pre-race camera.
let introSkip = false, introPadHeld = true;
let raceSettings = null, raceCourse = null, cpuQuadMatrices, cpuRiderMatrices, raceCpuModels = [];
// The player's rider gear and quad paint for the loaded ride (ATVs only).
let rideAppearance = null;
const keyboardHelp = new KeyboardHelp();
const usage = new UsageAnalytics();
const raceOverlay = new RaceOverlay({restart: async () => { await $('reset').onclick({restart:true}); canvas.focus({preventScroll:true}); }, quit: () => frontend?.show('single'),
  finish: race => usage.track('race_finish', { ...usage.ride, place: race.position, field: race.field, seconds: race.elapsed })});
let simReady = false, simBusy = false, simAccumulator = 0, simFrame, simStart, simGeneration = 0, orbit = 0, cameraFrame, cameraCycle = false;
const motorcyclePresentation = new MotorcyclePresentation();
const uphillSightline = new UphillSightline();
let presentedFrame, presentedCamera;
let litFlashes = new Set();
let sceneChunks = [], sceneDraws = [], skyChunks = [], ecoGroups = [], ecoBillboard;
let waterRenderer;
let trainRenderer;
let wheelEffectsRenderer;
let ambientEffectsRenderer;
let snowRenderer;
let bloodRenderer;
const identity = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
const camera = { x: 0, y: 0, z: 0, yaw: 0, pitch: -.35 };
const keys = new Set(), pointers = new Map();
const touchLean = new TouchLeanIntent(() => touchControls.steeringSensitivity);
const touchControls = mountTouchControls({
  surface: canvas,
  flips: () => crashSettings.flipsEnabled,
  active: () => simReady && focused && !document.hidden && !menuOpen && !lost,
  focus: () => { focused = true; canvas.focus({ preventScroll: true }); syncAudio(); scheduleRender(); },
});
const diagnostics = { ready: false, frames: 0, vertices: 0, triangles: 0, chunks: 0, glError: 0, camera: [], source: null };
let audio = new GameAudio(codec, message => { diagnostics.audioError = message; status(`Audio: ${message}`, true); });
const stats = new DiagnosticsOverlay({ snapshot: () => ({ ...diagnostics, minimap: minimap.snapshot(), frame: simFrame, paused: simulationPaused(), menuOpen, focused, resolution: [canvas.width, canvas.height] }) });
// Read-only snapshot for verification; does not expose the locally opened file.
Object.defineProperty(window, 'atvTerrainStatus', { get: () => ({ ...diagnostics, minimap: minimap.snapshot(), hud: hud?.snapshot(), funnyCrashes, crashMode: crashSettings.mode, chaseCamera: crashSettings.chase, speedRush: {...speedRush.snapshot(),setting:crashSettings.speed}, impactAudio: { impacts: impactEvents.impacts }, blood: { draws: bloodRenderer?.draws ?? 0, instances: bloodRenderer?.draws ? bloodRenderer.instances : 0, count: crashSettings.mode === 'gorn' ? bloodCount(simFrame?.funny_crash?.blood) : 0, stumps: crashSettings.mode === 'gorn' ? Math.floor((simFrame?.funny_crash?.blood?.stumps?.length ?? 0) / 7) : 0 }, performance: stats.snapshot(), audio: audio.snapshot(), simulation: simReady, multiplayer: multiplayer ? {state:structuredClone(multiplayer.state),remote:structuredClone(multiplayer.remote),models:[...remoteModels.keys()],reconnecting:multiplayer.reconnecting,lastDisconnect:multiplayer.lastDisconnect,droppedSnapshots:multiplayer.droppedSnapshots,networkAdapterRoundTripMs:multiplayer.networkEncodeMs,networkAdapterCalls:multiplayer.networkEncodeCalls} : null, frame: simFrame ? structuredClone(simFrame) : null, camera: [camera.x, camera.y, camera.z, camera.yaw, camera.pitch] }) });
// Render-frame helpers (terrain height, vegetation queues) run synchronously in
// a second, main-thread instance of the worker's compiled Rust module.
let renderCore, renderCoreFailed;
const renderCoreReady = new Promise((resolve, reject) => { renderCoreFailed = reject; worker.addEventListener('message', ({ data }) => {
  if (!data.ready) return;
  const started = performance.now();
  (data.module ? WebAssembly.instantiate(data.module, {}) : WebAssembly.instantiateStreaming(fetch('atv_web.wasm'), {}).then(result => result.instance))
    .then(instance => { renderCore = renderBridge(instance.exports); diagnostics.renderCore = { sharedModule: Boolean(data.module), instantiateMs: performance.now() - started, memoryBytes: instance.exports.memory.buffer.byteLength }; resolve(renderCore); }, reject);
}); });
renderCoreReady.catch(error => status(`Renderer failed: ${error.message}`, true));
function renderBridge(wasm) {
  const call = (op, bytes) => {
    if (!wasm.input_resize(bytes.byteLength)) throw new Error('Render input exceeds limit.');
    new Uint8Array(wasm.memory.buffer, wasm.input_ptr(), bytes.byteLength).set(bytes);
    const ok = wasm.run(op, 0), text = decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.output_ptr(), wasm.output_len()));
    if (diagnostics.renderCore) diagnostics.renderCore.memoryBytes = wasm.memory.buffer.byteLength;
    if (!ok) throw new Error(text);
    return JSON.parse(text);
  };
  let floats = 0;
  return {
    call, terrainHeight: (x, z) => wasm.render_terrain_height(x, z),
    // Player quad linkage for Enhanced/High wheel re-projection (presentation only).
    quadPose(steering, angles, compression) {
      const nodes = wasm.render_quad_pose(steering, ...angles, ...compression);
      return nodes ? new Float32Array(wasm.memory.buffer, wasm.render_quad_pose_matrices(), nodes * 16) : null;
    },
    loadVegetation(bytes) { const layout = call(142, bytes); floats = layout.capacity * (layout.billboard_floats + layout.mesh_floats); return layout; },
    // Views are recreated per frame because memory growth detaches old buffers.
    vegetation(x, z, forwardX, forwardZ, serial) {
      if (!wasm.render_vegetation_select(x, z, forwardX, forwardZ, serial)) return null;
      const at = wasm.render_vegetation_counts(), [visible, scanned, far, transition, near, groups] = new Uint32Array(wasm.memory.buffer, at, 6);
      return { visible, scanned, far, transition, near, groups: new Uint32Array(wasm.memory.buffer, at + 24, groups), instances: new Float32Array(wasm.memory.buffer, wasm.render_vegetation_instances(), floats) };
    },
  };
}
worker.onmessage = ({ data }) => {
  if (data.startupError) { renderCoreFailed(new Error(data.startupError)); status(data.startupError, true); return; }
  const request = pending.get(data.id);
  if (!request) return;
  if (data.missing) { request.retry(); return; }
  pending.delete(data.id);
  if (Number.isFinite(data.wasmBytes)) diagnostics.wasmBytes = data.wasmBytes;
  if (request.op === 50 && Number.isFinite(data.operationMs)) diagnostics.physicsBatchMs = data.operationMs;
  if ([81,82,83].includes(request.op) && Number.isFinite(data.operationMs)) {
    const meters=diagnostics.networkOperations ??= {};
    const meter=meters[request.op] ??= {calls:0,totalMs:0,maxMs:0};
    meter.calls++;meter.totalMs+=data.operationMs;meter.maxMs=Math.max(meter.maxMs,data.operationMs);
  }
  data.error ? request.reject(new Error(data.error)) : request.resolve(data.bytes);
};
worker.onerror = event => {
  status(`Decoder failed: ${event.message}`, true);
  for (const request of pending.values()) request.reject(new Error('Decoder failed; reload the page.'));
  pending.clear();
};
// Archive parsing (2) and member extraction (3) reuse one large input many
// times. The worker keeps recently used inputs under a key, so a call sends
// only the key; an evicted or collected input is sent again in full.
const RESIDENT_LIMIT = 48 * 1048576;
const residentKeys = new WeakMap(), resident = new Map();
const residentDrop = key => { if (resident.delete(key)) worker.postMessage({ release: key }); };
const residentRelease = new FinalizationRegistry(residentDrop);
let residentNext = 0;
function codecMessage(id, op, index, bytes, resend = false) {
  if ((op !== 2 && op !== 3) || bytes.byteLength < 65536 || bytes.byteLength > RESIDENT_LIMIT) return { id, op, index, bytes };
  let key = residentKeys.get(bytes);
  if (key === undefined) { key = ++residentNext; residentKeys.set(bytes, key); residentRelease.register(bytes, key); }
  const held = resident.has(key) && !resend;
  resident.delete(key); resident.set(key, bytes.byteLength);
  if (held) return { id, op, index, key };
  let total = [...resident.values()].reduce((sum, size) => sum + size, 0);
  for (const [old, size] of resident) { if (total <= RESIDENT_LIMIT || old === key) break; residentDrop(old); total -= size; }
  return { id, op, index, key, bytes };
}
function codec(op, bytes, index = 0) {
  return new Promise((resolve, reject) => {
    const id = ++requestId;
    pending.set(id, { resolve, reject, op, retry: () => worker.postMessage(codecMessage(id, op, index, bytes, true)) });
    worker.postMessage(codecMessage(id, op, index, bytes));
  });
}
async function json(op, bytes, index = 0) { return JSON.parse(decoder.decode(await codec(op, bytes, index))); }
async function command(op, value) { return json(op, encoder.encode(JSON.stringify(value))); }
async function read(file, offset, count) {
  if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(count) || offset < 0 || count < 0 || count > 128 * 1048576 || offset + count > file.size) throw new Error('Invalid disc file extent.');
  if (file.readRange) return file.readRange(offset, count);
  return new Uint8Array(await file.slice(offset, offset + count).arrayBuffer());
}
async function discTerrain(file, token) {
  let root;
  for (let sector = 16; sector < 64; sector++) {
    const bytes = await read(file, sector * 2048, 2048);
    if (decoder.decode(bytes.subarray(1, 6)) !== 'CD001') throw new Error('Open the ISO disc image, extracted from its ZIP.');
    if (bytes[0] === 1) { root = await json(0, bytes); break; }
    if (bytes[0] === 255) break;
  }
  if (!root) throw new Error('No ISO9660 primary volume descriptor.');
  const queue = [{ ...root, path: '' }], files = [], seen = new Set();
  while (queue.length) {
    if (token !== generation) throw new Error('Opening cancelled.');
    const dir = queue.pop(), key = `${dir.lba}:${dir.bytes}`;
    if (seen.has(key) || seen.size > 10000) throw new Error('Invalid disc directory tree.');
    seen.add(key);
    if (dir.bytes > 16 * 1048576) throw new Error('Disc directory is too large.');
    for (const entry of await json(1, await read(file, dir.lba * 2048, dir.bytes))) {
      const item = { ...entry, path: `${dir.path}/${entry.name}` };
      entry.directory ? queue.push(item) : files.push(item);
    }
  }
  const boot = files.find(entry => entry.path === '/SCUS_972.11');
  if (!boot) throw new Error('This requires the USA v2.00 disc.');
  const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', await read(file, boot.lba * 2048, boot.bytes)))].map(n => n.toString(16).padStart(2, '0')).join('');
  if (hash !== '11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49') throw new Error('Disc executable does not match USA v2.00.');
  return terrainFromContent(file, files, token);
}
// Request every archive a ride reads up front so they download in parallel;
// the sound bank yields bandwidth to the course and vehicle.
async function prefetchRide(content, environment, racing) {
  const { archive, placements } = courseResource(environment, racing);
  const model = await json(89, new Uint8Array([frontend?.riderId ?? 0]), frontend?.quadId ?? 1).catch(() => null);
  const rider = model && !model.motorcycle ? [model.rider, '/ANIMSRDR.RES', '/RIDER/RIDER_MT.RES'] : [];
  content.prefetch([archive, placements, model?.quad, model?.quad_materials, ...rider, '/OVERLAYS.RES', '/STARTEVT.RES', '/AUDIOTBL.RES']);
  content.prefetch(['/AUDIO/AUDIO.RES'], 'low');
}
async function terrainFromContent(file, files, token, environment = "env01", racing = false) {
  const {archive:archivePath,sceneStem,modeStem,placements} = courseResource(environment,racing);
  const memberNamed = (entry, name) => entry.name.split(/[\\/]/).at(-1).toLowerCase() === name.toLowerCase();
  // Sounds decode alongside the course; the ride still starts with them loaded.
  const audioLoaded = audio.load({ file, files, read, isCurrent: () => token === generation, quadId: frontend?.quadId ?? 1, female: frontend?.riderId === 1 });
  audioLoaded.catch(() => {});
  const archive = files.find(entry => entry.path === archivePath);
  if (!archive) throw new Error(`Original ${environment.toUpperCase()} terrain archive is missing.`);
  status('Reading original terrain…');
  const resource = await read(file, archive.lba * 2048, archive.bytes);
  const entries = await json(2, resource);
  const index = entries.findIndex(entry => memberNamed(entry, `${environment}.tr2`));
  if (index < 0) throw new Error('Original terrain is missing from the archive.');
  const sceneIndex = entries.findIndex(entry => memberNamed(entry, `${sceneStem}.scn`));
  if (sceneIndex < 0) throw new Error('Original start position is missing.');
  const modeIndex = entries.findIndex(entry => memberNamed(entry, `${modeStem}.toy`));
  const mode = modeIndex < 0 ? null : decoder.decode(await codec(3, resource, modeIndex));
  const sceneMetadata = await command(60, { scene: decoder.decode(await codec(3, resource, sceneIndex)), mode });
  const waterIndex = sceneMetadata.water_file ? entries.findIndex(entry => memberNamed(entry, sceneMetadata.water_file.toLowerCase())) : -1;
  if (sceneMetadata.water_file && waterIndex < 0) throw new Error('Original water definition is missing.');
  const water = waterIndex < 0 ? null : { resource, index: waterIndex };
  const cameraIndex = entries.findIndex(entry => entry.name.split(/[\\/]/).at(-1).toLowerCase() === 'followcampresets.pb');
  if (cameraIndex < 0) throw new Error('Original camera presets are missing.');
  const cameraPresets = await codec(3, resource, cameraIndex);
  const scene = { position: sceneMetadata.start_position, yaw: Math.atan2(sceneMetadata.start_direction[0], sceneMetadata.start_direction[2]) };
  let course = null, cpuNames = [], cpuRoster = null;
  if (racing) {
    const tdf = entries.findIndex(entry => memberNamed(entry, `${environment}nate.tdf`));
    if (tdf < 0) throw new Error('Original National course is missing.');
    course = await json(95, await codec(3,resource,tdf));
    // Pre-race camera shots <venue>nat1.vub… play before the countdown.
    for (let shot = 1; shot <= 4; shot++) {
      const vub = entries.findIndex(entry => memberNamed(entry, `${environment}nat${shot}.vub`));
      if (vub < 0) break;
      await json(113, await codec(3, resource, vub), shot - 1);
    }
    // After the finish a director cuts between these trackside cameras.
    const tv = entries.findIndex(entry => memberNamed(entry, `${environment}nat.tv`));
    if (tv >= 0) await json(114, await codec(3, resource, tv));
    // Rust places the grid behind the original TDF start gate.
    scene.position=course.start.position.slice(); scene.yaw=course.start.yaw;
    const ai = entries.findIndex(entry => memberNamed(entry,'airiders.pb'));
    if (ai < 0) throw new Error('Original CPU roster is missing.');
    const roster=await json(54,await codec(3,resource,ai));
    cpuNames=roster.sections.filter(s => /^AI[1-5]$/i.test(s.name)).map(s => s.fields.find(f => f.key.toLowerCase()==='name')?.value).filter(Boolean);
    if (cpuNames.length < 5) throw new Error('Original CPU names are incomplete.');
    // Original roster (00210d20/0021f680): CPU quads from the course or player
    // ATV group, and AI riders with their own names, numbers and outfits.
    const groupsIndex = entries.findIndex(entry => memberNamed(entry,'atvgroups.pb'));
    if (groupsIndex >= 0) {
      const champ = Number(/^\s*ChampQuadGroup\s*=\s*(-?\d+)/im.exec(mode ?? '')?.[1] ?? -1);
      cpuRoster = await command(105, { groups: decoder.decode(await codec(3,resource,groupsIndex)), riders: decoder.decode(await codec(3,resource,ai)), quad: frontend?.quadId ?? 1, female: (frontend?.riderId ?? 0) === 1, champ_group: champ, count: 4, seed: Math.floor(Math.random() * 2 ** 31) });
      cpuNames = cpuRoster.map(entry => entry.name);
    }
  }
  const model = await json(89, new Uint8Array([frontend?.riderId ?? 0]), frontend?.quadId ?? 1);
  diagnostics.playerModel = model;
  const quadFile = files.find(entry => entry.path === model.quad);
  if (!quadFile) throw new Error('Original ATV archive is missing.');
  status('Reading original ATV…');
  const quadArchive = await read(file, quadFile.lba * 2048, quadFile.bytes);
  const quadEntries = await json(2, quadArchive);
  const quadIndex = quadEntries.findIndex(entry => memberNamed(entry, model.quad_mesh));
  if (quadIndex < 0) throw new Error('Original ATV mesh is missing.');
  const bikeSource = model.motorcycle ? await json(102, quadArchive, model.quad_id) : null;
  const meshBytes = await codec(3, quadArchive, quadIndex), mesh = bikeSource?.mesh ?? await json(11, meshBytes);
  const physicsIndex = quadEntries.findIndex(entry => memberNamed(entry, model.physics));
  if (physicsIndex < 0) throw new Error('Original ATV physics is missing.');
  const physics = await codec(3, quadArchive, physicsIndex);
  const chassisIndex = quadEntries.findIndex(entry => memberNamed(entry, model.chassis));
  if (!model.motorcycle && chassisIndex < 0) throw new Error('Original chassis collision is missing.');
  const chassis = model.motorcycle ? null : await codec(3, quadArchive, chassisIndex);
  const actorIndex = quadEntries.findIndex(entry => memberNamed(entry, model.actor));
  if (!model.motorcycle && actorIndex < 0) throw new Error('Original actor collision rays are missing.');
  const actorRays = model.motorcycle ? null : await codec(3, quadArchive, actorIndex);
  let toyIndex = entries.findIndex(entry => memberNamed(entry, `${environment}.toy`)), toy;
  if (toyIndex >= 0) toy = await codec(3, resource, toyIndex);
  else {
    const sceneArchiveFile = files.find(entry => entry.path === placements);
    if (!sceneArchiveFile) throw new Error('Original terrain materials are missing.');
    const sceneArchive = await read(file, sceneArchiveFile.lba * 2048, sceneArchiveFile.bytes), sceneEntries = await json(2, sceneArchive);
    toyIndex = sceneEntries.findIndex(entry => memberNamed(entry, `${environment}.toy`));
    if (toyIndex < 0) throw new Error('Original terrain materials are missing.');
    toy = await codec(3, sceneArchive, toyIndex);
  }
  // CrowdPresent stadiums: crowd.aud lists (races) and <venue>.aud Soultree loops.
  let crowd = null;
  const venueText = async name => { const i = entries.findIndex(entry => memberNamed(entry, name)); return i < 0 ? null : decoder.decode(await codec(3, resource, i)); };
  // <venue>.aud: drive-on, ambient and Soultree world sounds for every venue.
  const venueAud = await venueText(`${environment}.aud`);
  if (sceneMetadata.crowd_present) crowd = { aud: racing ? await venueText('crowd.aud') : null, soultree: venueAud };
  const materialFile = files.find(entry => entry.path === model.quad_materials);
  if (!materialFile) throw new Error('Original ATV materials archive is missing.');
  const materialArchive = await read(file, materialFile.lba * 2048, materialFile.bytes), materialEntries = await json(2, materialArchive);
  const textures = await baseTextures(mesh, materialArchive, materialEntries);
  status('Reading original rider…');
  async function archiveAt(path) {
    const entry = files.find(entry => entry.path.toLowerCase() === path.toLowerCase());
    if (!entry) throw new Error(`Original archive is missing: ${path}`);
    const bytes = await read(file, entry.lba * 2048, entry.bytes);
    return { bytes, entries: await json(2, bytes) };
  }
  async function member(archive, suffix) {
    const index = archive.entries.findIndex(entry => entry.name.replaceAll('\\', '/').toLowerCase().endsWith(suffix.toLowerCase()));
    if (index < 0) throw new Error(`Original rider file is missing: ${suffix}`);
    return codec(3, archive.bytes, index);
  }
  // Installs the ATV2 rider rig and motions (player, or race CPUs on quads).
  async function loadAtvRider(selection) {
    const riderArchive = await archiveAt(selection.rider), motions = await archiveAt('/ANIMSRDR.RES');
    const mesh = await json(27, await member(riderArchive, '/' + selection.rider_mesh));
    const riding = await json(51, await member(riderArchive, '/' + selection.rider_catalog));
    for (let index = 0; index < riding.length; index++) {
      if (token !== generation) throw new Error('Opening cancelled.');
      await codec(52, await member(motions, '/' + riding[index].path.split('/').at(-1)), index);
    }
    return { mesh, pose: await codec(53, new Uint8Array()).then(bytes => JSON.parse(decoder.decode(bytes))) };
  }
  let riderMesh, pose, riderTextures, cpuRiderPose = null;
  if (bikeSource) {
    riderMesh = bikeSource.riderMesh; pose = bikeSource.riderPose;
    riderTextures = await baseTextures(riderMesh, quadArchive, quadEntries);
    // MX vs. ATV fields mix classes: race CPUs keep their original quads and rider.
    if (course && cpuRoster) cpuRiderPose = (await loadAtvRider(await json(89, new Uint8Array([frontend?.riderId ?? 0]), 1))).pose;
  } else {
  ({ mesh: riderMesh, pose } = await loadAtvRider(model));
  const riderMaterials = await archiveAt('/RIDER/RIDER_MT.RES');
  riderTextures = await baseTextures(riderMesh, riderMaterials.bytes, riderMaterials.entries);
  rideAppearance = model.motorcycle ? null : frontend?.appearance ?? null;
  diagnostics.rideAppearance = rideAppearance ? { ...rideAppearance, gear: [...rideAppearance.gear] } : null;
  for (const [material, decoded] of rideAppearance ? await gearTextures(rideAppearance, riderMesh) : []) riderTextures.set(material, decoded);
  }
  //00208cc0 reads the actual common/mode TOYs' [Weather], not legacy
  //SCN[Rain]. There is one local view per browser, including online rides;
  //the original online versus local-player-count mapping is not established.
  //Selection belongs to this scene load and survives ATV resets/GL.
  const weather = await command(120, { common: decoder.decode(toy), mode: mode ?? '', random: Math.floor(Math.random() * 0x80000000), players: 1 });
  if (weather.removed_model) sceneMetadata.models = sceneMetadata.models.filter(model => model.name.toLowerCase() !== weather.removed_model.toLowerCase());
  // National drop gate: its own soultree from the race TOY, not a scene
  // model (00222cd0); Rust poses it each frame after GO.
  if (racing && mode) {
    const gate = await json(116, encoder.encode(mode));
    if (gate) sceneMetadata.models.push({ name: gate.model, position: gate.matrix.slice(12, 15), look: gate.matrix.slice(8, 11), up: gate.matrix.slice(4, 7), use_lighting: true, dynamic_lighting: false, world_matrix: gate.matrix, drop_gate: true });
  }
  diagnostics.weather = weather;
  await audio.loadWeather({selection:weather,isCurrent:()=>token===generation});
  status('Decoding original terrain textures…');
  const bytes = await codec(3, resource, index), layout = await json(30, bytes), images = [];
  for (const record of layout.records) {
    if (token !== generation) throw new Error('Opening cancelled.');
    const mip = record.mips.at(-1);
    const rgba = await codec(35, bytes.slice(mip.offset, mip.offset + mip.bytes));
    images.push({ width: record.width, height: record.height, rgba });
  }
  // 002098b0 derives the original detail resource from the terrain basename.
  const detailIndex = entries.findIndex(entry => memberNamed(entry, `${environment}_d.pst`));
  if (detailIndex < 0) throw new Error('Original ground detail texture is missing.');
  const detailBytes = await codec(3, resource, detailIndex);
  const detail = { ...await json(7, detailBytes), rgba: await codec(8, detailBytes) };
  status('Decoding original sky…');
  const skyIndex = entries.findIndex(entry => entry.name.split(/[\\/]/).at(-1).toLowerCase() === sceneMetadata.cube_file.toLowerCase());
  if (skyIndex < 0) throw new Error('Original scene sky is missing.');
  const skyBytes = await codec(3, resource, skyIndex), sky = await json(38, skyBytes);
  for (const tile of sky.tiles) {
    if (token !== generation) throw new Error('Opening cancelled.');
    tile.rgba = await codec(35, skyBytes.slice(tile.offset, tile.offset + tile.bytes));
  }
  status('Reading original scene models…');
  const sceneMeshes = new Map(), sceneTextures = new Map(), unsupported = [];
  for (const name of new Set(sceneMetadata.models.map(model => model.name))) {
    const modelIndex = entries.findIndex(entry => entry.name.replaceAll('\\', '/').toLowerCase() === `geometry/${name}.psg`.toLowerCase());
    if (modelIndex < 0) throw new Error(`Original scene mesh is missing: ${name}`);
    try {
      // Static scene setters replace the PSG root; upload root-relative geometry.
      const model = await json(11, await codec(3, resource, modelIndex), 1);
      const textures = await baseTextures(model, resource, entries, sceneTextures, true);
      for (const [name, texture] of textures) sceneTextures.set(name, texture);
      sceneMeshes.set(name, model);
    } catch (error) { unsupported.push({ name, error: error.message }); }
  }
  status('Reading original vegetation…');
  const ecoIndex = entries.findIndex(entry => memberNamed(entry, `${environment}.esb`));
  let ecosystem;
  if (ecoIndex >= 0) {
    const ecoBytes = await codec(3, resource, ecoIndex);
    const data = await json(58, ecoBytes), models = new Map(), textures = new Map();
    for (const species of data.species) {
      if (token !== generation) throw new Error('Opening cancelled.');
      const member = entries.findIndex(entry => entry.name.replaceAll('\\', '/').toLowerCase() === `eco/${species.model}.psg`.toLowerCase());
      if (member < 0) throw new Error(`Original vegetation mesh is missing: ${species.model}`);
      const decoded = await json(59, await codec(3, resource, member));
      models.set(species.slot, decoded);
      for (const [name, image] of await baseTextures(decoded.mesh, resource, entries, textures)) textures.set(name, image);
      const material = species.billboard.replace(/\.[^.]+$/, '');
      for (const [name, image] of await baseTextures({ materials: [material], lods: [{ groups: [{ material: 0 }] }] }, resource, entries, textures)) textures.set(name, image);
    }
    ecosystem = { ...data, bytes: ecoBytes, models, textures };
  }
  status('Reading original scene collisions…');
  const collisionMeshes = new Map(), collisionPlacements = [];
  for (const placement of sceneMetadata.collision_models) {
    if (token !== generation) throw new Error('Opening cancelled.');
    const name = placement.name.toLowerCase();
    const collisionIndex = entries.findIndex(entry => entry.name.replaceAll('\\', '/').toLowerCase() === `geometry/${name}.col`);
    //00206e90 ignores missing COL files; authored non-solid overlays have none.
    if (collisionIndex < 0) continue;
    if (!collisionMeshes.has(name)) collisionMeshes.set(name, await codec(3, resource, collisionIndex));
    collisionPlacements.push({ ...placement, name });
  }
  const wheelEffects = model.motorcycle ? await loadMotorcycleEffects({resource:quadArchive,entries:quadEntries,config:bikeSource.particleConfig,codec,json}) : await loadWheelEffects({ resource, entries, quadArchive, quadEntries, codec, json, scene: decoder.decode(await codec(3, resource, sceneIndex)), mode });
  // Ambient mode sidecars are independent of the Freestyle spawn/material TOY.
  const ambientModeIndex=entries.findIndex(entry=>memberNamed(entry,`${environment}${environment==='game01'?'so':'fr'}.toy`));
  const ambientMode=ambientModeIndex>=0&&ambientModeIndex!==toyIndex?decoder.decode(await codec(3,resource,ambientModeIndex)):modeIndex!==toyIndex?mode:null;
  const ambientEffects = await loadAmbientEffects({resource,entries,codec,json,scene:decoder.decode(await codec(3,resource,sceneIndex)),common:decoder.decode(toy),mode:ambientMode,wheelEffects});
  const snow=await loadSnow({selection:weather,resource,entries,codec,json,gpuTexture:model.motorcycle?null:wheelEffects?.gpuTexture});
  const train = environment === 'env08' ? await loadTrain({ resource, entries, modeIndex, codec, json, baseTextures, isCurrent: () => token === generation }) : null;
  if (train && mode) await audio.loadSpatial({ toy: new TextEncoder().encode(mode), isCurrent: () => token === generation });
  // Rust reads grid RiderSpacing from the race mode TOY or, for Supercross, the scene TOY.
  const spacingSources = [mode, toy && decoder.decode(toy)];
  // Each roster CPU rides its own original quad: physics, mesh and collision.
  for (const entry of cpuRoster ?? []) {
    const cpu = await json(89, new Uint8Array([frontend?.riderId ?? 0]), entry.quad), archive = await archiveAt(cpu.quad);
    const part = async name => { const index = archive.entries.findIndex(e => memberNamed(e, name)); if (index < 0) throw new Error(`Original CPU quad resource is missing: ${name}`); return codec(3, archive.bytes, index); };
    const parts = [await part(cpu.physics), await part(cpu.quad_mesh), await part(cpu.chassis), await part(cpu.actor)];
    const bundle = new Uint8Array(16 + parts.reduce((n, p) => n + p.length, 0)), view = new DataView(bundle.buffer); let at = 16;
    parts.forEach((p, i) => { view.setUint32(i * 4, p.length, true); bundle.set(p, at); at += p.length; });
    entry.bundle = bundle;
  }
  await audioLoaded;
  return { crowd, venueAud, weather, snow, cpuRiderPose, motorcycle: model.motorcycle ? {archive:quadArchive,id:model.quad_id} : null, bytes, environment, course, cpuNames, cpuRoster, spacingSources, name: entries[index].name, scene, cameraPresets, mesh, meshBytes, physics, chassis, actorRays, collisions: { meshes: collisionMeshes, placements: collisionPlacements }, water, train, wheelEffects, ambientEffects, toy, textures, sky, terrainTextures: { layout, images, detail }, ecosystem, world: { metadata: sceneMetadata, meshes: sceneMeshes, textures: sceneTextures, unsupported }, rider: { mesh: riderMesh, pose, textures: riderTextures } };
}
async function baseTextures(mesh, materialArchive, materialEntries, cache = new Map(), multiPass = false) {
  const findMaterial = name => materialEntries.findIndex(entry => entry.name.split(/[\\/]/).at(-1).toLowerCase() === name.toLowerCase());
  const textures = new Map();
  for (const material of new Set(mesh.lods[0].groups.map(group => mesh.materials[group.material]))) {
    if (cache.has(material)) {const first=cache.get(material);for(const key of multiPass?first.passKeys??[material]:[material])textures.set(key,cache.get(key));continue;}
    const materialIndex = findMaterial(`${material}.psm`);
    const originalText = materialIndex < 0 ? '' : decoder.decode(await codec(3, materialArchive, materialIndex));
    // Rust reads ordered passes, bindings, shader, alpha test and mud maps. Without
    // an authored PSM, 001bf430 ->001b5080 binds the material's own stem (set0).
    const passes=await command(145,{text:originalText,multi_pass:multiPass,fallback:materialIndex<0?material:null}),passKeys=[];
    for(const [passIndex,described] of passes.entries()) {
    const passKey=materialPassKey(material,passIndex);passKeys.push(passKey);
    const {binding,pass,reflective,source:materialText}=described,shader=described.shader??undefined;
    if (!binding) continue;
    if (described.unsupported_reflection) { const unsupported = { shader, reflective }; textures.set(passKey, unsupported); cache.set(passKey, unsupported); continue; }
    const textureIndex = findMaterial(`${binding}.pst`);
    const atmIndex = pass?.texture_animation || textureIndex < 0 ? findMaterial(`${binding}.atm`) : -1;
    const tgaIndex = textureIndex < 0 && atmIndex < 0 ? findMaterial(`${binding}.tga`) : -1;
    if (textureIndex < 0 && atmIndex < 0 && tgaIndex < 0) { if (materialIndex < 0) continue; throw new Error(`Original material texture is missing: ${binding}`); }
    const pst = await codec(3, materialArchive, atmIndex >= 0 ? atmIndex : tgaIndex >= 0 ? tgaIndex : textureIndex);
    let image;
    try {
      const info = await json(atmIndex >= 0 ? 56 : tgaIndex >= 0 ? 97 : 7, pst);
      const animationFrames = atmIndex >= 0 && pass?.texture_animation ? await Promise.all(Array.from({length:info.frames},(_,index)=>codec(57,pst,index))) : null;
      // Reverse-looping crowd ATMs retain their original first frame until the
      // native playback path is recovered. The material reports that limitation.
      const rgba = animationFrames ? animationFrames[0] : await codec(atmIndex >= 0 ? 57 : tgaIndex >= 0 ? 98 : 8, pst, 0);
      // Scene cutout values are Rust's explicit GEQUAL reading, including the original
      // tower lattice. FB_ONLY/source-alpha edge blending is not yet a complete GS match.
      image = { ...info, rgba, animationFrames, shader, reflective, pass, materialText, alphaTest: described.alpha_test, sceneAlphaFailure: described.scene_alpha_failure, sceneAlphaCutoff: described.scene_alpha_cutoff };
    } catch (error) { image = { shader, reflective, error: error.message }; }
    // MX MudAnimator maps, or the ATV2 PixelAnimation dirt pass (vehicle/rider).
    const mudName = described.mud_maps.length > 1 ? described.mud_maps[1] : described.dirt_map, mudIndex = mudName ? findMaterial(`${mudName}.pst`) : -1;
    if (mudIndex >= 0) { const bytes=await codec(3,materialArchive,mudIndex); image.mud={...await json(7,bytes),rgba:await codec(8,bytes)}; }
    textures.set(passKey, image); cache.set(passKey, image);
    }
    if(multiPass&&textures.has(material))textures.get(material).passKeys=passKeys.filter(key=>textures.has(key));
  }
  return textures;
}
const vertexSource = `#version 300 es
precision highp float;
layout(location=0) in vec3 position;
layout(location=1) in vec3 normal;
layout(location=2) in vec2 uv;
layout(location=3) in float quadNode;
layout(location=4) in vec3 ecoPosition;
layout(location=5) in vec2 ecoScale;
layout(location=6) in vec4 vertexColor;
layout(location=7) in vec3 reflectionPosition;
layout(location=8) in vec3 reflectionNormal;
layout(location=9) in vec3 ecoState;
layout(location=10) in vec4 ecoAtlasHorizontal;
layout(location=11) in vec4 ecoAtlasVertical;
uniform int sceneryReflectionPlane;
uniform vec2 sceneryUvOffset;
out vec4 authoredColor;
uniform bool useEcosystem;
uniform bool ecoBillboard;
uniform bool ecoPerInstance;
flat out float ecoReference;
uniform vec2 ecoRotation;
uniform vec3 ecoAmbient;
uniform vec3 ecoSunlight;
uniform vec3 ecoSunDirection;
uniform bool ecoSunPresent;
uniform mat4 viewProjection;
uniform mat4 model;
uniform vec3 eye;
uniform bool skyPass;
uniform bool useQuadPose;
uniform mat4 quadBones[32];
out vec3 surfaceNormal;
out float distanceToEye;
out float worldHeight;
out vec2 texcoord;
out vec2 worldXZ;
${fogVertexShader}
void main() {
  texcoord = uv + sceneryUvOffset;
  authoredColor = vertexColor;
  ecoReference = 0.0;
  mat4 localPose = useQuadPose ? quadBones[int(quadNode)] : mat4(1.0);
  vec4 worldPosition = model * localPose * vec4(position, 1.0);
  if (useEcosystem) {
    vec2 rotation = ecoBillboard || !ecoPerInstance ? ecoRotation : ecoState.xy;
    vec3 scaled = position * vec3(ecoScale.x, ecoScale.y, ecoScale.x);
    if (ecoBillboard) {
      float horizontal = normal.x < 0.5 ? ecoAtlasHorizontal.x : normal.x < 1.5 ? 0.0 : ecoAtlasHorizontal.y;
      scaled = vec3(horizontal*ecoScale.x,(position.y+ecoAtlasVertical.w)*ecoScale.y,0.0);
      texcoord = vec2(normal.x < 0.5 ? ecoAtlasHorizontal.z : normal.x < 1.5 ? ecoAtlasVertical.x : ecoAtlasHorizontal.w,
        position.y > 0.5 ? ecoAtlasVertical.y : ecoAtlasVertical.z);
    }
    worldPosition = vec4(ecoPosition + vec3(rotation.x * scaled.x - rotation.y * scaled.z, scaled.y, rotation.y * scaled.x + rotation.x * scaled.z), 1.0);
    // TriStripPrologue transforms the light through the scaled instance basis.
    // DiffuseCompressed retains authored normal magnitude and bias quantization.
    vec3 localLight = vec3((rotation.x * ecoScale.x) * ecoSunDirection.x + (rotation.y * ecoScale.x) * ecoSunDirection.z,
      ecoSunDirection.y * ecoScale.y, (-rotation.y * ecoScale.x) * ecoSunDirection.x + (rotation.x * ecoScale.x) * ecoSunDirection.z);
    float weight = max((localLight.x * normal.x + localLight.y * normal.y) + localLight.z * normal.z, 0.0);
    float alpha = ecoPerInstance ? min(64.0,64.0-(ecoState.z-16.0)*1.3333334) : 64.0;
    ecoReference = trunc(alpha) / 255.0;
    vec4 biased = vec4(8388608.0) + vec4(ecoAmbient,alpha);
    biased = min(biased + vec4(ecoSunlight,ecoSunPresent ? 1.0 : 0.0) * weight,vec4(8388863.0));
    authoredColor = vec4(floatBitsToUint(biased) & uvec4(255u));
    if (ecoBillboard) {
      //002fea30 shades the left edge, center pivot and right edge separately.
      vec3 look = vec3(-ecoRotation.y,0.0,ecoRotation.x);
      vec3 lightNormal = normal.x < 0.5 ? vec3(-look.z+look.x*0.1,0.0,look.x+look.z*0.1)
        : normal.x < 1.5 ? -look : vec3(look.z+look.x*0.1,0.0,-look.x+look.z*0.1);
      vec2 horizontalSun = ecoSunDirection.xz;
      float sunLength = length(horizontalSun);
      vec3 sun = sunLength > 0.0 ? vec3(horizontalSun.x/sunLength,0.0,horizontalSun.y/sunLength) : vec3(0.0);
      float diffuse = max(dot(normalize(lightNormal),sun),0.0);
      authoredColor = vec4(trunc(min(ecoAmbient+ecoSunlight*diffuse,vec3(127.0))),min(64.0,trunc(ecoState.z*1.3333334)));
    }
  }
  if (sceneryReflectionPlane != 0) {
    // TriStripPrologue transforms the world camera into each rigid node's
    // coordinates. The compressed reflection programs retain the authored
    // scaled normal; normalizing it would change the original UVs.
    mat4 nodeWorld = model * quadBones[int(quadNode)];
    vec3 localEye = transpose(mat3(nodeWorld)) * (eye - nodeWorld[3].xyz);
    vec3 direction = normalize(localEye - reflectionPosition);
    vec3 reflected = vec3(0.5) - 0.5 * direction + reflectionNormal * dot(reflectionNormal, direction);
    texcoord = sceneryReflectionPlane == 1 ? reflected.xy : sceneryReflectionPlane == 2 ? reflected.xz : reflected.yz;
  }
  worldXZ = worldPosition.xz;
  surfaceNormal = mat3(model) * mat3(localPose) * normal;
  worldHeight = worldPosition.y;
  distanceToEye = length(worldPosition.xyz - eye);
  gl_Position = viewProjection * worldPosition;
  prepareFog(distanceToEye, gl_Position);
  if (useEcosystem) authoredColor *= gl_Position.w;
  if (skyPass) gl_Position = gl_Position.xyww;
}`;
const fragmentSource = `#version 300 es
precision highp float;
in vec3 surfaceNormal;
in float worldHeight;
in vec2 texcoord;
in vec2 worldXZ;
in vec4 authoredColor;
uniform bool useEcosystem;
uniform bool ecoBillboard;
uniform bool ecoPerInstance;
flat in float ecoReference;
uniform bool useVertexColor;
uniform bool sceneryBlend;
uniform bool alphaFailPass;
uniform vec2 heights;
uniform vec3 surfaceColor;
uniform vec3 eye;
uniform vec4 crashFocus;
uniform sampler2D baseTexture;
uniform bool useTexture;
uniform sampler2D motorcycleMudTexture;
uniform float motorcycleMudAmount;
uniform bool mudReveal;
uniform float alphaCutoff;
uniform bool useTerrainTexture;
uniform highp sampler2DArray terrainColors;
uniform highp usampler2D terrainMapping;
uniform sampler2D terrainDetail;
uniform float terrainSpacing;
uniform vec2 terrainDetailScale;
uniform vec3 ambientLight;
uniform vec3 sunlightColor;
uniform vec3 sunlightDirection;
uniform vec3 fogColor;
uniform bool unlit;
uniform bool skyPass;
uniform vec2 quadPaint;
out vec4 color;
${fogFragmentShader}
${paintGlsl}
${shadowReceiverGlsl}
void main() {
  // Let both crash actors remain visible through foreground foliage. A stable
  // screen-door fade keeps tree context without extra translucent draw passes.
  if (useEcosystem && crashFocus.w > 0.0) {
    vec3 ray = crashFocus.xyz - eye;
    vec3 p = vec3(worldXZ.x,worldHeight,worldXZ.y) - eye;
    float t = dot(p,ray) / max(dot(ray,ray),1.0);
    if (t > 0.0 && t < 1.0 && length(p-ray*t) < crashFocus.w
      && mod(gl_FragCoord.x + gl_FragCoord.y*3.0,8.0) > 0.5) discard;
  }
  vec3 n = normalize(surfaceNormal);
  float altitude = clamp((worldHeight - heights.x) / max(1.0, heights.y - heights.x), 0.0, 1.0);
  vec3 base = mix(vec3(.29,.32,.22), vec3(.60,.54,.41), altitude);
  base = mix(base, vec3(.40,.38,.34), smoothstep(.14,.65,1.0-n.y));
  if (surfaceColor.x >= 0.0) base = surfaceColor;
  float sourceAlpha = 1.0;
  if (useTexture) {
    vec4 sampled = texture(baseTexture, texcoord);
    if (useVertexColor) sampled *= (useEcosystem ? authoredColor / fogInterpolation.y : authoredColor) / 128.0;
    //002fe710 overrides TEST with GREATER/KEEP and a per-placement reference.
    if (useEcosystem && ecoPerInstance) { if (sampled.a <= (ecoBillboard ? alphaCutoff : ecoReference)) discard; }
    else if (alphaFailPass ? sampled.a >= alphaCutoff : sampled.a < alphaCutoff) discard;
    base = applyPaint(sampled.rgb, quadPaint);
    if (motorcycleMudAmount > 0.0) {vec4 mud = texture(motorcycleMudTexture,texcoord);
      // ATV2 dirt passes (authored reveal): heavier-alpha texels dirty first.
      float weight = mudReveal ? clamp(mud.a*(255.0/128.0),0.0,1.0)*smoothstep(1.0-motorcycleMudAmount,1.12-motorcycleMudAmount,mud.a) : clamp(mud.a*(255.0/128.0)*motorcycleMudAmount,0.0,1.0);
      base = mix(base,mud.rgb,weight);}
    // GS source alpha uses 128 for unity, while decoded PST bytes use 255.
    sourceAlpha = sceneryBlend ? clamp(sampled.a * (255.0 / 128.0), 0.0, 1.0) : 1.0;
  }
  if (useTerrainTexture) {
    vec2 grid = worldXZ / (terrainSpacing * 64.0);
    ivec2 tile = clamp(ivec2(floor(grid)), ivec2(0), textureSize(terrainMapping, 0) - 1);
    uint layer = texelFetch(terrainMapping, tile, 0).r;
    // 001325c0 selects the 64-cell subtexture; 00131790 submits its 8px border.
    vec2 coord = fract(grid) * .9375 + .03125;
    // Derivatives must precede fract(): its discontinuity otherwise selects
    // tiny mip levels at each 64-cell boundary and produces blurry strips.
    base = textureGrad(terrainColors, vec3(coord, float(layer)), dFdx(grid) * .9375, dFdy(grid) * .9375).rgb;
    // Single-player detail coordinates: param9=8 at 00209b0c,
    // 00168490 computes (8*256/textureWidth)/64 per terrain cell.
    vec4 detail = texture(terrainDetail, worldXZ / terrainSpacing * terrainDetailScale);
    // Near terrain VU RGB=127, A=64; GS MODULATE divides by 128.
    // ALPHA=0x44 is (Cs-Cd)*As+Cd. PST alpha stays in source units.
    // Original projected-coverage pass selection is still pending; WebGL
    // mip filtering currently averages this detail continuously at distance.
    base = mix(base, detail.rgb, detail.a * (255.0 / 256.0)) * (127.0 / 128.0);
  }
  float sunlight = max(dot(n, normalize(sunlightDirection)),0.0);
  vec3 lit = unlit ? base : base * (ambientLight + sunlightColor * sunlight);
  if (shadowReceive && !skyPass) lit *= vehicleShadowFactor(vec3(worldXZ.x, worldHeight, worldXZ.y));
  color = vec4(skyPass ? base : mix(fogColor, lit, fogCoverage()),sourceAlpha);
}`;
function shader(type, source) {
  const result = gl.createShader(type);
  gl.shaderSource(result, source); gl.compileShader(result);
  if (!gl.getShaderParameter(result, gl.COMPILE_STATUS)) { const error = gl.getShaderInfoLog(result); gl.deleteShader(result); throw new Error(error); }
  return result;
}
function initializeGl() {
  const vertex = shader(gl.VERTEX_SHADER, vertexSource), fragment = shader(gl.FRAGMENT_SHADER, fragmentSource);
  program = gl.createProgram(); gl.attachShader(program, vertex); gl.attachShader(program, fragment); gl.linkProgram(program);
  gl.deleteShader(vertex); gl.deleteShader(fragment);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
  uniforms = Object.fromEntries(['viewProjection', 'model', 'eye', 'heights', 'surfaceColor', 'baseTexture', 'useTexture', 'useTerrainTexture', 'terrainColors', 'terrainMapping', 'terrainDetail', 'terrainDetailScale', 'terrainSpacing', 'ambientLight', 'sunlightColor', 'sunlightDirection', 'fogColor', 'unlit', 'skyPass', 'useVertexColor', 'sceneryBlend', 'sceneryUvOffset', 'alphaFailPass', 'crashFocus'].map(name => [name, gl.getUniformLocation(program, name)]));
  uniforms.useEcosystem = gl.getUniformLocation(program, 'useEcosystem');
  uniforms.sceneryReflectionPlane = gl.getUniformLocation(program, 'sceneryReflectionPlane');
  uniforms.fogPacking = gl.getUniformLocation(program, 'fogPacking');
  uniforms.fogEnabled = gl.getUniformLocation(program, 'fogEnabled');
  uniforms.ecoRotation = gl.getUniformLocation(program, 'ecoRotation');
  for (const name of ['ecoBillboard','ecoPerInstance']) uniforms[name] = gl.getUniformLocation(program,name);
  for (const name of ['ecoAmbient','ecoSunlight','ecoSunDirection','ecoSunPresent']) uniforms[name] = gl.getUniformLocation(program,name);
  uniforms.alphaCutoff = gl.getUniformLocation(program, 'alphaCutoff');
  uniforms.quadPaint = gl.getUniformLocation(program, 'quadPaint');
  uniforms.useQuadPose = gl.getUniformLocation(program, 'useQuadPose');
  uniforms.quadBones = gl.getUniformLocation(program, 'quadBones[0]');
  uniforms.motorcycleMudTexture = gl.getUniformLocation(program,'motorcycleMudTexture');
  uniforms.motorcycleMudAmount = gl.getUniformLocation(program,'motorcycleMudAmount');
  uniforms.mudReveal = gl.getUniformLocation(program,'mudReveal');
  for (const name of ['shadowReceive','vehicleShadow','shadowMatrix']) uniforms[name] = gl.getUniformLocation(program, name);
  gl.useProgram(program);gl.uniform1i(uniforms.motorcycleMudTexture,4);gl.uniform1i(uniforms.vehicleShadow,7);
  // Unit 7 is reserved for the vehicle shadow sampler: it always holds a
  // comparison texture (High binds its map); other passes use units 0-6.
  shadowDummy = gl.createTexture(); gl.activeTexture(gl.TEXTURE7); gl.bindTexture(gl.TEXTURE_2D, shadowDummy);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.DEPTH_COMPONENT16, 1, 1, 0, gl.DEPTH_COMPONENT, gl.UNSIGNED_SHORT, new Uint16Array([65535]));
  for (const [k, v] of [[gl.TEXTURE_MIN_FILTER, gl.NEAREST], [gl.TEXTURE_MAG_FILTER, gl.NEAREST], [gl.TEXTURE_COMPARE_MODE, gl.COMPARE_REF_TO_TEXTURE]]) gl.texParameteri(gl.TEXTURE_2D, k, v);
  gl.activeTexture(gl.TEXTURE0);
  fallbackTexture = texture({ width: 1, height: 1, rgba: new Uint8Array([255, 255, 255, 255]) });
  gl.activeTexture(gl.TEXTURE4);gl.bindTexture(gl.TEXTURE_2D,fallbackTexture);gl.activeTexture(gl.TEXTURE0);
  terrainAtlas = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D_ARRAY, terrainAtlas);
  gl.texImage3D(gl.TEXTURE_2D_ARRAY, 0, gl.RGBA8, 1, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 255, 255]));
  gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  terrainTextureMap = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, terrainTextureMap);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.R16UI, 1, 1, 0, gl.RED_INTEGER, gl.UNSIGNED_SHORT, new Uint16Array([0]));
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  terrainDetailMap = texture({ width: 1, height: 1, rgba: new Uint8Array([255, 255, 255, 0]) });
  gl.enable(gl.DEPTH_TEST); gl.disable(gl.CULL_FACE); gl.clearColor(.34, .43, .53, 1);
}
function clearMesh() {
  speedRush.dispose();
  for (const model of remoteModels.values()) { disposeRemoteGpu(model); }
  bloodRenderer?.dispose(); bloodRenderer = undefined;
  wheelEffectsRenderer?.dispose(); wheelEffectsRenderer = undefined;
  ambientEffectsRenderer?.dispose(); ambientEffectsRenderer = undefined;
  snowRenderer?.dispose(); snowRenderer=undefined;
  trainRenderer?.dispose(); trainRenderer = undefined;
  waterRenderer?.dispose(); waterRenderer = undefined;
  for (const chunk of [...chunks, ...vehicleChunks, ...sceneChunks, ...skyChunks]) { gl.deleteVertexArray(chunk.vao); gl.deleteBuffer(chunk.vertices); gl.deleteBuffer(chunk.indices); if(chunk.cutBuffer)gl.deleteBuffer(chunk.cutBuffer); }
  for (const texture of gpuTextures) gl.deleteTexture(texture);
  for (const group of ecoGroups) { gl.deleteBuffer(group.instances); for (const chunk of group.chunks) { gl.deleteVertexArray(chunk.vao); gl.deleteBuffer(chunk.vertices); gl.deleteBuffer(chunk.indices); } }
  const billboard=ecoBillboard;
  if(billboard){gl.deleteBuffer(billboard.instances);gl.deleteVertexArray(billboard.chunk.vao);gl.deleteBuffer(billboard.chunk.vertices);gl.deleteBuffer(billboard.chunk.indices);}
  ecoGroups = []; ecoBillboard = null;
  gpuTextures = [];
  anisotropicTextures = new Map(); anisotropic(terrainAtlas, gl.TEXTURE_2D_ARRAY); anisotropic(terrainDetailMap);
  chunks = []; vehicleChunks = []; sceneChunks = []; sceneDraws = []; skyChunks = [];
}
function texture({ width, height, rgba }) {
  if (rgba.byteLength !== width * height * 4) throw new Error('Invalid texture dimensions.');
  const result = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, result);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
  gl.generateMipmap(gl.TEXTURE_2D);
  return result;
}
function uploadTerrainTextures() {
  const source = terrain?.textures;
  if (!source) return;
  const { layout, images, detail } = source;
  if (images.length !== layout.records.length || images.length > gl.getParameter(gl.MAX_ARRAY_TEXTURE_LAYERS)) throw new Error('Terrain texture layer count is unsupported.');
  const width = Math.ceil((terrain.width - 1) / 64), height = Math.ceil((terrain.depth - 1) / 64), mapping = new Uint16Array(width * height).fill(65535);
  for (const region of layout.regions) {
    if (region.span_cells !== 256 || region.x % 256 || region.z % 256 || region.subtextures.some(value => value === null)) throw new Error('Unsupported terrain texture region.');
    for (let index = 0; index < 16; index++) {
      const x = region.x / 64 + index % 4, z = region.z / 64 + Math.floor(index / 4), layer = region.subtextures[index];
      if (x >= width || z >= height || layer >= images.length || mapping[z * width + x] !== 65535) throw new Error('Invalid terrain texture region coverage.');
      mapping[z * width + x] = layer;
    }
  }
  if (mapping.some(value => value === 65535)) throw new Error('Terrain texture regions do not cover the terrain.');
  gl.bindTexture(gl.TEXTURE_2D, terrainTextureMap);
  gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.R16UI, width, height, 0, gl.RED_INTEGER, gl.UNSIGNED_SHORT, mapping);
  gl.bindTexture(gl.TEXTURE_2D_ARRAY, terrainAtlas);
  gl.texImage3D(gl.TEXTURE_2D_ARRAY, 0, gl.RGBA8, 256, 256, images.length, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
  images.forEach((image, index) => {
    if (image.width !== 256 || image.height !== 256 || image.rgba.length !== 256 * 256 * 4) throw new Error('Unsupported terrain texture image.');
    gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, index, 256, 256, 1, gl.RGBA, gl.UNSIGNED_BYTE, image.rgba);
  });
  gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
  gl.generateMipmap(gl.TEXTURE_2D_ARRAY);
  if (detail.rgba.length !== detail.width * detail.height * 4) throw new Error('Invalid terrain detail texture.');
  gl.bindTexture(gl.TEXTURE_2D, terrainDetailMap);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, detail.width, detail.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, detail.rgba);
  gl.generateMipmap(gl.TEXTURE_2D);
  // Mean colour of each original terrain layer, for authored dust tinting.
  const colors = new Float32Array(images.length * 3);
  images.forEach((image, layer) => { let n = 0; for (let i = 0; i < image.rgba.length; i += 64) { colors[layer * 3] += image.rgba[i]; colors[layer * 3 + 1] += image.rgba[i + 1]; colors[layer * 3 + 2] += image.rgba[i + 2]; n++; } for (let c = 0; c < 3; c++) colors[layer * 3 + c] /= n * 255; });
  terrain.layerTint = { colors, mapping, width, height };
  diagnostics.terrainTextures = images.length;
  diagnostics.terrainTextureResolution = [256, 256];
  diagnostics.terrainDetailTexture = [detail.width, detail.height];
  diagnostics.terrainLighting = 'original base/detail blend; projected pass selection pending';
}
function upload(vertices, indices, hasUv = false, hasNodes = false, hasColors = false, hasReflection = false) {
  const chunk = { vao: gl.createVertexArray(), vertices: gl.createBuffer(), indices: gl.createBuffer(), count: indices.length };
  gl.bindVertexArray(chunk.vao); gl.bindBuffer(gl.ARRAY_BUFFER, chunk.vertices); gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
  const stride = hasReflection ? 76 : hasColors ? 48 : hasNodes ? 36 : hasUv ? 32 : 24;
  gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, stride, 0);
  gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 3, gl.FLOAT, false, stride, 12);
  if (hasUv) { gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2, 2, gl.FLOAT, false, stride, 24); }
  if (hasNodes) { gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3, 1, gl.FLOAT, false, stride, 32); }
  if (hasColors) { gl.enableVertexAttribArray(6); gl.vertexAttribPointer(6, 4, gl.FLOAT, false, stride, 32); }
  if (hasReflection) {
    gl.enableVertexAttribArray(7); gl.vertexAttribPointer(7, 3, gl.FLOAT, false, stride, 48);
    gl.enableVertexAttribArray(8); gl.vertexAttribPointer(8, 3, gl.FLOAT, false, stride, 60);
    gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3, 1, gl.FLOAT, false, stride, 72);
  }
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, chunk.indices); gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW);
  chunk.bounds = vertexBounds(vertices, stride / 4);
  return chunk;
}
// Visible ground for cameras and presentation: Rust interpolates the dense render
// grid the terrain mesh is built from. Gameplay starts use the contact plane (op 140).
function terrainHeight(x, z) { return renderCore.terrainHeight(x, z); }
function setVehicle(source) {
  vehicle = undefined;
  if (!source.mesh || !source.scene) return;
  const groups = source.mesh.lods?.[0]?.groups;
  if (!groups?.length) throw new Error('Original ATV mesh has no geometry.');
  let min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
  for (const group of groups) for (const vertex of group.vertices) for (let axis = 0; axis < 3; axis++) { min[axis] = Math.min(min[axis], vertex.position[axis]); max[axis] = Math.max(max[axis], vertex.position[axis]); }
  if (![...min, ...max].every(Number.isFinite)) throw new Error('Invalid ATV bounds.');
  const position = source.scene.position.slice();
  if (position[0] < 0 || position[2] < 0 || position[0] > terrain.sizeX || position[2] > terrain.sizeZ) throw new Error('Scene start lies outside the recovered terrain.');
  // Keep the original scene spawn height unless its bind-pose bounds intersect ground.
  position[1] = Math.max(position[1], terrainHeight(position[0], position[2]) - min[1]);
  if (source.mesh.nodes.length > 32) throw new Error('Quad node count exceeds supported GPU pose capacity.');
  vehicle = { groups, nodes: source.mesh.nodes, nodeMatrices: new Float32Array(source.mesh.nodes.flatMap(node => node.world_matrix)), position, yaw: source.scene.yaw, length: max[2] - min[2], width: max[0] - min[0], height: max[1] - min[1], restLift: null, materials: source.mesh.materials, textures: source.textures, rider: source.rider };
  vehicle.pelvis = pelvisNode(source.rider?.mesh?.nodes);
}
function transform(matrix, value, point = false) {
  return [0, 1, 2].map(row => matrix[row] * value[0] + matrix[4 + row] * value[1] + matrix[8 + row] * value[2] + (point ? matrix[12 + row] : 0));
}
function inverseDirection(matrix, value) {
  const [a, b, c] = [[matrix[0], matrix[1], matrix[2]], [matrix[4], matrix[5], matrix[6]], [matrix[8], matrix[9], matrix[10]]];
  const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const determinant = dot(a, cross(b, c));
  if (Math.abs(determinant) < 1e-10) throw new Error('Rider bind matrix is singular.');
  return [cross(b, c), cross(c, a), cross(a, b)].map(row => dot(row, value) / determinant);
}
function disposeRemoteGpu(model) {
  for (const chunk of model.chunks ?? []) { gl.deleteVertexArray(chunk.vao); gl.deleteBuffer(chunk.vertices); gl.deleteBuffer(chunk.indices); }
  for (const texture of model.gpuTextures ?? []) gl.deleteTexture(texture);
  model.chunks = null; model.gpuTextures = [];
  for (const entry of model.gear?.values() ?? []) entry.gpu = null;
}
function buildRemoteGpu(model) {
  if (!model.source || model.chunks || lost) return;
  model.chunks = []; model.gpuTextures = [];
  for (const [mesh, textures, rider] of [[model.source.mesh,model.source.textures,false],[model.source.riderMesh,model.source.riderTextures,true]]) {
    if (mesh.nodes.length > 32) throw new Error('Remote rig exceeds GPU pose capacity.');
    const gpu = new Map();
    for (const [name, decoded] of textures) { if (!decoded.rgba) continue; const value = texture(decoded); gpu.set(name,value); model.gpuTextures.push(value); }
    for (const group of mesh.lods[0].groups) {
      const vertices = new Float32Array(group.vertices.length * 9);
      group.vertices.forEach((v,i) => vertices.set([...v.local_position,...inverseDirection(mesh.nodes[v.node].world_matrix,v.normal),...v.uv,v.node],i*9));
      const chunk = upload(vertices,new Uint16Array(group.indices),true,true); chunk.rider=rider; chunk.material=mesh.materials[group.material]; chunk.texture=gpu.get(mesh.materials[group.material]); chunk.fog=textures.get(mesh.materials[group.material])?.pass?.fog ?? true; model.chunks.push(chunk);
    }
  }
}
async function remoteAssets(quad, rider) {
  const key = `${quad}:${rider}`;
  if (remoteModels.has(key)) {
    const model=await remoteModels.get(key).pending;
    if (multiplayer?.started && model.prepareAnimation) await model.prepareAnimation();
    return model;
  }
  if(remoteModels.size>=MAX_REMOTE_MODELS) {
    const active=new Set((multiplayer?.state?.lobby?.players??[]).map(p=>`${p.atv_model}:${p.rider_model}`));
    for(const [cachedKey,cached] of remoteModels) {if(!active.has(cachedKey)){disposeRemoteGpu(cached);remoteModels.delete(cachedKey);}}
  }
  const model = {source:null,chunks:null,gpuTextures:[]}; remoteModels.set(key,model);
  model.pending = (async () => {
    const content = await (hostedContent ??= openHostedContent());
    const selected = await json(89,new Uint8Array([rider]),quad);
    const archive = async path => { const f=content.files.find(f=>f.path===path); if (!f) throw new Error(`Player resource unavailable: ${path}`); const bytes=await read(content.file,f.lba*2048,f.bytes); return {bytes,entries:await json(2,bytes)}; };
    const member = async (a,name) => { const index=a.entries.findIndex(e=>e.name.replaceAll('\\','/').split('/').at(-1).toLowerCase()===name.toLowerCase()); if(index<0) throw new Error('Remote player mesh missing.'); return codec(3,a.bytes,index); };
    const quadArchive=await archive(selected.quad), riderArchive=await archive(selected.rider);
    const bike=selected.motorcycle ? await json(102,quadArchive.bytes,quad) : null;
    const mesh=bike?.mesh ?? await json(11,await member(quadArchive,selected.quad_mesh));
    const riderMesh=bike?.riderMesh ?? await json(11,await member(riderArchive,selected.rider_mesh));
    const quadMaterials=await archive(selected.quad_materials), riderMaterials=selected.motorcycle ? riderArchive : await archive('/RIDER/RIDER_MT.RES');
    // Garage figures: authored top speed and gearbox from the quad's physics.
    const stats=bike ? null : await (async()=>json(108,await member(quadArchive,selected.physics)))().catch(()=>null);
    model.source={mesh,riderMesh,riderPose:bike?.riderPose,bikeArchive:bike?quadArchive.bytes:null,stats,textures:await baseTextures(mesh,quadMaterials.bytes,quadMaterials.entries),riderTextures:await baseTextures(riderMesh,riderMaterials.bytes,riderMaterials.entries)};
    if(!bike) {
      model.prepareAnimation=async()=> {
        const known=multiplayer?.state?.network?.rigKeys;
        if(model.animationReady && (!known || (multiplayer.state.network.rigRevision??0)<model.readyRevision || known.includes(quad*2+rider)))return;
        if(model.animationReady && known && !known.includes(quad*2+rider)) {model.animationPending=null;model.animationReady=false;}
        model.animationPending ??= (async()=> {
          const motions=await archive('/ANIMSRDR.RES');
          const parts=[quadArchive.bytes,riderArchive.bytes,motions.bytes];
          const bundle=new Uint8Array(12+parts.reduce((n,p)=>n+p.length,0)),view=new DataView(bundle.buffer);let at=12;
          for(let i=0;i<parts.length;i++){view.setUint32(i*4,parts[i].length,true);bundle.set(parts[i],at);at+=parts[i].length;}
          const installed=await json(104,bundle,quad*2+rider);model.readyRevision=installed.rigRevision;model.animationReady=true;
        })();
        await model.animationPending;
      };
      if(multiplayer?.started)await model.prepareAnimation();
    }
    return model;
  })().catch(error=> {if(remoteModels.get(key)===model)remoteModels.delete(key);throw error;});
  return model.pending;
}
async function selectionAssets(quad, rider) {
  const model = await remoteAssets(quad,rider);
  if (!multiplayer?.state?.lobby) {
    const keep = new Set([...remoteModels.keys()].slice(-2));
    for (const [key, cached] of remoteModels) if (!keep.has(key) && cached.source) { disposeRemoteGpu(cached); remoteModels.delete(key); }
  }
  return model.source;
}
let menuRiderResource, menuRiderTextures;
async function menuRiderAssets(rider, isCurrent, quad = 1) {
  if (quad >= 27) {
    const source = await selectionAssets(quad, rider);
    if (!isCurrent()) return null;
    const original = await json(103, source.bikeArchive, quad);
    // MX selection clips face the opposite way to the ATV menu rig.
    return {...original, textures:source.riderTextures, previewYaw:Math.PI-.6};
  }
  const content = await hostedContent;
  const file = content.files.find(f => f.path === '/UI/RESOURCE/PAGEA/PAGE07.RES');
  if (!file) return null;
  menuRiderResource ??= read(content.file,file.lba*2048,file.bytes);
  const bytes = await menuRiderResource;
  if (!menuRiderTextures) {
    const entries = await json(2,bytes), textures = new Map();
    for (const [index,entry] of entries.entries()) {
      if (!entry.name.toLowerCase().endsWith('.pst')) continue;
      const image = await codec(3,bytes,index), info = await json(7,image);
      // Menu rider alpha is a specular/reflection mask, never transparency.
      textures.set(entry.name.split('/').at(-1).toLowerCase().replace(/\.pst$/, ''),{...info,rgba:await codec(8,image),opaque:true});
    }
    textures.set('atv2_shirt_ui_export',textures.get('atv2_shirt_ui'));
    menuRiderTextures = textures;
  }
  if (!isCurrent()) return null;
  const original = await json(92,bytes,rider);
  return {...original,textures:menuRiderTextures};
}
// AIRiders outfit materials, decoded once per stem from the rider material archive.
const outfitCache = new Map();
let riderMaterialArchive;
// Returns rider-mesh material → decoded outfit texture. Shirts keep the
// mesh slot's variant suffix (atv2_shirt001 → <AI base shirt>001).
async function outfitTextures(outfit, riderMesh) {
  const content = await (hostedContent ??= openHostedContent());
  const file = content.files.find(f => f.path === '/RIDER/RIDER_MT.RES');
  if (!file || !riderMesh) return new Map();
  riderMaterialArchive ??= read(content.file, file.lba * 2048, file.bytes).then(async bytes => ({ bytes, entries: await json(2, bytes) }));
  const archive = await riderMaterialArchive, replacements = new Map();
  for (const material of riderMesh.materials) {
    const slot = outfitSlot(material), name = slot && outfit[slot];
    if (name) replacements.set(material, slot === 'shirt' ? name + (/\d+$/.exec(material)?.[0] ?? '001') : name);
  }
  const names = [...new Set(replacements.values())];
  const mesh = { materials: names, lods: [{ groups: names.map((_, material) => ({ material })) }] };
  let decoded;
  try { decoded = await baseTextures(mesh, archive.bytes, archive.entries, outfitCache); }
  catch { return new Map(); } // Older content packages carry only the player outfit.
  const textures = new Map();
  for (const [material, name] of replacements) if (decoded.get(name)?.rgba) textures.set(material, decoded.get(name));
  return textures;
}
// Player gear (frontend appearance) → rider-mesh material textures. Each
// catalog item lists in-game material candidates; shirts pass their base so
// the mesh slot suffix applies, and the jersey's sleeves follow its index.
async function gearTextures(appearance, riderMesh) {
  const catalog = frontend?.gearCatalog, gear = appearance?.gear;
  if (!catalog || !gear?.some(Boolean) || !riderMesh) return new Map();
  const content = await (hostedContent ??= openHostedContent());
  const file = content.files.find(f => f.path === '/RIDER/RIDER_MT.RES');
  if (!file) return new Map();
  riderMaterialArchive ??= read(content.file, file.lba * 2048, file.bytes).then(async bytes => ({ bytes, entries: await json(2, bytes) }));
  const names = new Set((await riderMaterialArchive).entries.map(e => e.name.split(/[\\/]/).at(-1).toLowerCase().replace(/\.(psm|pst)$/, '')));
  const outfit = {};
  ['helmet','glove','boot','shirt','sleeve','pant','goggle'].forEach((slot, at) => {
    const item = gear[at] ? catalog[at]?.items[gear[at] - 1] : null;
    if (!item) return;
    const found = item.materials.find(m => names.has(m));
    if (found) outfit[slot] = slot === 'shirt' ? found.replace(/001$/, '') : found;
  });
  return outfitTextures(outfit, riderMesh);
}
// Remote riders: gear textures per model and appearance, uploaded lazily.
function peerGear(model, appearance) {
  const key = appearanceKey(appearance);
  if (!key) return null;
  model.gear ??= new Map();
  let entry = model.gear.get(key);
  if (!entry) {
    entry = { decoded: null, gpu: null };
    model.gear.set(key, entry);
    void gearTextures(appearance, model.source?.riderMesh).then(decoded => { entry.decoded = decoded; }).catch(() => { entry.decoded = new Map(); });
  }
  if (entry.decoded && !entry.gpu && !lost) { entry.gpu = new Map(); for (const [name, decoded] of entry.decoded) { const gpu = texture(decoded); entry.gpu.set(name, gpu); model.gpuTextures.push(gpu); } }
  return entry.gpu;
}
const appearanceKey = a => a && (a.gear?.some(Boolean) || a.paint) ? `${a.gear.join('.')}:${a.paint}` : '';
const peerAppearance = peer => { const bytes = multiplayer?.state?.lobby?.players?.find(p => p.id === peer)?.appearance; return Array.isArray(bytes) && bytes.length === 8 ? { gear: bytes.slice(0, 7), paint: bytes[7] } : null; };
// Rider mesh material → AIRiders outfit field.
const outfitSlot = material => /helmet/i.test(material) ? 'helmet' : /glove/i.test(material) ? 'glove' : /goggle/i.test(material) ? 'goggle' : /pant/i.test(material) ? 'pant' : /shirt/i.test(material) ? 'shirt' : /sleeve/i.test(material) ? 'sleeve' : /boot/i.test(material) ? 'boot' : null;
function disposeCpuOutfits() { for (const cpu of raceCpuModels) for (const texture of cpu.gpu?.values() ?? []) gl.deleteTexture(texture); for (const cpu of raceCpuModels) cpu.gpu = null; }
// Node matrices arrive as [[16 floats], ...]; copy them into a reused buffer
// instead of allocating flattened arrays for every rider every frame.
function bonesInto(rows, target) {
  const out = target?.length === rows.length * 16 ? target : new Float32Array(rows.length * 16);
  for (let i = 0; i < rows.length; i++) out.set(rows[i], i * 16);
  return out;
}
// Quad and rider chunks share one bone palette and model matrix each; upload
// them only when a chunk switches between the two.
function drawPosedChunks(chunks, transforms, bones, prepare) {
  let draws = 0, current = null;
  for (const chunk of chunks) {
    const part = chunk.rider ? 1 : 0;
    if (part !== current) { current = part; gl.uniformMatrix4fv(uniforms.model, false, transforms[part]); gl.uniformMatrix4fv(uniforms.quadBones, false, bones[part]); }
    const bound = prepare(chunk);
    gl.uniform1i(uniforms.fogEnabled, Boolean(terrain.world?.metadata.fog) && chunk.fog);
    gl.uniform1i(uniforms.useTexture, Boolean(bound)); gl.bindTexture(gl.TEXTURE_2D, bound ?? fallbackTexture);
    gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0); draws++;
  }
  return draws;
}
function drawRosterCpu(cpu, slot, transform, riderTransform) {
  const roster = raceCpuModels[slot], model = roster?.model;
  if (!model?.source) return 0;
  let quad, rider;
  if (cpu.physical) {
    if (cpu.physical.quad_matrices.length !== model.source.mesh.nodes.length || cpu.physical.rider_matrices.length !== model.source.riderMesh.nodes.length) return 0;
    quad = roster.quadBones = bonesInto(cpu.physical.quad_matrices, roster.quadBones); rider = roster.riderBones = bonesInto(cpu.physical.rider_matrices, roster.riderBones);
  } else {
    // Grid before the first physics step: the quad's rest pose and the
    // player's seated pose (same rider rig).
    if (!cpuRiderMatrices || cpuRiderMatrices.length !== model.source.riderMesh.nodes.length * 16) return 0;
    model.restMatrices ??= new Float32Array(model.source.mesh.nodes.flatMap(node => [...node.world_matrix]));
    quad = model.restMatrices; rider = cpuRiderMatrices;
  }
  buildRemoteGpu(model);
  if (!roster.gpu) { roster.gpu = new Map(); for (const [name, decoded] of roster.outfit ?? []) roster.gpu.set(name, texture(decoded)); }
  gl.uniform1i(uniforms.useQuadPose, true);
  return drawPosedChunks(model.chunks ?? [], [transform, riderTransform], [quad, rider], chunk => (chunk.rider ? roster.gpu.get(chunk.material) : null) ?? chunk.texture);
}
const peerBones = new Map();
function drawRemotePlayers() {
  let draws=0, gearPeers=0, paintPeers=0;
  for (const peer of multiplayer?.remote ?? []) {
    const model=remoteModels.get(`${peer.atv_model}:${peer.rider_model}`); if (!model?.source) continue;
    if (peer.quad_node_matrices.length!==model.source.mesh.nodes.length || peer.rider_node_matrices.length!==model.source.riderMesh.nodes.length) continue;
    buildRemoteGpu(model);
    const bones=peerBones.get(peer.peer) ?? {};peerBones.set(peer.peer,bones);
    const quad=bones.quad=bonesInto(peer.quad_node_matrices,bones.quad), rider=bones.rider=bonesInto(peer.rider_node_matrices,bones.rider);
    const quadWorld=new Float32Array(peer.world_matrix), riderWorld=new Float32Array(peer.rider_world_matrix);
    gl.uniform1i(uniforms.useQuadPose,true);
    const appearance = peer.atv_model <= 26 ? peerAppearance(peer.peer) : null, gear = peerGear(model, appearance), paint = PAINT_PALETTE[appearance?.paint ?? 0] ?? PAINT_PALETTE[0];
    if (gear?.size) gearPeers++; if (appearance?.paint) paintPeers++;
    draws+=drawPosedChunks(model.chunks ?? [],[quadWorld,riderWorld],[quad,rider],chunk=>{
      gl.uniform2fv(uniforms.quadPaint,!chunk.rider&&paintable(chunk.material)?paint:PAINT_PALETTE[0]);
      return (chunk.rider && gear?.get(chunk.material)) || chunk.texture;
    });
  }
  gl.uniform2fv(uniforms.quadPaint,PAINT_PALETTE[0]);
  diagnostics.multiplayerDraws=draws; diagnostics.remoteGear={gear:gearPeers,paint:paintPeers};
}
function dustTint(x, z) {
  const t = terrain?.layerTint;
  if (!t) return [.62, .55, .45];
  const tx = Math.max(0, Math.min(t.width - 1, Math.floor(x / (terrain.spacing * 64)))), tz = Math.max(0, Math.min(t.height - 1, Math.floor(z / (terrain.spacing * 64))));
  const layer = t.mapping[tz * t.width + tx];
  return [0, 1, 2].map(c => Math.min(1, t.colors[layer * 3 + c] ?? .5));
}
// Rider rig node that carries the torso; contact shadows follow it when detached.
function pelvisNode(nodes) {
  if (!nodes?.length) return 0;
  for (const pattern of [/pelvis/i, /hip/i, /torso|spine/i]) { const at = nodes.findIndex(node => pattern.test(node.name ?? '')); if (at >= 0) return at; }
  return 0;
}
// World matrix of a rider node: model-space pose under its rider world.
function nodeWorld(world, pose, node) {
  const m = pose.length >= (node + 1) * 16 ? pose.subarray?.(node * 16, node * 16 + 16) ?? pose.slice(node * 16, node * 16 + 16) : null;
  if (!m) return world;
  const out = new Float32Array(16);
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) out[c * 4 + r] = world[r] * m[c * 4] + world[4 + r] * m[c * 4 + 1] + world[8 + r] * m[c * 4 + 2] + world[12 + r] * m[c * 4 + 3];
  return out;
}
// Enhanced/High soft contact shadows for the player, its detached rider, CPU
// racers and remote riders, in one draw under the vegetation pass.
function drawContactShadows({ viewProjection, actor, quadWorld, riderWorld, smoothRider, opponents }) {
  const ground = (x, z) => terrainHeight(x, z), rest = vehicle.restLift ?? 1.5, footprints = [];
  const distance = m => Math.hypot(m[12] - camera.x, m[13] - camera.y, m[14] - camera.z);
  const quad = (m, strength = shadowMap && graphics.shadowMap ? .35 : .6) => footprints.push(shadowFootprint({ matrix: m, length: vehicle.length, width: vehicle.width, lift: m[13] - ground(m[12], m[14]) - rest, distance: distance(m), strength }));
  const rider = (m, strength = .45) => footprints.push(shadowFootprint({ matrix: m, length: 3, width: 2.2, lift: m[13] - ground(m[12], m[14]) - .9, distance: distance(m), strength }));
  quad(quadWorld);
  // Detached: Original/Crazy wreck rig, or the ragdoll's world-space bones.
  if (actor?.rider_world_matrix || (funnyCrashes && simFrame?.funny_crash?.rider_node_matrices.length)) rider(nodeWorld(riderWorld, smoothRider, vehicle.pelvis));
  for (const cpu of opponents) {
    if (Math.hypot(cpu.origin[0] - camera.x, cpu.origin[2] - camera.z) > 240) continue;
    const m = vehicleMatrix({ origin: cpu.origin, orientation: cpu.physical?.orientation ?? [Math.cos(cpu.yaw / 2), 0, Math.sin(cpu.yaw / 2), 0] });
    quad(m);
    if (cpu.physical?.rider_world_matrix) rider(nodeWorld(new Float32Array(cpu.physical.rider_world_matrix), new Float32Array(cpu.physical.rider_matrices.flat()), vehicle.pelvis));
  }
  for (const peer of multiplayer?.remote ?? []) {
    if (!peer.world_matrix || peer.world_matrix.length !== 16) continue;
    const m = new Float32Array(peer.world_matrix); quad(m);
    const r = peer.rider_world_matrix;
    if (r?.length === 16 && Math.hypot(r[12] - m[12], r[13] - m[13], r[14] - m[14]) > 1.5) rider(new Float32Array(r));
  }
  contactShadows ??= new ContactShadows(gl);
  contactShadows.draw({ viewProjection, footprints, height: ground });
  gl.useProgram(program);
  diagnostics.contactShadows = { draws: contactShadows.draws, casters: contactShadows.casters, gpuBytes: contactShadows.gpuBytes };
}
// High: vehicle/rider-only sun shadow map, rendered before the receivers.
function renderVehicleShadows({ opponents, quadWorld, riderWorld, smoothQuad, smoothRider, width, height }) {
  if (lost || !simFrame) return false;
  try { shadowMap ??= new VehicleShadowMap(gl, graphics.shadowMap); }
  catch (error) { diagnostics.shadowMapError = error.message; graphics = { ...graphics, shadowMap: 0 }; return false; }
  const casters = [], origin = (presentedFrame ?? simFrame).origin;
  for (const chunk of vehicleChunks) casters.push({ chunk, count: chunk.count, model: chunk.rider ? riderWorld : quadWorld, bones: chunk.rider ? smoothRider : smoothQuad });
  const near = point => Math.hypot(point[0] - origin[0], point[2] - origin[2]) < 40;
  for (const [slot, cpu] of opponents.entries()) {
    if (!near(cpu.origin)) continue;
    const transform = vehicleMatrix({ origin: cpu.origin, orientation: cpu.physical?.orientation ?? [Math.cos(cpu.yaw / 2), 0, Math.sin(cpu.yaw / 2), 0] });
    const riderTransform = cpu.physical?.rider_world_matrix ? new Float32Array(cpu.physical.rider_world_matrix) : transform;
    const model = raceCpuModels[slot]?.model, quad = cpu.physical ? new Float32Array(cpu.physical.quad_matrices.flat()) : model?.restMatrices ?? cpuQuadMatrices;
    const rider = cpu.physical ? new Float32Array(cpu.physical.rider_matrices.flat()) : cpuRiderMatrices;
    for (const chunk of model?.chunks ?? vehicleChunks) casters.push({ chunk, count: chunk.count, model: chunk.rider ? riderTransform : transform, bones: chunk.rider ? rider : quad });
  }
  for (const peer of multiplayer?.remote ?? []) {
    const model = remoteModels.get(`${peer.atv_model}:${peer.rider_model}`);
    if (!model?.chunks || peer.world_matrix?.length !== 16 || !near(peer.world_matrix.slice(12, 15))) continue;
    const quad = new Float32Array(peer.quad_node_matrices.flat()), rider = new Float32Array(peer.rider_node_matrices.flat());
    const world = new Float32Array(peer.world_matrix), riderTransform = new Float32Array(peer.rider_world_matrix);
    for (const chunk of model.chunks) casters.push({ chunk, count: chunk.count, model: chunk.rider ? riderTransform : world, bones: chunk.rider ? rider : quad });
  }
  const sun = terrain.world?.metadata.lights?.find(light => light.kind === 'Directional');
  const toSun = sun?.look?.map(value => -value) ?? (sun?.position && sun?.target ? sun.position.map((value, i) => value - sun.target[i]) : [-.6, 1, .35]);
  const light = lightMatrices(origin, toSun, shadowMap.size);
  shadowMap.render(casters, light.clip, [0, 0, width, height]);
  gl.useProgram(program);
  gl.activeTexture(gl.TEXTURE7); gl.bindTexture(gl.TEXTURE_2D, shadowMap.texture); gl.activeTexture(gl.TEXTURE0);
  gl.uniformMatrix4fv(uniforms.shadowMatrix, false, light.texture);
  diagnostics.shadowMap = { size: shadowMap.size, draws: shadowMap.draws, gpuBytes: shadowMap.gpuBytes };
  return true;
}
function buildEcosystem() {
  const eco = terrain.ecosystem;
  diagnostics.vegetationPlacements = eco?.placements.length ?? 0;
  if (!eco) return;
  const textures = new Map();
  eco.billboard = ecoBillboard = null;
  for (const [name, decoded] of eco.textures) {
    if (!decoded.rgba) continue;
    const gpu = anisotropic(texture(decoded)); textures.set(name, gpu); gpuTextures.push(gpu);
  }
  // Rust registers placements (00129b70 resampling) and owns per-frame
  // selection; this layer keeps meshes, materials and instance buffers.
  const metadata = terrain.world.metadata, config = encoder.encode(JSON.stringify({ far: metadata.fog?.end ?? 768, distances: metadata.ecosystem_distances,
    species: eco.species.map(species => { const { bounds } = eco.models.get(species.slot); return { slot: species.slot, center_y: bounds.center[1], half_extents: bounds.half_extents.slice(0, 2) }; }) }));
  const framed = new Uint8Array(4 + config.length + eco.bytes.length);
  new DataView(framed.buffer).setUint32(0, config.length, true); framed.set(config, 4); framed.set(eco.bytes, 4 + config.length);
  const layout = eco.layout = renderCore.loadVegetation(framed);
  for (const species of eco.species) {
    const { mesh } = eco.models.get(species.slot);
    const group = { chunks: [], instances: gl.createBuffer() };
    gl.bindBuffer(gl.ARRAY_BUFFER, group.instances); gl.bufferData(gl.ARRAY_BUFFER, Math.min(layout.capacity, eco.placements.length) * layout.mesh_floats * 4, gl.DYNAMIC_DRAW);
    for (const source of mesh.lods[0].groups) {
      if (source.vertices.length > 65536) throw new Error('Vegetation mesh exceeds index bounds.');
      const vertices = new Float32Array(source.vertices.length * 8);
      source.vertices.forEach((v, i) => vertices.set([...v.position, ...v.normal, ...v.uv], i * 8));
      const chunk = upload(vertices, new Uint16Array(source.indices), true);
      const material = mesh.materials[source.material], decoded = eco.textures.get(material);
      chunk.texture = textures.get(material);
      chunk.fog = decoded?.pass?.fog ?? true;
      chunk.alphaCutoff = decoded?.alphaTest ?? 0;
      chunk.blend = decoded?.pass?.blend ?? false;
      chunk.depthWrite = decoded?.pass?.depth_write ?? true;
      chunk.alphaFailure = decoded?.sceneAlphaFailure ?? 0;
      gl.bindVertexArray(chunk.vao); gl.bindBuffer(gl.ARRAY_BUFFER, group.instances);
      bindEcoInstances(group.instances);
      group.chunks.push(chunk);
    }
    if (species.atlas && !eco.billboard) {
      const order = [[1,0],[0,0],[0,1],[1,1],[2,1],[2,0]];
      const vertices = new Float32Array(6*8);
      order.forEach(([column,row],i) => vertices.set([0,row,0,column,1,0,0,0],i*8));
      const chunk = upload(vertices,new Uint16Array([0,1,2,0,2,3,0,3,4,0,4,5]),true);
      const material = species.billboard.replace(/\.[^.]+$/, ''), decoded = eco.textures.get(material);
      if (!decoded?.rgba || decoded.shader !== 'EcoSystemCode2D') throw new Error(`Original vegetation atlas is unavailable: ${material}`);
      chunk.texture = textures.get(material); chunk.fog = decoded.pass?.fog ?? true;
      chunk.blend = decoded.pass?.blend ?? true; chunk.depthWrite = decoded.pass?.depth_write ?? true; chunk.alphaCutoff = decoded.alphaTest;
      const instances=gl.createBuffer();
      gl.bindVertexArray(chunk.vao);gl.bindBuffer(gl.ARRAY_BUFFER,instances);gl.bufferData(gl.ARRAY_BUFFER,layout.capacity*layout.billboard_floats*4,gl.DYNAMIC_DRAW);
      for(const [location,size,offset]of [[4,3,0],[5,2,12],[9,3,20],[10,4,32],[11,4,48]]){gl.enableVertexAttribArray(location);gl.vertexAttribPointer(location,size,gl.FLOAT,false,64,offset);gl.vertexAttribDivisor(location,1);}
      eco.billboard=ecoBillboard={chunk,instances,material};
    }
    if(species.atlas && eco.billboard.material.toLowerCase()!==species.billboard.replace(/\.[^.]+$/,'').toLowerCase())throw new Error('Mixed vegetation atlas families require separate ordered batches.');
    ecoGroups.push(group);
  }
}
function bindEcoInstances(buffer) {
  gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
  for (const [location,size,offset] of [[4,3,0],[5,2,12],[9,3,20]]) {
    gl.enableVertexAttribArray(location); gl.vertexAttribPointer(location,size,gl.FLOAT,false,32,offset); gl.vertexAttribDivisor(location,1);
  }
}
function drawEcosystem() {
  diagnostics.vegetationTriangles = 0;
  diagnostics.vegetationDraws = 0;
  if (!ecoGroups.length) return;
  const eco = terrain.ecosystem, forwardX = Math.sin(camera.yaw), forwardZ = Math.cos(camera.yaw), layout = eco.layout;
  // Rust selects (0012a1c0 phases/orientation, bounded nearest-depth heap) and
  // writes ordered instance floats; this layer uploads and submits them.
  const selection = renderCore.vegetation(camera.x, camera.z, forwardX, forwardZ, frame);
  if (!selection) return;
  const instances = selection.instances;
  gl.uniform1i(uniforms.useEcosystem,true);gl.uniform1i(uniforms.ecoPerInstance,true);
  gl.uniform1i(uniforms.unlit,true);gl.uniform1i(uniforms.useQuadPose,false);
  const lighting=terrain.world.metadata.ecosystem_lighting;
  gl.uniform3fv(uniforms.ecoAmbient,lighting.ambient);gl.uniform3fv(uniforms.ecoSunlight,lighting.sunlight);
  gl.uniform3fv(uniforms.ecoSunDirection,lighting.direction);gl.uniform1i(uniforms.ecoSunPresent,lighting.sun_present);
  gl.uniform1i(uniforms.useVertexColor,true);diagnostics.vegetationLighting=lighting;
  gl.uniformMatrix4fv(uniforms.model,false,identity);gl.uniform2f(uniforms.ecoRotation,forwardZ,-forwardX);
  let draws=0;
  //0012af80 submits distant, depth-disabled transition, then near material queues.
  gl.uniform1i(uniforms.ecoBillboard,true);
  for(const transition of [false,true]) {
    const billboard=eco.billboard;if(!billboard)continue;
    const chunk=billboard.chunk,count=transition?selection.transition:selection.far,first=transition?selection.far:0;
    if(!count)continue;
    gl.bindBuffer(gl.ARRAY_BUFFER,billboard.instances);gl.bufferSubData(gl.ARRAY_BUFFER,0,instances.subarray(first*layout.billboard_floats,(first+count)*layout.billboard_floats));
    gl.uniform1f(uniforms.alphaCutoff,transition?1/255:chunk.alphaCutoff);
    ecoMaterial(chunk,transition?false:chunk.depthWrite);
    gl.drawElementsInstanced(gl.TRIANGLES,chunk.count,gl.UNSIGNED_SHORT,0,count);draws++;
    diagnostics.vegetationTriangles+=chunk.count/3*count;
  }
  diagnostics.vegetationBillboardDraws=draws;
  gl.uniform1i(uniforms.ecoBillboard,false);
  let offset=layout.mesh_offset;
  for(const [index,group] of ecoGroups.entries()) {
    const count=selection.groups[index];
    if(!count)continue;
    gl.bindBuffer(gl.ARRAY_BUFFER,group.instances);gl.bufferSubData(gl.ARRAY_BUFFER,0,instances.subarray(offset,offset+count*layout.mesh_floats));offset+=count*layout.mesh_floats;
    for(const chunk of group.chunks) {
      gl.uniform1f(uniforms.alphaCutoff,chunk.alphaCutoff);ecoMaterial(chunk,chunk.depthWrite);
      gl.drawElementsInstanced(gl.TRIANGLES,chunk.count,gl.UNSIGNED_SHORT,0,count);draws++;
      diagnostics.vegetationTriangles+=chunk.count/3*count;
    }
  }
  Object.assign(diagnostics,{vegetationVisible:selection.visible,vegetationDraws:draws,vegetationNear:selection.near,vegetationFar:selection.far,vegetationTransition:selection.transition,vegetationScanned:selection.scanned});
  gl.uniform1i(uniforms.useEcosystem,false);gl.uniform1i(uniforms.ecoPerInstance,false);gl.uniform1f(uniforms.alphaCutoff,0);
  gl.uniform1i(uniforms.useVertexColor,false);gl.uniform1i(uniforms.sceneryBlend,false);gl.disable(gl.BLEND);gl.depthMask(true);
}
function ecoMaterial(chunk,depthWrite) {
  gl.uniform1i(uniforms.useTexture,Boolean(chunk.texture));gl.uniform1i(uniforms.fogEnabled,Boolean(terrain.world.metadata.fog)&&chunk.fog);
  gl.uniform1i(uniforms.sceneryBlend,chunk.blend);gl.depthMask(depthWrite);
  if(chunk.blend){gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);}else gl.disable(gl.BLEND);
  gl.bindTexture(gl.TEXTURE_2D,chunk.texture??fallbackTexture);gl.bindVertexArray(chunk.vao);
}
function buildTerrainMesh() {
  const reusable = chunks; chunks = [];
  const { width, depth, spacing, heights } = terrain;
  // Retain original samples near the rider and a bounded coarse far field.
  // This browser LOD is provisional; it does not alter terrain contacts.
  const step = Math.max(1, Math.ceil(Math.sqrt((width - 1) * (depth - 1) / 100000)));
  const xs = [], zs = [];
  for (let x = 0; x < width - 1; x += step) xs.push(x);
  for (let z = 0; z < depth - 1; z += step) zs.push(z);
  xs.push(width - 1); zs.push(depth - 1);
  const height = (x, z) => heights[z * width + x];
  const centerX = vehicle ? Math.max(0, Math.min(width - 1, vehicle.position[0] / spacing)) : 0;
  const centerZ = vehicle ? Math.max(0, Math.min(depth - 1, vehicle.position[2] / spacing)) : 0;
  const patch = vehicle ? {
    x0: Math.max(0, Math.floor((centerX - 96) / step) * step),
    x1: Math.min(width - 1, Math.ceil((centerX + 96) / step) * step),
    z0: Math.max(0, Math.floor((centerZ - 96) / step) * step),
    z1: Math.min(depth - 1, Math.ceil((centerZ + 96) / step) * step),
  } : null;
  let vertexCount = 0, triangleCount = 0, reused = 0, uploadedBytes = 0;
  const started = performance.now();
  function grid(gridX, gridZ, fine = false) {
    const nx = gridX.length, nz = gridZ.length;
    const old = reusable[chunks.length], capacity = nx * nz * 6;
    const sameGrid = old?.fine === fine && old.gridX === gridX[0] && old.gridZ === gridZ[0] && old.nx === nx && old.nz === nz;
    const maskX0 = patch ? Math.max(gridX[0], patch.x0) : 0, maskX1 = patch ? Math.min(gridX.at(-1), patch.x1) : 0;
    const maskZ0 = patch ? Math.max(gridZ[0], patch.z0) : 0, maskZ1 = patch ? Math.min(gridZ.at(-1), patch.z1) : 0;
    const mask = maskX1 > maskX0 && maskZ1 > maskZ0 ? `${maskX0}:${maskX1}:${maskZ0}:${maskZ1}` : 'all';
    if (!fine && sameGrid && old.mask === mask) {
      chunks.push(old); reused++; vertexCount += capacity / 6; triangleCount += old.count / 3; return;
    }
    const vertices = !fine && sameGrid ? null : old?.cpuVertices?.length === capacity ? old.cpuVertices : new Float32Array(capacity);
    const indices = fine && old?.fine && old.nx === nx && old.nz === nz ? null : [];
    let offset = 0;
    if (vertices) for (let zi = 0; zi < nz; zi++) for (let xi = 0; xi < nx; xi++) {
      const x = gridX[xi], z = gridZ[zi], left = Math.max(0, x - 1), right = Math.min(width - 1, x + 1), back = Math.max(0, z - 1), front = Math.min(depth - 1, z + 1);
      const dx = (height(right, z) - height(left, z)) / ((right - left) * spacing);
      const dz = (height(x, front) - height(x, back)) / ((front - back) * spacing);
      const length = Math.hypot(dx, 1, dz);
      let y = height(x, z);
      // Stitch the fine patch boundary to the surrounding coarse grid.
      if (fine && (x === patch.x0 || x === patch.x1)) {
        const z0 = Math.floor(z / step) * step, z1 = Math.min(depth - 1, z0 + step);
        y = z0 === z1 ? height(x, z0) : height(x, z0) + (height(x, z1) - height(x, z0)) * (z - z0) / (z1 - z0);
      } else if (fine && (z === patch.z0 || z === patch.z1)) {
        const x0 = Math.floor(x / step) * step, x1 = Math.min(width - 1, x0 + step);
        y = x0 === x1 ? height(x0, z) : height(x0, z) + (height(x1, z) - height(x0, z)) * (x - x0) / (x1 - x0);
      }
      vertices.set([x * spacing, y, z * spacing, -dx / length, 1 / length, -dz / length], offset); offset += 6;
    }
    if (indices) for (let z = 0; z < nz - 1; z++) for (let x = 0; x < nx - 1; x++) {
      if (!fine && patch && gridX[x] >= patch.x0 && gridX[x + 1] <= patch.x1 && gridZ[z] >= patch.z0 && gridZ[z + 1] <= patch.z1) continue;
      const a = z * nx + x, b = a + 1, c = a + nx, d = c + 1;
      indices.push(a, c, b, b, c, d);
    }
    const packed = indices ? new Uint16Array(indices) : old.cpuIndices;
    let chunk;
    if (old && old.fine === fine && old.vertexCapacity === capacity && old.indexCapacity >= packed.length) {
      chunk = old; reused++;
      gl.bindVertexArray(chunk.vao);
      if (vertices) { gl.bindBuffer(gl.ARRAY_BUFFER, chunk.vertices); gl.bufferSubData(gl.ARRAY_BUFFER, 0, vertices); uploadedBytes += vertices.byteLength; chunk.bounds = vertexBounds(vertices); }
      // Only coarse tiles overlapping a changed fine patch need a new mask.
      let changed = packed.length !== old.count;
      if (!changed && packed !== old.cpuIndices) for (let i = 0; i < packed.length; i++) if (packed[i] !== old.cpuIndices[i]) { changed = true; break; }
      if (changed) { gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, chunk.indices); gl.bufferSubData(gl.ELEMENT_ARRAY_BUFFER, 0, packed); uploadedBytes += packed.byteLength; }
      chunk.count = packed.length;
    } else {
      if (old) { gl.deleteVertexArray(old.vao); gl.deleteBuffer(old.vertices); gl.deleteBuffer(old.indices); }
      // Reserve full coarse topology; masks can grow again when the patch leaves.
      const reserved = new Uint16Array((nx - 1) * (nz - 1) * 6); reserved.set(packed);
      chunk = upload(vertices, reserved); chunk.count = packed.length;
      chunk.vertexCapacity = capacity; chunk.indexCapacity = reserved.length;
      uploadedBytes += vertices.byteLength + reserved.byteLength;
    }
    Object.assign(chunk, { fine, mask, gridX: gridX[0], gridZ: gridZ[0], nx, nz, cpuVertices: fine ? vertices : null, cpuIndices: packed });
    chunks.push(chunk); vertexCount += capacity / 6; triangleCount += packed.length / 3;
  }
  for (let z0 = 0; z0 < zs.length - 1; z0 += 64) for (let x0 = 0; x0 < xs.length - 1; x0 += 64) grid(xs.slice(x0, x0 + 65), zs.slice(z0, z0 + 65));
  if (patch) {
    const fineX = Array.from({ length: patch.x1 - patch.x0 + 1 }, (_, i) => patch.x0 + i), fineZ = Array.from({ length: patch.z1 - patch.z0 + 1 }, (_, i) => patch.z0 + i);
    for (let z0 = 0; z0 < fineZ.length - 1; z0 += 64) for (let x0 = 0; x0 < fineX.length - 1; x0 += 64) grid(fineX.slice(x0,x0+65),fineZ.slice(z0,z0+65),true);
  }
  for (const chunk of reusable.slice(chunks.length)) { gl.deleteVertexArray(chunk.vao); gl.deleteBuffer(chunk.vertices); gl.deleteBuffer(chunk.indices); }
  gl.bindVertexArray(null);
  diagnostics.terrainMesh = { reused, uploadedBytes, updateCpuMs: performance.now() - started };
  Object.assign(diagnostics, { vertices: vertexCount, triangles: triangleCount, chunks: chunks.length, finePatch: patch, terrainGrid: [width,depth], terrainSampleSpacing: spacing, terrainFarStep: step, terrainLod: 'full stored samples nearby; provisional far grid', terrainDetailCenter: [centerX,centerZ], terrainDetailUpdates: (diagnostics.terrainDetailUpdates ?? 0)+1 });
}
function buildMesh() {
  clearMesh(); buildTerrainMesh(); buildEcosystem();
  if(crashSettings.mode==='gorn')bloodRenderer=new BloodRenderer(gl);
  if(vehicle) { vehicle.boundsByNode=meshNodeBounds(vehicle.groups,vehicle.nodes.length); vehicle.cutKey=null; }
  if(terrain.snowSource?.config){snowRenderer=new SnowRenderer(gl,terrain.snowSource);}
  if (terrain.wheelEffectsSource) { wheelEffectsRenderer = new WheelEffectsRenderer(gl, terrain.wheelEffectsSource); wheelEffectsRenderer.accept(simFrame?.wheel_particles); }
  if (terrain.ambientEffectsSource?.config.active) ambientEffectsRenderer=new AmbientEffectsRenderer(gl,terrain.ambientEffectsSource);
  diagnostics.ambientEffects=terrain.ambientEffectsSource?{...terrain.ambientEffectsSource.config,frame:terrain.ambientEffectsSource.frame,draws:0,visible:0,gpuBytes:ambientEffectsRenderer?.gpuBytes??0}:null;
  if (terrain.trainSource) trainRenderer = new TrainRenderer(gl, terrain.trainSource, { upload, texture, inverseDirection });
  diagnostics.wheelEffects = wheelEffectsRenderer ? { atlas: terrain.wheelEffectsSource.config.atlas.texture, frames: terrain.wheelEffectsSource.config.atlas.frames.length, particles: 0, draws: 0, gpuBytes: wheelEffectsRenderer.gpuBytes } : null;
  diagnostics.train = trainRenderer ? { time: terrain.trainSource.frame.time, duration: terrain.trainSource.frame.duration, cars: terrain.trainSource.frame.cars.filter(car => car.use_collision).length, wheelNodes: terrain.trainSource.frame.wheel_matrices?.length ?? 0, frame: terrain.trainSource.frame } : null;
  if (terrain.waterSource) waterRenderer = new WaterRenderer(gl, terrain.waterSource);
  const materialTextures = new Map();
  if (vehicle) for (const [name, decoded] of vehicle.textures) { if (!decoded.rgba) continue; const gpu = texture(decoded); materialTextures.set(name, gpu); gpuTextures.push(gpu); }
  if (vehicle) for (const group of vehicle.groups) {
    if (group.vertices.length > 65536) throw new Error('ATV mesh group exceeds index bounds.');
    const vertices = new Float32Array(group.vertices.length * 9);
    group.vertices.forEach((vertex, index) => {
      const node = vehicle.nodes[vertex.node];
      if (!node) throw new Error('Quad vertex node is missing.');
      vertices.set([...vertex.local_position, ...inverseDirection(node.world_matrix, vertex.normal), ...vertex.uv, vertex.node], index * 9);
    });
    const chunk = upload(vertices, new Uint16Array(group.indices), true, true);
    chunk.quad = true; chunk.material = vehicle.materials[group.material];
    const mud=vehicle.textures.get(vehicle.materials[group.material])?.mud;
    if (mud) {chunk.mudTexture=texture(mud);gpuTextures.push(chunk.mudTexture);}
    chunk.texture = materialTextures.get(vehicle.materials[group.material]);
    chunk.fog = vehicle.textures.get(vehicle.materials[group.material])?.pass?.fog ?? true;
    vehicleChunks.push(chunk);
  }
  let riderTriangles = 0, riderTextureCount = 0;
  if (vehicle?.rider) {
    const rider = vehicle.rider, riderTextures = new Map();
    rider.boundsByNode=meshNodeBounds(rider.mesh.lods[0].groups,rider.mesh.nodes.length);
    rider.gpuPose=new Float32Array(rider.pose.world_matrices.flat());
    for (const [name, decoded] of rider.textures) { if (!decoded.rgba) continue; const gpu = texture(decoded); riderTextures.set(name, gpu); gpuTextures.push(gpu); }
    riderTextureCount = riderTextures.size;
    for (const group of rider.mesh.lods[0].groups) {
      if (group.vertices.length > 65536) throw new Error('Rider mesh group exceeds index bounds.');
      const vertices = new Float32Array(group.vertices.length * 9);
      group.vertices.forEach((vertex, index) => {
        const bone = rider.mesh.nodes[vertex.node], pose = rider.pose.world_matrices[vertex.node];
        if (!bone || !pose) throw new Error('Rider animation node is missing.');
        vertices.set([...vertex.local_position, ...inverseDirection(bone.world_matrix, vertex.normal), ...vertex.uv, vertex.node], index * 9);
      });
      const chunk = upload(vertices, new Uint16Array(group.indices), true, true);
      chunk.rider = true; chunk.group=group;
      const mud=rider.textures.get(rider.mesh.materials[group.material])?.mud;
      if (mud) {chunk.mudTexture=texture(mud);gpuTextures.push(chunk.mudTexture);}
      chunk.texture = riderTextures.get(rider.mesh.materials[group.material]);
      chunk.fog = rider.textures.get(rider.mesh.materials[group.material])?.pass?.fog ?? true;
      vehicleChunks.push(chunk); riderTriangles += group.indices.length / 3;
    }
  }
  let sceneTriangles = 0, sceneInstances = 0, skippedSceneGroups = 0;
  if (terrain.sky) for (const tile of terrain.sky.tiles) {
    // 00125960 and 00125570 emit local UVs spanning .03125..96875.
    const uv = [[.03125, .03125], [.96875, .03125], [.03125, .96875], [.96875, .96875]];
    const vertices = new Float32Array(32);
    tile.positions.forEach((position, index) => vertices.set([...position, 0, 1, 0, ...uv[index]], index * 8));
    const chunk = upload(vertices, new Uint16Array([0, 2, 1, 1, 2, 3]), true);
    chunk.texture = texture({ width: tile.size, height: tile.size, rgba: tile.rgba });
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    skyChunks.push(chunk); gpuTextures.push(chunk.texture);
  }
  if (terrain.world) {
    const world = terrain.world, textures = new Map(), geometry = new Map();
    for (const [name, decoded] of world.textures) { if (!decoded.rgba) continue; const gpu = anisotropic(texture(decoded)); textures.set(name, gpu); gpuTextures.push(gpu); }
    for (const [name, mesh] of world.meshes) {
      const groups = [];
      for (const group of mesh.lods[0].groups) {
        const baseMaterial = mesh.materials[group.material];
        for(const [passIndex,material] of (world.textures.get(baseMaterial)?.passKeys??[baseMaterial]).entries()) {
        const decoded = world.textures.get(material);
        const reflection = decoded?.pass?.reflection_plane ?? 0;
        if (decoded?.reflective && !reflection) { skippedSceneGroups++; continue; }
        const prelit = decoded?.pass?.prelit ?? false;
        if (reflection && mesh.nodes.length > 32) throw new Error('Reflection mesh exceeds original rigid-node GPU capacity.');
        const width = reflection ? 19 : prelit ? 12 : 8;
        const vertices = new Float32Array(group.vertices.length * width);
        group.vertices.forEach((vertex, index) => vertices.set([...vertex.position, ...vertex.normal, ...vertex.uv, ...(prelit ? vertex.color ?? [128,128,128,128] : []), ...(reflection ? [...vertex.local_position, ...vertex.local_normal, vertex.node] : [])], index * width));
        const chunk = upload(vertices, new Uint16Array(group.indices), true, false, prelit, Boolean(reflection));
        chunk.reflection = reflection;
        chunk.reflectionNodes = reflection ? new Float32Array(mesh.nodes.flatMap(node => node.world_matrix)) : null;
        chunk.texture = textures.get(material); chunk.alphaCutoff = decoded?.sceneAlphaCutoff ?? 0;
        chunk.animationFrames = decoded?.animationFrames?.map(rgba => { const gpu = anisotropic(texture({...decoded,rgba})); gpuTextures.push(gpu); return gpu; });
        chunk.prelit = prelit; chunk.material = material; chunk.passIndex=passIndex; chunk.additive=decoded?.pass?.additive??false; chunk.blend = decoded?.pass?.blend ?? false; chunk.alphaFailure = decoded?.sceneAlphaFailure ?? 0;
        chunk.depthWrite = decoded?.pass?.depth_write ?? true;
        chunk.fog = decoded?.pass?.fog ?? true;
        groups.push(chunk); sceneChunks.push(chunk);
        }
      }
      geometry.set(name, groups);
    }
    // Stadium cameraflash.slt models are hidden and lit in random batches
    // (0020f2c0); each keeps its pool index in scene order.
    let flashes = 0;
    for (const model of world.metadata.models) {
      const flash = world.metadata.crowd_present && model.name.toLowerCase() === 'cameraflash' ? flashes++ : -1;
      const groups = geometry.get(model.name);
      if (!groups?.length) continue;
      sceneInstances++;
      for (const chunk of groups) { const matrix = new Float32Array(model.world_matrix); sceneDraws.push({ name: model.name, chunk, matrix, flash, gate: Boolean(model.drop_gate), bounds: worldBounds(chunk.bounds, matrix), unlit: chunk.prelit || !model.use_lighting }); sceneTriangles += chunk.count / 3; }
    }
  }
  gl.bindVertexArray(null);
  diagnostics.skyTiles = skyChunks.length;
  diagnostics.sceneryPrelitGroups = sceneChunks.filter(chunk => chunk.prelit).length;
  diagnostics.sceneryReflectionGroups = sceneChunks.filter(chunk => chunk.reflection).length;
  diagnostics.sceneryUnsupportedAnimations = [...(terrain.world?.textures ?? [])].filter(([, texture]) => texture.pass?.unsupported_animation).map(([name, texture]) => ({ name, reason: texture.pass.unsupported_animation }));
  diagnostics.unsupportedSceneTextures = [...(terrain.world?.textures ?? [])].filter(([, texture]) => texture.error).map(([name, texture]) => ({ name, error: texture.error }));
  Object.assign(diagnostics, { vehicleVertices: vehicle?.groups.reduce((sum, group) => sum + group.vertices.length, 0) ?? 0, vehicleTriangles: vehicle?.groups.reduce((sum, group) => sum + group.indices.length / 3, 0) ?? 0, vehicleTextures: materialTextures.size, riderTriangles, riderTextures: riderTextureCount, vehiclePosition: vehicle?.position, sceneInstances, sceneTriangles, skippedSceneGroups, unsupportedSceneMeshes: terrain.world?.unsupported ?? [] });
}
function resetCamera() {
  if (cameraFrame && simReady) { acceptCameraFrame(cameraFrame); canvas.focus({ preventScroll: true }); return; }
  if (!terrain) return;
  const span = Math.max(terrain.sizeX, terrain.sizeZ);
  camera.x = terrain.sizeX * .5; camera.z = terrain.sizeZ * .88; camera.y = terrain.max + span * .13;
  camera.yaw = Math.PI; camera.pitch = -.35;
  if (vehicle) {
    const distance = vehicle.length * 2.2;
    camera.x = vehicle.position[0] - Math.sin(vehicle.yaw) * distance;
    camera.z = vehicle.position[2] - Math.cos(vehicle.yaw) * distance;
    camera.y = Math.max(vehicle.position[1] + vehicle.height * 1.9, terrainHeight(camera.x, camera.z) + vehicle.height);
    camera.yaw = vehicle.yaw;
    camera.pitch = Math.atan2(vehicle.position[1] + vehicle.height * .55 - camera.y, distance);
  }
  canvas.focus({ preventScroll: true });
}
function vehicleMatrix(actor = presentedFrame ?? simFrame) {
  const [w, x, y, z] = actor?.orientation ?? [Math.cos(vehicle.yaw / 2), 0, Math.sin(vehicle.yaw / 2), 0];
  const position = actor?.origin ?? vehicle.position;
  return new Float32Array([
    1 - 2 * (y * y + z * z), 2 * (x * y + w * z), 2 * (x * z - w * y), 0,
    2 * (x * y - w * z), 1 - 2 * (x * x + z * z), 2 * (y * z + w * x), 0,
    2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y), 0,
    ...position, 1,
  ]);
}
function matrix() {
  if (cameraFrame) presentCameraFrame();
  // Original00325e80 uses horizontal FOV. Preserve the4:3 vertical framing
  // while extending the view horizontally on widescreen displays.
  // 0012b360 applies the active fog end to the view's far clipping plane.
  // Cull whole objects there, but extend the depth range by the largest
  // object so an admitted stadium stand is not sliced open into the sky.
  const near = 1, far = terrain.world?.metadata.fog?.end ?? Math.max(terrain.sizeX, terrain.sizeZ) * 5;
  const f = (cameraFrame ? diagnostics.cameraPresentation.focalLength : 1 / Math.tan(Math.PI / 6)) * speedRush.focalScale;
  const aspect = canvas.width / canvas.height, depthFar = far + boundsReach(sceneDraws, boundsReach(chunks)) + 1;
  diagnostics.depthFar = depthFar;
  return { viewProjection: sceneViewProjection(camera, aspect, f, near, depthFar), cull: sceneViewProjection(camera, aspect, f, near, far) };
}
function deadzone(value) { return Number.isFinite(value) && Math.abs(value) > .15 ? Math.sign(value) * (Math.abs(value) - .15) / .85 : 0; }
function gamepad() {
  let pad;
  if (gamepadsAvailable) {
    try { pad = navigator.getGamepads?.()?.find(pad => pad?.connected); }
    catch { gamepadsAvailable = false; }
  }
  return pad;
}
function drivingInput() {
  const empty = { throttle: 0, brake: 0, steer: 0, lean: 0, reverse: false };
  if (!focused || document.hidden || menuOpen) return { input: empty, actions: {} };
  const clamp = value => Math.max(-1, Math.min(1, value));
  const { input, actions } = gameplayBindings.read(keys, gamepad());
  input.steer += touchLean.steer;
  input.lean += touchLean.lean;
  const touch = touchControls.sample();
  input.touch_drive = touch.drive;
  input.steer += touch.steer; input.lean += touch.lean;
  // Cancellation also resets the native lean-flick history. A resumed forward
  // lean must not release a preload that was abandoned by blur/menu/disconnect.
  input.input_generation = gameplayBindings.generation + touchControls.generation;
  if (touch.stunt_modifiers) Object.assign(input, { stunt_modifiers: touch.stunt_modifiers, stunt_x: touch.stunt_x, stunt_y: touch.stunt_y });
  if (touch.preload_axis !== null) input.preload_axis = touch.preload_axis;
  // Two-finger mode: the left pad's full pull when the drive thumb lifts.
  const padFlip = !touch.flip_request && touch.preload_axis === -1 && crashSettings.flipsEnabled && !touchControls.oneHanded ? touchLean.launchFlip() : null;
  if (padFlip) Object.assign(input, { flip_request: padFlip.flip, flip_launch: true, flip_hold: padFlip.hold });
  else if (touch.flip_request) Object.assign(input, { flip_request: touch.flip_request, flip_launch: touch.flip_launch, flip_hold: touch.flip_hold });
  input.throttle = Math.max(0, clamp(input.throttle)); input.brake = Math.max(0, clamp(input.brake));
  // Source +X is screen-right; preserve the native steering sign.
  input.steer = clamp(input.steer); // Rust clamps the combined physical lean; one-handed touch also supplies lean.
  return { input, actions };
}
function acceptFrame(frame, effects = true) {
  if (effects) {
    const severity = impactEvents.accept(frame, focused && !document.hidden && !menuOpen && !lost && !pageSuspended);
    // Authored boom belongs to the optional Crazy/Gorn crash presentation.
    if (severity >= 40 && funnyCrashes) audio.crashBoom(severity);
    rumble.accept(simFrame, frame, severity, vibrationSettings.enabled && focused && !document.hidden && !menuOpen && !lost && !pageSuspended);
    // Presentation-only dirt level; one step per delivered 60 Hz frame.
    if (frame.vehicle_kind !== 'motorcycle') dirtBuildup.step(frame);
    if (graphics.tireTracks && !lost && renderCore && frame.vehicle_kind !== 'motorcycle') (tireTracks ??= new TireTrackRenderer(gl, graphics.tireTracks)).step(frame, dirtBuildup.zones, terrainHeight);
    if (graphics.dust && !lost && frame.vehicle_kind !== 'motorcycle') (lingeringDust ??= new LingeringDustRenderer(gl, graphics.dust)).step(frame, dirtBuildup.zones, dustTint);
  }
  wheelEffectsRenderer?.accept(frame.wheel_particles);
  if (diagnostics.wheelEffects) { diagnostics.wheelEffects.frame = frame.wheel_particles; diagnostics.wheelEffects.particles = frame.wheel_particles?.particles.length ?? 0; }
  if (!frame?.origin?.every(Number.isFinite) || frame.origin.length !== 3 || !frame.orientation?.every(Number.isFinite) || frame.orientation.length !== 4 || !Number.isFinite(frame.speed) || !Number.isFinite(frame.rpm)) throw new Error('Simulation returned a nonfinite pose.');
  simFrame = frame;
  // Resting body height above the visible ground, for contact-shadow lift.
  if (graphics.contactShadows && vehicle && renderCore && frame.wreck?.phase === 'Riding' && frame.grounded?.every(Boolean)) {
    const rest = frame.origin[1] - terrainHeight(frame.origin[0], frame.origin[2]);
    if (Number.isFinite(rest)) vehicle.restLift = vehicle.restLift === null || vehicle.restLift === undefined ? rest : vehicle.restLift + (rest - vehicle.restLift) * .1;
  }
  // A completed old batch may arrive after pause/blur/cancel. Keep its physical
  // frame, but do not overwrite cleared controls with the old touch generation.
  if (frame.applied_input && frame.applied_input.input_generation === gameplayBindings.generation + touchControls.generation) diagnostics.input = frame.applied_input;
  touchControls.feedback(frame);
  vehicle.position = frame.origin.slice();
  if (frame.quad_node_matrices?.length !== vehicle.nodes.length || frame.quad_node_matrices.some(matrix => matrix.length !== 16 || !matrix.every(Number.isFinite))) throw new Error('Simulation returned an invalid quad linkage pose.');
  if (effects) wheelLift = 0;
  if (graphics.wheelReprojection && quadLinkage && effects) reprojectWheels(frame);
  const quadPose=drawnQuadPose(frame);
  vehicle.nodeMatrices ??= new Float32Array(quadPose.length*16);
  quadPose.forEach((m,i)=>vehicle.nodeMatrices.set(m,i*16));
  diagnostics.animatedQuadNodes = frame.quad_node_matrices.length;
  if (vehicle.rider) {
    if (frame.rider_node_matrices?.length !== vehicle.rider.mesh.nodes.length || frame.rider_node_matrices.some(matrix => matrix.length !== 16 || !matrix.every(Number.isFinite))) throw new Error('Simulation returned an invalid rider pose.');
    vehicle.rider.pose.world_matrices = funnyCrashes && frame.funny_crash?.rider_node_matrices.length ? frame.funny_crash.rider_node_matrices : frame.rider_node_matrices;
    vehicle.rider.gpuPose ??= new Float32Array(frame.rider_node_matrices.length*16);
    vehicle.rider.pose.world_matrices.forEach((m,i)=>vehicle.rider.gpuPose.set(m,i*16));
    const severed=funnyCrashes ? frame.funny_crash?.severed_nodes??[] : [];
    const key=severed.join(',');
    if(vehicle.cutKey!==key) {
      vehicle.cutKey=key;
      let removed=0;
      for(const chunk of vehicleChunks)if(chunk.rider) {
        chunk.cutCount=null;
        if(severed.length) {
          const indices=severedIndices(chunk.group,vehicle.rider.mesh.nodes,severed);
          chunk.cutBuffer ??= gl.createBuffer();
          gl.bindVertexArray(chunk.vao);gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,chunk.cutBuffer);
          gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,indices,gl.STATIC_DRAW);
          gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,chunk.indices);
          chunk.cutCount=indices.length;removed+=(chunk.count-indices.length)/3;
        }
      }
      gl.bindVertexArray(null);diagnostics.severedBridgeTriangles=removed;
    }
    if(funnyCrashes && frame.funny_crash) {
      // Bounds belong to this physics step, never the previous render sample.
      const actorWorld=vehicleMatrix(frame);
      vehicle.crashSubjects=crashSubjects(vehicle,frame,quadPose,actorWorld);
    } else vehicle.crashSubjects=null;
    diagnostics.animatedRiderNodes = frame.rider_node_matrices.length;
    diagnostics.riderMode = frame.rider_mode;
    if (frame.rider_world_matrix && (frame.rider_world_matrix.length !== 16 || !frame.rider_world_matrix.every(Number.isFinite))) throw new Error('Simulation returned an invalid detached rider transform.');
    diagnostics.riderWreck = frame.rider_wreck ?? null;
  }
}
// Enhanced/High: redraw sunk wheels on the visible ground after integration,
// clamped to suspension travel. Only verified-identical linkages are replaced.
function reprojectWheels(frame) {
  const compression = frame.vehicle_kind === 'motorcycle' ? null : wheelReprojection(frame, terrainHeight);
  if (!compression) return;
  const base = renderCore.quadPose(frame.visual_steering_angle, frame.wheel_angles, frame.wheel_compression);
  if (!base || base.length !== frame.quad_node_matrices.length * 16 || frame.quad_node_matrices.some((m, n) => m.some((v, i) => Math.abs(v - base[n * 16 + i]) > 1e-3))) return;
  const lifted = renderCore.quadPose(frame.visual_steering_angle, frame.wheel_angles, compression);
  if (!lifted) return;
  visualQuad.set(frame, Array.from({ length: frame.quad_node_matrices.length }, (_, n) => Array.from(lifted.subarray(n * 16, n * 16 + 16))));
  wheelLift = Math.max(...compression.map((c, i) => c - frame.wheel_compression[i]));
}
async function startSimulation(source) {
  impactEvents.reset();
  motorcyclePresentation.reset(); atvPresentation.reset(); presentedFrame = presentedCamera = undefined;
  dirtBuildup = new DirtBuildup(parseDirtZones(source.toy ? decoder.decode(source.toy) : '')); tireTracks?.reset(); lingeringDust?.reset();
  diagnostics.simulationSteps = 0;
  simReady = false; simFrame = undefined; cameraFrame = undefined; simAccumulator = 0; simGeneration++;
  if (!source.physics) return;
  await codec(20, source.bytes); if (!source.motorcycle) { await codec(21, source.physics); await codec(22, source.meshBytes); } await codec(26, source.toy);
  await codec(65, source.ecosystem?.bytes ?? new Uint8Array());
  diagnostics.water = source.water ? await json(64, source.water.resource, source.water.index) : null;
  terrain.waterSource = source.water ? await json(66, source.water.resource, source.water.index) : null;
  waterRenderer?.dispose();
  waterRenderer = terrain.waterSource ? new WaterRenderer(gl, terrain.waterSource) : undefined;
  diagnostics.waterRender = waterRenderer ? { channels: waterRenderer.channelCount, gpuBytes: waterRenderer.gpuBytes, draws: 0, baseHeight: terrain.waterSource.frame.base_height, scope: terrain.waterSource.scope } : null;
  diagnostics.chassis = source.motorcycle ? null : await json(44, source.chassis);
  const actor = source.motorcycle ? {rays:0} : await json(48, source.actorRays, 0), collisionIds = new Map();
  for (const [name, bytes] of source.collisions.meshes) {
    try { collisionIds.set(name, (await json(48, bytes, 1)).mesh); } catch (error) { throw new Error(`${name}: ${error.message}`); }
  }
  const obstacles = await command(49, {
    placements: source.collisions.placements.map(placement => ({ mesh: collisionIds.get(placement.name), name: placement.name, world_matrix: placement.world_matrix })),
    base_name: source.environment,
    boundary_launch: source.world.metadata.boundary_launch, crowd_present: source.world.metadata.crowd_present,
    scene_partition: source.world.metadata.collision_partition,
  });
  diagnostics.obstacles = { ...obstacles, rays: actor.rays };
  // Rust places race grids on the terrain and spaces online slots.
  simStart = await command(140, { position: source.scene.position, yaw: source.scene.yaw, course: Boolean(source.course), slot: multiplayer?.state.peer ?? null });
  if (source.motorcycle) await json(100,source.motorcycle.archive,source.motorcycle.id);
  acceptFrame(await command(23, simStart));
  cpuQuadMatrices = vehicle.nodeMatrices.slice(); cpuRiderMatrices = new Float32Array((source.cpuRiderPose ?? vehicle.rider.pose).world_matrices.flat());
  for (const [index, entry] of (source.course ? source.cpuRoster ?? [] : []).entries()) await json(106, entry.bundle, index);
  // Stadium crowd: reactions (races), flashes and Soultree ambient loops.
  litFlashes = new Set();
  const flashPool = source.world.metadata.crowd_present ? source.world.metadata.models.filter(model => model.name.toLowerCase() === 'cameraflash').length : 0;
  let stadium = { soultree: [], crowd: [] };
  // Tracks.dat settings+8: Pro Supercross courses play the Pro crowd (002cb9b0).
  const crowdPrefix = frontend?.proSupercross?.has(source.environment) ? 'Pro' : 'Am';
  try { stadium = await command(115, { crowd: source.course ? source.crowd?.aud ?? null : null, crowd_prefix: crowdPrefix, toy: source.toy ? decoder.decode(source.toy) : null, flash_pool: flashPool, soultree: source.crowd?.soultree ?? null }); }
  catch (error) { diagnostics.crowdError = error.message; await command(115, {}).catch(() => {}); }
  diagnostics.crowd = { flashPool, prefix: crowdPrefix, soultree: stadium.soultree.length, reactions: stadium.crowd.length };
  void audio.loadStadium(stadium).catch(error => { diagnostics.audioError = error.message; });
  void audio.loadVenue(source.venueAud ?? null, { stadium: Boolean(source.world.metadata.crowd_present) }).catch(error => { diagnostics.audioError = error.message; });
  const intro = !raceRestarting; raceRestarting = false;
  if (source.course) await command(96,{active:true,intro,laps:raceSettings.laps,difficulty:raceSettings.difficulty,aggressive:Boolean(raceSettings.aggressive),all_out:Boolean(raceSettings.all_out),names:source.cpuNames.slice(0,raceSettings.cpu),spacing_sources:source.spacingSources});
  raceCourse = source.course;
  raceOverlay.setCourse(raceCourse);
  disposeCpuOutfits(); raceCpuModels = [];
  if (source.course && source.cpuRoster) raceCpuModels = await Promise.all(source.cpuRoster.map(async entry => ({ entry, model: await remoteAssets(entry.quad, frontend?.riderId ?? 0), outfit: null, gpu: null })));
  for (const cpu of raceCpuModels) cpu.outfit = await outfitTextures(cpu.entry.outfit, cpu.model.source?.riderMesh);
  diagnostics.cpuRoster = raceCpuModels.map(({ entry, outfit, model }) => ({ name: entry.name, ai: entry.ai, number: entry.number, quad: entry.quad, outfitTextures: outfit?.size ?? 0, model: Boolean(model?.source) }));
  diagnostics.cameraPresets = (await json(45, source.cameraPresets)).map(preset => preset.name);
  orbit = 0; acceptCameraFrame(await command(46, { dt: 1 / 60, orbit }));
  if (source.train) {
    source.train.frame = await json(67, source.train.resource, source.train.modeIndex);
    diagnostics.trainCollision = await command(69, {});
    trainRenderer?.accept(source.train.frame);
    diagnostics.train = { time: source.train.frame.time, duration: source.train.frame.duration, cars: source.train.frame.cars.filter(car => car.use_collision).length, wheelNodes: source.train.frame.wheel_matrices?.length ?? 0, frame: source.train.frame };
  } else diagnostics.train = null;
  const snowConfig=await command(121,source.snow?{active:true,definition:source.weather.definition,emitter:source.snow.emitter}:{active:false});
  if(source.snow){source.snow.config=snowConfig;snowRenderer?.dispose();snowRenderer=new SnowRenderer(gl,source.snow);}
  diagnostics.snow=source.snow?{rings:snowConfig.offsets.length,gpuBytes:snowRenderer.gpuBytes,capacity:snowRenderer.capacity,time:0,births:0,live:0,draws:0,scope:'source camera rings/EMT births and mode2 motion; portable bounded GPU slots; global RNG/render queue provenance pending'}:null;
  const uvSources = Object.fromEntries([...source.world.textures].filter(([, texture]) => texture.pass?.uv_scroll || texture.pass?.texture_animation).map(([name, texture]) => [name, {source:texture.materialText,frames:texture.animationFrames?.length}]));
  const uvState = await command(91, uvSources);
  diagnostics.sceneryUv = Object.fromEntries(uvState.map(([name]) => [name, [0,0]]));
  diagnostics.sceneryTextureFrames = {};
  if (source.ambientEffects) {await command(131,source.ambientEffects.source);ambientEffectsRenderer?.accept(await command(132,{}));}
  simReady = true;
}
// An online menu only releases local controls. Physics, contacts, scene clocks
// and peer snapshots keep advancing for the active shared ride.
const liveMultiplayer = () => Boolean(multiplayer?.started && !multiplayer.loading && multiplayer.state.lobby?.started);
const simulationPaused = () => document.hidden || lost || pageSuspended || ((!focused || menuOpen) && !liveMultiplayer());
async function pumpSimulation() {
  if (simBusy || !simReady || simulationPaused()) return;
  const count = Math.min(4, Math.floor(simAccumulator * 60));
  if (count < 1) return;
  simBusy = true;
  const token = simGeneration;
  try {
    const { input, actions } = drivingInput();
    input.funny_crashes = funnyCrashes;
    input.crash_mode = crashSettings.mode;
    input.flips = crashSettings.flipsEnabled;
    // Menu/reset transitions cancel input before any further physics is queued.
    if (actions.pause) { void frontend?.show('pause'); return; }
    if (actions.reset) { await $('reset').onclick(); return; }
    for (let step = 0; step < count; step++) simAccumulator -= 1 / 60;
    const cycle = cameraCycle || actions.camera; cameraCycle = false;
    if (cycle) { orbit = 0; impactEvents.reset(); }
    const submitted = performance.now();
    const pad = focused ? gamepad() : null, padPressed = Boolean(pad?.buttons.some(b => b.pressed));
    if (simFrame?.race?.phase === 'intro' && padPressed && !introPadHeld) introSkip = true;
    introPadHeld = padPressed;
    const skip_intro = introSkip && simFrame?.race?.phase === 'intro'; introSkip = false;
    const frames = await command(50, { count, input, orbit, cycle, skip_intro, now: Math.floor(performance.now()) });
    diagnostics.physicsRoundTripMs = performance.now() - submitted;
    diagnostics.physicsBatchSteps = count;
    if (token !== simGeneration) return;
    const previousFrame = simFrame, previousCamera = cameraFrame;
    for (const { frame, camera: cameraResult, water, train, scenery_uv, scenery_textures, race, ambient, snow, flashes } of frames) {
      if(race){frame.race=race;startLightSound(race);if(race.crowd)audio.crowdCue(race.crowd);if(race.gate)poseGate(race.gate);}
      raceMusicEvent(race);
      if(flashes){litFlashes=new Set(flashes);diagnostics.flashes=flashes.length;}
      acceptFrame(frame);
      if(ambient&&terrain.ambientEffectsSource){terrain.ambientEffectsSource.frame=ambient;ambientEffectsRenderer?.accept(ambient);diagnostics.ambientEffects.frame=ambient;}
      if(snow&&snowRenderer){snowRenderer.accept(snow);Object.assign(diagnostics.snow,{time:snow.time,births:snow.total_emitted,activeEmitters:snow.active_emitters,live:terrain.snowSource.history.reduce((n,birth)=>n+Number(Math.fround(snow.time-birth.time)<=birth.life),0)});}
      diagnostics.sceneryUv = scenery_uv; diagnostics.sceneryTextureFrames = scenery_textures;
      if (trainRenderer && train) { trainRenderer.accept(train); diagnostics.train.time = train.time; diagnostics.train.frame = train; }
      if (waterRenderer && water) { waterRenderer.accept(water); terrain.waterSource.frame = water; diagnostics.waterRender.baseHeight = water.base_height; }
      diagnostics.simulationSteps++;
      acceptCameraFrame(cameraResult);
      void audio.update(1 / 60, frame.applied_input ?? input, frame, { train, camera: cameraResult });
    }
    motorcyclePresentation.accept(previousFrame, simFrame, previousCamera, cameraFrame, performance.now(), focused && !document.hidden && !menuOpen && !cycle);
    if (graphics.interpolation) atvPresentation.accept(previousFrame, simFrame, previousCamera, cameraFrame, performance.now(), frames.length, focused && !document.hidden && !menuOpen && !cycle);
    else atvPresentation.reset();
  } catch (error) {
    if (token === simGeneration) { simReady = false; status(`Simulation stopped: ${error.message}`, true); }
  } finally { simBusy = false; }
}
// 321 LIGHTS sounds (002e79a0): beep01 as each red lamp lights, go with the
// green lamp; a National drops its start gate (00223bf0, gate.vag).
function poseGate(matrix) {
  let draws = 0;
  for (const draw of sceneDraws) if (draw.gate) { draws++; draw.matrix.set(matrix); draw.bounds = worldBounds(draw.chunk.bounds, draw.matrix); }
  diagnostics.dropGate = { draws, origin: [...matrix.slice(12, 15)], up: [...matrix.slice(4, 7)] };
}
// IntOut race music follows the race phase (00217bb0, 00217590, 00216260).
let raceMusicPhase = null;
function raceMusicEvent(race) {
  const phase = race?.phase ?? null;
  if (phase === raceMusicPhase) return;
  const previous = raceMusicPhase; raceMusicPhase = phase;
  if (phase === 'intro') void audio.raceMusic('intro');
  else if (phase === 'countdown' && previous !== 'intro') void audio.raceMusic('restart');
  else if (phase === 'racing') void audio.raceMusic('go');
  else if (phase === 'finished') void audio.raceMusic('over', race.position === 1);
  else if (!phase) void audio.raceMusic('leave');
}
let startLights = null;
function startLightSound(race) {
  const lights = race.lights ?? null;
  if (lights !== startLights && lights != null && lights > 0) {
    audio.oneShot(lights < 4 ? 'beep01.vag' : 'go.vag');
    if (lights === 4 && race.gate) audio.oneShot('gate.vag');
  }
  startLights = lights;
}
function acceptCameraFrame(frame) {
  if (frame?.position?.length !== 3 || frame?.target?.length !== 3 || ![...frame.position, ...frame.target, frame.fov].every(Number.isFinite)) throw new Error('Simulation returned an invalid camera pose.');
  cameraFrame = frame;
  presentCameraFrame(frame);
  diagnostics.followCamera = { ...frame, presetName: diagnostics.cameraPresets?.[frame.preset] };
}
function presentCameraFrame(frame = presentedCamera ?? cameraFrame) {
  const aspect=canvas.width/canvas.height;
  // Pre-race shots and trackside TV cameras are authored per course: no
  // chase adaptation.
  const chase=frame.mode==='intro'||frame.mode==='tv'?cinematicPresentation(frame,aspect):chasePresentation(frame,presentedFrame??simFrame,aspect,uphillSightline,terrainHeight);
  const view=crashPresentation(chase,crashSettings.camera === 'tight' ? vehicle?.crashSubjects : null,aspect,terrainHeight);
  [camera.x, camera.y, camera.z] = view.position;
  camera.yaw = view.yaw;
  camera.pitch = view.pitch;
  diagnostics.cameraPresentation = view.presentation;
}
function followCamera(dt) {
  const pad = focused ? gamepad() : null;
  if (!focused || document.hidden || menuOpen) return;
  // Side-effect-free values here: edge actions/preload are sampled once per
  // submitted physics batch, so render frames cannot swallow a release.
  const amount = id => Math.max(gameplayBindings.value('keyboard', id, keys, pad), gameplayBindings.value('controller', id, keys, pad));
  orbit -= (amount('cameraRight') - amount('cameraLeft')) * dt * 1.7;
}
function updateCamera(dt) {
  if (simReady) { uphillSightline.update(crashSettings.chaseAssist ? presentedFrame ?? simFrame : null, presentedCamera ?? cameraFrame, dt, terrainHeight); followCamera(dt); return; }
  if (!focused || document.hidden) return;
  const pad = gamepad();
  let sideways = Number(keys.has('KeyD') || keys.has('ArrowRight')) - Number(keys.has('KeyA') || keys.has('ArrowLeft')) + deadzone(pad?.axes[0]);
  let forward = Number(keys.has('KeyW') || keys.has('ArrowUp')) - Number(keys.has('KeyS') || keys.has('ArrowDown')) - deadzone(pad?.axes[1]);
  let vertical = Number(keys.has('KeyE')) - Number(keys.has('KeyQ')) + (pad?.buttons[7]?.value ?? 0) - (pad?.buttons[6]?.value ?? 0);
  camera.yaw += deadzone(pad?.axes[2]) * dt * 1.7;
  camera.pitch -= deadzone(pad?.axes[3]) * dt * 1.3;
  for (const pointer of pointers.values()) if (pointer.move) { sideways += pointer.dx / 70; forward -= pointer.dy / 70; }
  camera.pitch = Math.max(-1.45, Math.min(1.45, camera.pitch));
  const aboveGround = Math.max(0, camera.y - terrainHeight(camera.x, camera.z));
  const speed = Math.max(vehicle ? 12 : 30, Math.min(Math.max(terrain.sizeX, terrain.sizeZ) * .09, aboveGround * .6)) * (keys.has('ShiftLeft') ? 3 : 1) * dt;
  const length = Math.max(1, Math.hypot(sideways, forward, vertical));
  sideways /= length; forward /= length; vertical /= length;
  camera.x += (Math.sin(camera.yaw) * forward + Math.cos(camera.yaw) * sideways) * speed;
  camera.z += (Math.cos(camera.yaw) * forward - Math.sin(camera.yaw) * sideways) * speed;
  camera.y += (Math.sin(camera.pitch) * forward + vertical) * speed;
  const span = Math.max(terrain.sizeX, terrain.sizeZ);
  camera.x = Math.max(-span, Math.min(terrain.sizeX + span, camera.x));
  camera.z = Math.max(-span, Math.min(terrain.sizeZ + span, camera.z));
  camera.y = Math.max(terrain.min - span * .2, Math.min(terrain.max + span * 2, camera.y));
}
function scheduleRender() {
  if (!frame && !pageSuspended) frame = requestAnimationFrame(render);
}
function render(time) {
  frame = 0;
  const started = performance.now();
  // Menus have their own controller/animation loop. Wake this loop on lifecycle
  // changes; a live shared ride keeps its original simulation clock advancing.
  if (!document.hidden && !lost && terrain && (!menuOpen || liveMultiplayer()) && (focused || liveMultiplayer())) scheduleRender();
  touchControls.refresh();
  let presentation = motorcyclePresentation.sample(simFrame, cameraFrame, time, focused && !lost && !menuOpen);
  // Enhanced/High: ATV frames between 60 Hz states (Original snaps to each).
  const atvSmooth = !presentation.interpolated && graphics.interpolation && (presentation = atvPresentation.sample(simFrame, cameraFrame, time, focused && !lost && !menuOpen)).interpolated;
  presentedFrame = presentation.frame; presentedCamera = presentation.camera;
  minimap.draw(time, presentedFrame, simReady && !raceSettings && !lost && focused && !document.hidden && !menuOpen, multiplayer?.remote ?? []);
  playerIndicators.root.hidden = !simReady || lost || document.hidden || menuOpen;
  // Enabled Freestyle GPS owns speed even while its raster is warming up or
  // temporarily hidden. Only the explicit Off setting restores standalone MPH.
  // The original race overlays sit below the toolbar buttons.
  const raceHud = raceSettings && simFrame?.race && simFrame.race.phase !== 'intro' ? simFrame.race : null;
  hud?.draw(simFrame?.hud, simReady && focused && !lost && !document.hidden && !menuOpen && simFrame?.race?.phase !== 'intro', { speedVisible: raceSettings ? raceOverlays.shown('speed') : !minimapSettings.enabled, lights: simFrame?.race?.lights ?? null, race: raceHud, frame: raceHud && (presentedFrame ?? simFrame), topInset: raceHud ? toolbarBottom() : 0, overlays: raceOverlays });
  raceOverlay.lamps = Boolean(hud?.lamps); raceOverlay.native = Boolean(hud?.race);
  raceOverlay.draw(simFrame?.race,simFrame,simReady && focused && !lost && !document.hidden && !menuOpen);
  if (lost || !gl || !terrain || document.hidden || (menuOpen && !liveMultiplayer())) { lastTime = time; stats.sample(time, 0, false); return; }
  const frameMs = time - (lastTime || time), dt = Math.min(.05, Math.max(0, frameMs / 1000)); lastTime = time;
  if (simReady && focused && !menuOpen && devicePixelRatio > 1) renderScale.sample(frameMs);
  const ratio = renderScale.pixelRatio(devicePixelRatio), width = Math.max(1, Math.round(canvasSize.width * ratio)), height = Math.max(1, Math.round(canvasSize.height * ratio));
  if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
  if (!menuOpen) updateCamera(dt);
  if (simReady && !simulationPaused()) {
    simAccumulator = Math.min(.1, simAccumulator + dt); void pumpSimulation();
    // Input callbacks may synchronously reset/pause and clear presentation.
    if(!simReady||simulationPaused()||!presentedFrame){stats.sample(time,performance.now()-started,false);return;}
    const center = diagnostics.terrainDetailCenter;
    const x = Math.max(0,Math.min(terrain.width-1,vehicle.position[0]/terrain.spacing)), z = Math.max(0,Math.min(terrain.depth-1,vehicle.position[2]/terrain.spacing));
    if (center && (Math.abs(x-center[0]) > 48 || Math.abs(z-center[1]) > 48)) buildTerrainMesh();
  }
  if (!focused) {
    // Shared rides still consume remote poses/audio events while this window's
    // expensive graphics are asleep. Physics and networking keep their clocks.
    if (liveMultiplayer()) void multiplayer.sample();
    playerIndicators.root.hidden = true; lastTime = time; stats.sample(time, 0, false); return;
  }
  diagnostics.motorcyclePresentation = simFrame?.vehicle_kind === 'motorcycle' ? { alpha: presentation.alpha, interpolated: presentation.interpolated, origin: (presentedFrame ?? simFrame).origin, orientation: (presentedFrame ?? simFrame).orientation } : null;
  const crashPose = atvSmooth && funnyCrashes ? presentedFrame.funny_crash : null;
  const smoothQuad = presentation.interpolated ? new Float32Array((crashPose ?? presentedFrame).quad_node_matrices.flat()) : vehicle?.nodeMatrices;
  const smoothRider = presentation.interpolated ? new Float32Array((crashPose?.rider_node_matrices.length ? crashPose : presentedFrame).rider_node_matrices.flat()) : vehicle?.rider?.gpuPose;
  const quadWorld=vehicle ? vehicleMatrix() : identity;
  const riderFrame = atvSmooth ? presentedFrame : simFrame;
  const opponents = (presentation.interpolated && presentedFrame?.race ? presentedFrame : simFrame)?.race?.opponents ?? [];
  const riderWorld=funnyCrashes && simFrame?.funny_crash?.rider_node_matrices.length ? identity : riderFrame?.rider_world_matrix ? new Float32Array(riderFrame.rider_world_matrix) : quadWorld;
  gl.viewport(0, 0, width, height); gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT | gl.STENCIL_BUFFER_BIT); gl.useProgram(program);
  gl.uniform1f(uniforms.motorcycleMudAmount,0); gl.uniform1i(uniforms.shadowReceive,false);
  gl.uniform1i(uniforms.useQuadPose, false); gl.uniform1i(uniforms.useEcosystem, false); gl.uniform1f(uniforms.alphaCutoff, 0);
  gl.uniform1i(uniforms.sceneryReflectionPlane, 0);
  gl.uniform1i(uniforms.useVertexColor, false); gl.uniform1i(uniforms.sceneryBlend, false); gl.uniform1i(uniforms.alphaFailPass, false); gl.uniform2f(uniforms.sceneryUvOffset, 0, 0);
  const actor=presentedFrame??simFrame;
  // The FOV kick and the speed pass share one smoothed amount per frame.
  speedRush.update({speed:actor?.speed_magnitude??0,intensity:crashSettings.speed,
    active:simReady && focused && !menuOpen && !lost && actor?.wreck?.phase==='Riding',dt});
  const { viewProjection, cull } = matrix();
  const shadowActive = Boolean(graphics.shadowMap && vehicle) && renderVehicleShadows({ opponents, quadWorld, riderWorld, smoothQuad, smoothRider, width, height });
  if (!shadowActive) diagnostics.shadowMap = null;
  playerIndicators.draw(multiplayer?.remote ?? [], multiplayer?.state.lobby, viewProjection, simFrame?.origin ?? [0,0,0], simReady && !menuOpen);
  const planes = viewPlanes(cull);
  let terrainDraws = 0, terrainTriangles = 0, sceneryDraws = 0, sceneryTriangles = 0, sceneryVisibleGroups = 0;
  gl.uniformMatrix4fv(uniforms.viewProjection, false, viewProjection); gl.uniform3f(uniforms.eye, camera.x, camera.y, camera.z);
  const crashView=diagnostics.cameraPresentation?.crash;
  if(crashView)gl.uniform4f(uniforms.crashFocus,...crashView.target,Math.hypot(...crashView.bounds.max.map((v,i)=>v-crashView.bounds.min[i]))/2+1);
  else gl.uniform4f(uniforms.crashFocus,0,0,0,0);
  const fog=terrain.world?.metadata.fog;
  gl.uniform2f(uniforms.heights, terrain.min, terrain.max);
  gl.uniform2f(uniforms.fogPacking,fog?.encoded_bias??524288,fog?.coefficient??0);
  gl.uniform1i(uniforms.fogEnabled,Boolean(fog)); diagnostics.fog=fog;
  const lights = terrain.world?.metadata.lights;
  const ambient = lights?.find(light => light.kind === 'Ambient')?.color.map(value => value / 255) ?? [.38, .38, .38];
  const sun = lights?.find(light => light.kind === 'Directional'), sunColor = sun?.color.map(value => value / 255) ?? [.8, .8, .8];
  const sunDirection = sun?.look?.map(value => -value) ?? (sun?.position && sun?.target ? sun.position.map((value, i) => value - sun.target[i]) : [-.6, 1, .35]);
  const fogColor = terrain.world?.metadata.fog_color.map(value => value / 255) ?? [.34, .43, .53];
  gl.uniform3fv(uniforms.ambientLight, ambient); gl.uniform3fv(uniforms.sunlightColor, sunColor); gl.uniform3fv(uniforms.sunlightDirection, sunDirection); gl.uniform3fv(uniforms.fogColor, fogColor); gl.uniform1i(uniforms.unlit, false);
  gl.clearColor(...fogColor, 1);
  gl.uniform3f(uniforms.surfaceColor, -1, -1, -1);
  gl.uniformMatrix4fv(uniforms.model, false, identity);
  gl.activeTexture(gl.TEXTURE0); gl.uniform1i(uniforms.baseTexture, 0); gl.uniform1i(uniforms.useTexture, 0); gl.bindTexture(gl.TEXTURE_2D, fallbackTexture);
  gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D_ARRAY, terrainAtlas); gl.uniform1i(uniforms.terrainColors, 1);
  gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D, terrainTextureMap); gl.uniform1i(uniforms.terrainMapping, 2);
  gl.activeTexture(gl.TEXTURE3); gl.bindTexture(gl.TEXTURE_2D, terrainDetailMap); gl.uniform1i(uniforms.terrainDetail, 3);
  const detail = terrain.textures?.detail;
  gl.uniform2f(uniforms.terrainDetailScale, detail ? 32 / detail.width : 0, detail ? 32 / detail.height : 0);
  gl.uniform1f(uniforms.terrainSpacing, terrain.spacing); gl.uniform1i(uniforms.useTerrainTexture, false);
  if (terrain.sky) {
    const skyMatrix = new Float32Array(identity); skyMatrix.set(terrain.sky.origin, 12);
    gl.uniformMatrix4fv(uniforms.model, false, skyMatrix);
    gl.uniform1i(uniforms.skyPass, true); gl.uniform1i(uniforms.useTexture, true);
    gl.uniform1i(uniforms.fogEnabled, false);
    gl.activeTexture(gl.TEXTURE0); gl.depthMask(false); gl.disable(gl.DEPTH_TEST);
    for (const chunk of skyChunks) {
      gl.bindTexture(gl.TEXTURE_2D, chunk.texture); gl.bindVertexArray(chunk.vao);
      gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0);
    }
    gl.depthMask(true); gl.enable(gl.DEPTH_TEST);
  }
  gl.uniformMatrix4fv(uniforms.model, false, identity); gl.uniform1i(uniforms.skyPass, false); gl.uniform1i(uniforms.useTexture, false);
  gl.uniform1i(uniforms.fogEnabled, Boolean(fog));
  gl.uniform1i(uniforms.useTerrainTexture, Boolean(terrain.textures));
  if (shadowActive) gl.uniform1i(uniforms.shadowReceive, true);
  // The original single-player terrain VU program supplies constant RGB,
  // not a normal-based sun pass. Keep authored base colors free of our
  // provisional prop/vehicle lighting.
  gl.uniform1i(uniforms.unlit, Boolean(terrain.textures));
  for (const chunk of chunks) {
    if (!chunk.count || outsideView(chunk.bounds, planes)) continue;
    gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0); terrainDraws++; terrainTriangles += chunk.count / 3;
  }
  gl.uniform1i(uniforms.useTerrainTexture, false); gl.uniform1i(uniforms.unlit, false); gl.activeTexture(gl.TEXTURE0);
  gl.uniform3f(uniforms.surfaceColor, .68, .70, .73);
  let previousSceneryChunk=null,previousSceneryUnlit=null,materialUpdates=0,rainDraws=0,rainVisibleInstances=0,rainMaterialUpdates=0;
  const rainName=terrain.rainModelName;
  for (const { name, chunk, matrix, unlit, bounds, flash } of sceneDraws) {
    if(flash>=0&&!litFlashes.has(flash))continue;
    if(outsideView(bounds,planes))continue;
    const rain=name.toLowerCase()===rainName;
    if(rain){rainDraws++;rainVisibleInstances++;}
    sceneryVisibleGroups++; sceneryDraws++; sceneryTriangles += chunk.count / 3;
    gl.uniformMatrix4fv(uniforms.model, false, matrix);
    //Original rain uses FB_ONLY. Preserve its per-placement accepted/failed
    //pass order; cache identical material state rather than reorder its pixels.
    if(previousSceneryChunk!==chunk||previousSceneryUnlit!==unlit){
      materialUpdates++;if(rain)rainMaterialUpdates++;
      previousSceneryChunk=chunk;previousSceneryUnlit=unlit;
      gl.uniform1i(uniforms.unlit, unlit);
      gl.uniform1f(uniforms.alphaCutoff, chunk.alphaCutoff);
      gl.uniform1i(uniforms.fogEnabled,Boolean(fog)&&chunk.fog);
      gl.uniform1i(uniforms.useVertexColor, chunk.prelit); gl.uniform1i(uniforms.sceneryBlend, chunk.blend);
      gl.uniform1i(uniforms.sceneryReflectionPlane, chunk.reflection);
      gl.depthFunc(chunk.passIndex>0?gl.LEQUAL:gl.LESS);
      if (chunk.reflectionNodes) gl.uniformMatrix4fv(uniforms.quadBones, false, chunk.reflectionNodes);
      gl.uniform2fv(uniforms.sceneryUvOffset, diagnostics.sceneryUv?.[chunk.material] ?? [0,0]);
      if (chunk.blend) { gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA, chunk.additive?gl.ONE:gl.ONE_MINUS_SRC_ALPHA); } else gl.disable(gl.BLEND);
      const sceneTexture = chunk.animationFrames?.[diagnostics.sceneryTextureFrames?.[chunk.material] ?? 0] ?? chunk.texture;
      gl.uniform1i(uniforms.useTexture, Boolean(sceneTexture)); gl.bindTexture(gl.TEXTURE_2D, sceneTexture ?? fallbackTexture);
      gl.bindVertexArray(chunk.vao);
    }
    gl.depthMask(chunk.depthWrite);gl.drawElements(gl.TRIANGLES,chunk.count,gl.UNSIGNED_SHORT,0);
    // GS FB_ONLY keeps failed fragments in the framebuffer without writing Z.
    // Split the mutually exclusive predicates so each source pixel blends once.
    if (chunk.alphaFailure === 1 && chunk.alphaCutoff > 0) {
      gl.uniform1i(uniforms.alphaFailPass, true); gl.depthMask(false);
      sceneryDraws++; sceneryTriangles += chunk.count / 3;if(rain)rainDraws++;
      gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0);
      gl.uniform1i(uniforms.alphaFailPass, false);
    }
  }
  diagnostics.sceneryMaterialUpdates=materialUpdates;
  diagnostics.rainRender={sourceInstances:terrain.rainSourceInstances,visibleInstances:rainVisibleInstances,draws:rainDraws,materialUpdates:rainMaterialUpdates,avoidedMaterialUpdates:rainVisibleInstances-rainMaterialUpdates,scope:'original geometry/RGBA/UV/FB_ONLY pass order; cached material state, no added storm'};
  gl.uniform1f(uniforms.alphaCutoff, 0); gl.uniform1i(uniforms.useVertexColor, false); gl.uniform1i(uniforms.sceneryBlend, false);
  gl.uniform1i(uniforms.sceneryReflectionPlane, 0);
  gl.uniform2f(uniforms.sceneryUvOffset, 0, 0); gl.depthMask(true); gl.depthFunc(gl.LESS); gl.disable(gl.BLEND);
  if (shadowActive) gl.uniform1i(uniforms.shadowReceive, false);
  if (trainRenderer) { trainRenderer.draw(uniforms, fallbackTexture, Boolean(fog)); diagnostics.train.draws = trainRenderer.draws; diagnostics.train.triangles = trainRenderer.triangles; }
  if (graphics.contactShadows && vehicle) drawContactShadows({ viewProjection, actor, quadWorld, riderWorld, smoothRider, opponents });
  else diagnostics.contactShadows = null;
  if (graphics.tireTracks && tireTracks) { tireTracks.draw({ viewProjection, now: simFrame?.wheel_effects?.time ?? 0 }); gl.useProgram(program); diagnostics.tireTracks = { draws: tireTracks.draws, segments: tireTracks.builder.count, capacity: tireTracks.builder.capacity, gpuBytes: tireTracks.gpuBytes }; }
  else diagnostics.tireTracks = null;
  drawEcosystem();
  gl.uniform1i(uniforms.unlit, false);
  if (vehicle) gl.uniformMatrix4fv(uniforms.model, false, vehicleMatrix());
  if (vehicle) gl.uniformMatrix4fv(uniforms.quadBones, false, smoothQuad);
  const rushMask=speedRush.amount>0;
  if(rushMask){gl.enable(gl.STENCIL_TEST);gl.stencilFunc(gl.ALWAYS,1,0xff);gl.stencilOp(gl.KEEP,gl.KEEP,gl.REPLACE);}
  const playerPaint = PAINT_PALETTE[rideAppearance?.paint ?? 0] ?? PAINT_PALETTE[0];
  for (const chunk of vehicleChunks) {
    gl.uniform2fv(uniforms.quadPaint, chunk.quad && paintable(chunk.material) ? playerPaint : PAINT_PALETTE[0]);
    gl.activeTexture(gl.TEXTURE4);gl.bindTexture(gl.TEXTURE_2D,chunk.mudTexture ?? fallbackTexture);
    gl.uniform1i(uniforms.mudReveal,simFrame?.vehicle_kind!=='motorcycle');
    gl.uniform1f(uniforms.motorcycleMudAmount,simFrame?.vehicle_kind==='motorcycle' && chunk.mudTexture ? Math.max(...simFrame.mud_amount) : graphics.dirt && chunk.mudTexture ? dirtBuildup.amount : 0);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform1i(uniforms.fogEnabled,Boolean(fog)&&chunk.fog);
    gl.uniformMatrix4fv(uniforms.model, false, chunk.rider ? riderWorld : quadWorld);
    gl.uniform1i(uniforms.useQuadPose, Boolean(chunk.quad || chunk.rider));
    if (chunk.quad) gl.uniformMatrix4fv(uniforms.quadBones, false, smoothQuad);
    if (chunk.rider) gl.uniformMatrix4fv(uniforms.quadBones, false, smoothRider);
    gl.uniform1i(uniforms.useTexture, chunk.texture ? 1 : 0); gl.bindTexture(gl.TEXTURE_2D, chunk.texture ?? fallbackTexture);
    gl.bindVertexArray(chunk.vao);
    if(chunk.rider && chunk.cutCount!==null && chunk.cutCount!==undefined)gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,chunk.cutBuffer);
    gl.drawElements(gl.TRIANGLES, chunk.rider ? chunk.cutCount??chunk.count : chunk.count, gl.UNSIGNED_SHORT, 0);
    if(chunk.rider && chunk.cutCount!==null && chunk.cutCount!==undefined)gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,chunk.indices);
  }
  if(rushMask)gl.disable(gl.STENCIL_TEST);
  gl.uniform2fv(uniforms.quadPaint, PAINT_PALETTE[0]);
  gl.uniform1f(uniforms.motorcycleMudAmount,0);
  let cpuDraws = 0, cpuTriangles = 0;
  for (const [slot, cpu] of opponents.entries()) {
    if (Math.hypot(cpu.origin[0]-camera.x,cpu.origin[2]-camera.z) > 864) continue;
    const transform = vehicleMatrix({origin:cpu.origin,orientation:cpu.physical?.orientation ?? [Math.cos(cpu.yaw/2),0,Math.sin(cpu.yaw/2),0]});
    const riderTransform = cpu.physical?.rider_world_matrix ? new Float32Array(cpu.physical.rider_world_matrix) : transform;
    const rosterDraws = drawRosterCpu(cpu, slot, transform, riderTransform);
    if (rosterDraws) { cpuDraws += rosterDraws; continue; }
    const quadMatrices = cpu.physical ? new Float32Array(cpu.physical.quad_matrices.flat()) : cpuQuadMatrices;
    const riderMatrices = cpu.physical ? new Float32Array(cpu.physical.rider_matrices.flat()) : cpuRiderMatrices;
    for (const chunk of vehicleChunks) {
      gl.uniformMatrix4fv(uniforms.model,false,chunk.rider ? riderTransform : transform);
      gl.uniform1i(uniforms.useQuadPose,Boolean(chunk.quad || chunk.rider));
      gl.uniformMatrix4fv(uniforms.quadBones,false,chunk.rider ? riderMatrices : quadMatrices);
      gl.uniform1i(uniforms.useTexture,chunk.texture ? 1 : 0); gl.bindTexture(gl.TEXTURE_2D,chunk.texture ?? fallbackTexture);
      gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES,chunk.count,gl.UNSIGNED_SHORT,0); cpuDraws++; cpuTriangles += chunk.count/3;
    }
  }
  drawRemotePlayers();
  void multiplayer?.sample();
  if (waterRenderer) {
    waterRenderer.draw({ camera, viewProjection, focalLength: (cameraFrame ? diagnostics.cameraPresentation.focalLength : 1 / Math.tan(Math.PI / 6)) * speedRush.focalScale, aspect: canvas.width / canvas.height,
      farDistance: fog?.end ?? Math.max(terrain.sizeX, terrain.sizeZ) * 5, fogColor, fog, fresnel: graphics.fresnel });
    diagnostics.waterRender.draws = waterRenderer.draws;
    diagnostics.waterRender.gpuBytes = waterRenderer.gpuBytes;
    diagnostics.waterRender.cpuMilliseconds = waterRenderer.cpuMilliseconds;
    diagnostics.waterRender.gpuMilliseconds = waterRenderer.gpuMilliseconds;
  }
  if(ambientEffectsRenderer){ambientEffectsRenderer.draw({camera,viewProjection,farDistance:fog?.end??768,fogColor,fog});Object.assign(diagnostics.ambientEffects,{draws:ambientEffectsRenderer.draws,visible:ambientEffectsRenderer.visible,gpuBytes:ambientEffectsRenderer.gpuBytes,cpuMilliseconds:ambientEffectsRenderer.cpuMilliseconds,gpuSlots:ambientEffectsRenderer.streams?.state.count??0,gpuWraps:ambientEffectsRenderer.streams?.state.wraps??0,gpuUploadCalls:ambientEffectsRenderer.streams?.uploadCalls??0});}
  if(snowRenderer){snowRenderer.draw({camera,viewProjection});Object.assign(diagnostics.snow,{draws:snowRenderer.draws,uploadBytes:snowRenderer.uploadBytes});}
  if (crashSettings.mode === 'gorn' && (bloodCount(simFrame?.funny_crash?.blood) || simFrame?.funny_crash?.blood?.stumps?.length)) {
    bloodRenderer ??= new BloodRenderer(gl);
    bloodRenderer.draw(simFrame.funny_crash.blood, {camera,viewProjection,program});
  } else if (bloodRenderer) bloodRenderer.draws = 0;
  if (wheelEffectsRenderer) {
    wheelEffectsRenderer.draw({ camera, viewProjection });
    diagnostics.wheelEffects.draws = wheelEffectsRenderer.draws; diagnostics.wheelEffects.triangles = wheelEffectsRenderer.triangles; diagnostics.wheelEffects.cpuMilliseconds = wheelEffectsRenderer.cpuMilliseconds; diagnostics.wheelEffects.wetLive = wheelEffectsRenderer.wetLive ?? 0;
  }
  if (graphics.dust && lingeringDust) {
    lingeringDust.draw({ viewProjection, camera, now: simFrame?.wheel_effects?.time ?? 0, light: [1.1, 1.1, 1.1], fogColor, fogEnd: fog?.end ?? 768 });
    diagnostics.lingeringDust = { draws: lingeringDust.draws, live: lingeringDust.emitter.live, capacity: lingeringDust.emitter.capacity, gpuBytes: lingeringDust.gpuBytes };
  } else diagnostics.lingeringDust = null;
  if(speedRush.amount>0){
    speedRush.draw({width,height,active:true,intensity:crashSettings.speed,
      focus:speedRushFocus(viewProjection,[camera.x,camera.y,camera.z],actor?.linear_velocity??simFrame?.linear_velocity??[0,0,0]),
      vehicle:speedRushBounds(viewProjection,actor.origin)});
  } else speedRush.draws=0;
  diagnostics.visibility = { terrainDraws, terrainTriangles, sceneryDraws, sceneryTriangles, terrainCulled: chunks.length - terrainDraws, sceneryCulled: sceneDraws.length - sceneryVisibleGroups };
  diagnostics.frames++; diagnostics.renderRatio = renderScale.ratio;
  diagnostics.graphics = { setting: graphicsSettings.preset, ...graphics, anisotropyApplied: graphics.anisotropy ? anisotropyLevel() : 0, interpolated: Boolean(atvSmooth), dirtLevel: dirtBuildup.level, dirtApplied: graphics.dirt ? dirtBuildup.amount : 0, wheelLinkage: Boolean(quadLinkage), wheelLift };
  diagnostics.vibration = { enabled: vibrationSettings.enabled, pulses: rumble.pulses, last: rumble.last };
  if (stats.mode !== 'off') {
    diagnostics.cpuDraws = cpuDraws; diagnostics.cpuTriangles = cpuTriangles;
    diagnostics.drawCalls = speedRush.draws + cpuDraws + terrainDraws + sceneryDraws + vehicleChunks.length + (terrain.sky ? skyChunks.length : 0) + diagnostics.vegetationDraws + (waterRenderer?.draws ?? 0) + (trainRenderer?.draws ?? 0) + (wheelEffectsRenderer?.draws ?? 0) + (ambientEffectsRenderer?.draws ?? 0) + (snowRenderer?.draws ?? 0) + (diagnostics.contactShadows?.draws ?? 0) + (diagnostics.tireTracks?.draws ?? 0) + (diagnostics.shadowMap?.draws ?? 0) + (diagnostics.lingeringDust?.draws ?? 0);
    diagnostics.drawTriangles = cpuTriangles + (terrainTriangles * 3 + sceneryTriangles * 3 + vehicleChunks.reduce((sum, c) => sum + c.count, 0) + (terrain.sky ? skyChunks.reduce((sum, c) => sum + c.count, 0) : 0)) / 3 + diagnostics.vegetationTriangles + (trainRenderer?.triangles ?? 0) + (wheelEffectsRenderer?.triangles ?? 0) + (ambientEffectsRenderer?.triangles ?? 0) + (snowRenderer?.triangles ?? 0);
  }
  if (diagnostics.frames === 1 || diagnostics.frames % 120 === 0) {
    diagnostics.glError = gl.getError();
    if (diagnostics.glError) status(`Graphics error ${diagnostics.glError}.`, true);
  }
  stats.sample(time, performance.now() - started, focused);
}
async function loadGame(file = null, environment = "env01", racing = null) {
  if (!gl || lost) throw new Error('Graphics context is unavailable; wait for recovery.');
  const directStart = frontend?.root.hidden;
  const token = ++generation;
  if (frontend && !file) await frontend.beginLoading(environment);
  status('Opening terrain…'); diagnostics.ready = false;
  minimap.setTerrain(null);
  audio.close(); audio = new GameAudio(codec, message => { diagnostics.audioError = message; status(`Audio: ${message}`, true); });
  frontend?.syncPreview();
  delete diagnostics.audioError; void audio.resume().catch(error => { diagnostics.audioError = error.message; });
  simReady = false; simGeneration++; simAccumulator = 0; simFrame = undefined; $('disc').disabled = true;
  try {
    raceSettings = racing; raceCourse = null; raceOverlay.setCourse(null); await command(96,{active:false});
    let source;
    if (file) source = file.name.toLowerCase().endsWith('.iso') ? await discTerrain(file, token) : { bytes: await read(file, 0, file.size), name: file.name };
    else {
      const content = await (hostedContent ??= openHostedContent());
      if (token !== generation) return;
      await prefetchRide(content, environment, Boolean(racing));
      source = await terrainFromContent(content.file, content.files, token, environment,Boolean(racing));
    }
    diagnostics.contentSource = file ? 'local-file' : 'hosted-original';
    const info = await json(9, source.bytes), raw = await codec(10, source.bytes);
    await renderCoreReady;
    if (token !== generation) return;
    renderCore.call(141, source.bytes);
    quadLinkage = null;
    if (!source.motorcycle && source.meshBytes && source.physics) {
      const framed = new Uint8Array(4 + source.meshBytes.length + source.physics.length);
      new DataView(framed.buffer).setUint32(0, source.meshBytes.length, true); framed.set(source.meshBytes, 4); framed.set(source.physics, 4 + source.meshBytes.length);
      try { quadLinkage = renderCore.call(144, framed); } catch (error) { diagnostics.wheelReprojectionError = error.message; }
    }
    const width = info.width_cells + 1, depth = info.depth_cells + 1;
    if (!Number.isInteger(width) || !Number.isInteger(depth) || width < 2 || depth < 2 || !Number.isFinite(info.spacing) || info.spacing <= 0 || raw.byteLength !== width * depth * 4) throw new Error('Invalid terrain dimensions.');
    const heights = new Float32Array(width * depth), view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    let min = Infinity, max = -Infinity;
    for (let i = 0; i < heights.length; i++) { const value = view.getFloat32(i * 4, true); if (!Number.isFinite(value)) throw new Error('Nonfinite terrain height.'); heights[i] = value; min = Math.min(min, value); max = Math.max(max, value); }
    terrain = { weather: source.weather, snowSource:source.snow, width, depth, spacing: info.spacing, heights, min, max, sizeX: (width - 1) * info.spacing, sizeZ: (depth - 1) * info.spacing, textures: source.terrainTextures, world: source.world, sky: source.sky, ecosystem: source.ecosystem, trainSource: source.train, wheelEffectsSource: source.wheelEffects, ambientEffectsSource:source.ambientEffects };
    terrain.rainModelName=source.weather?.definition?.kind==='rain'?source.weather.definition.model.replace(/\.slt$/i,'').toLowerCase():null;
    terrain.rainSourceInstances=source.world?.metadata.models.filter(model=>model.name.toLowerCase()===terrain.rainModelName).length ?? 0;
    minimap.setTerrain(racing ? null : terrain);
    diagnostics.terrainTextures = 0;
    uploadTerrainTextures();
    if (hud && source.physics) {
      await codec(63, await hud.init(environment, Boolean(racing)));
    }
    setVehicle(source); buildMesh(); await startSimulation(source); resetCamera();
    Object.assign(diagnostics, { ready: true, source: source.name, width, depth, min, max, frames: 0 });
    $('reset').disabled = false; $('camera').disabled = !simReady; $('help').hidden = false;
    $('reset').textContent = simReady ? 'Reset ATV' : 'Reset view';
    $('help').textContent = simReady ? 'Pause → Controls: keyboard / controller bindings · left touch: steer / lean · right: drive / brake on ground; swipe, hold or circle in air' : 'Free camera · drag to look · WASD to move · Q/E up/down';
    status(simReady ? 'Physics reconstruction. Handling, collisions, animation and sound are incomplete.' : 'Static terrain. Open the disc to load the ATV.'); scheduleRender();
    if (directStart && frontend && !file) { frontend.busy = false; frontend.resume(); }
  } catch (error) { if (token === generation) status(error.message, true); throw error; }
  finally { if (token === generation) $('disc').disabled = false; }
}
// Anonymous usage context only: mode, venue, vehicle number and race settings.
const vehicleKind = quad => quad > 26 ? 'motorcycle' : 'atv';
const onlineRide = map => ({ mode: multiplayer?.shared ? 'freeride' : 'multiplayer', activity: 'freestyle', map, vehicle: 'atv', quad: frontend?.quadId });
async function startRide(environment, racing) {
  const quad = frontend?.quadId, started = performance.now();
  const ride = { mode: 'single', activity: racing ? 'race' : 'freestyle', map: environment, vehicle: vehicleKind(quad), quad, ...(racing ? { laps: racing.laps, cpu: racing.cpu } : {}) };
  try { await loadGame(null, environment, racing); }
  catch (error) { usage.rideFailed(ride); throw error; }
  if (simReady) usage.rideStarted(ride, { seconds: (performance.now() - started) / 1000 });
}
$('disc').addEventListener('change', event => { const file = event.target.files[0]; if (file) void loadGame(file).catch(error => status(error.message, true)); });
$('reset').onclick = async (options) => {
  if (!simStart || !vehicle) { resetCamera(); return; }
  // During a race, Reset returns to the last on-track point facing the race
  // direction; Restart Race / Race Again reload the whole event.
  const race = simFrame?.race;
  let start = simStart;
  if (raceSettings) {
    // After the finish the CPU driver has the quad; only Restart reloads.
    if (race?.phase === 'finished' && options?.restart !== true) return;
    if (options?.restart === true || race?.phase !== 'racing' || !race.track_point) { raceRestarting = options?.restart === true; await loadGame(null, raceSettings.course ?? 'env01', raceSettings); return; }
    start = await command(140, { position: race.track_point, yaw: race.track_heading, course: true, slot: null });
  }
  impactEvents.reset();
  simGeneration++; simReady = false; simAccumulator = 0; clearInput();
  try { await audio.reset(); dirtBuildup.reset(); tireTracks?.reset(); lingeringDust?.reset(); acceptFrame(await command(23, start)); await command(47, {}); orbit = 0; acceptCameraFrame(await command(46, { dt: 1 / 60, orbit })); simReady = true; buildMesh(); resetCamera(); }
  catch (error) { status(error.message, true); }
};
$('menu').onclick = () => { void frontend?.show('pause'); };
$('camera').onclick = () => { if (simReady) cameraCycle = true; canvas.focus({ preventScroll: true }); };
$('fullscreen').onclick = async () => {
  try { document.fullscreenElement ? await document.exitFullscreen() : await document.documentElement.requestFullscreen(); }
  catch { status('Fullscreen is unavailable in this browser; the view already fills the window.'); }
};
canvas.addEventListener('pointerdown', event => {
  if (!terrain) return;
  if (simReady && (menuOpen || document.hidden || lost)) return;
  event.preventDefault();
  const left = event.pointerType === 'touch' && event.clientX < innerWidth / 2;
  // An unclaimed right gameplay touch must never fall through to mouse orbit.
  if (simReady && event.pointerType === 'touch' && !left) return;
  if (left && !touchLean.begin(event.pointerId, event.clientX, event.clientY)) return;
  focused = true; canvas.focus({ preventScroll: true }); syncAudio(); scheduleRender(); canvas.setPointerCapture(event.pointerId);
  pointers.set(event.pointerId, { x: event.clientX, y: event.clientY, startX: event.clientX, startY: event.clientY, dx: 0, dy: 0, touch: event.pointerType === 'touch', move: event.pointerType === 'touch' && event.clientX < innerWidth / 2 });
});
canvas.addEventListener('pointermove', event => {
  const pointer = pointers.get(event.pointerId); if (!pointer) return;
  if (pointer.move) touchLean.move(event.pointerId, event.clientX, event.clientY);
  pointer.dx = Math.max(-70, Math.min(70, event.clientX - pointer.startX)); pointer.dy = Math.max(-70, Math.min(70, event.clientY - pointer.startY));
  if (!pointer.move) {
    if (simReady) { if (!pointer.touch) orbit -= (event.clientX - pointer.x) * .004; }
    else { camera.yaw += (event.clientX - pointer.x) * .004; camera.pitch -= (event.clientY - pointer.y) * .004; }
  }
  pointer.x = event.clientX; pointer.y = event.clientY;
});
for (const name of ['pointerup', 'pointercancel', 'lostpointercapture']) canvas.addEventListener(name, event => { touchLean.end(event.pointerId, name !== 'pointerup'); pointers.delete(event.pointerId); });
// While riding, a click on the page or a toolbar button must not silently
// drop driving keys. Menus, dialogs, race results and fields keep their focus.
function keyboardTarget() {
  const active = document.activeElement;
  if (active === canvas) return true;
  if (!simReady || menuOpen || (active && active !== document.body && !$('toolbar').contains(active))) return false;
  canvas.focus({ preventScroll: true });
  return true;
}
window.addEventListener('keydown', event => {
  if (event.ctrlKey || event.metaKey) return;
  // A key held across blur/pause may resume with OS repeat events. It must be
  // released and pressed again after cancellation before becoming new input.
  if (event.repeat && !keys.has(event.code)) return;
  const bound = simReady ? gameplayBindings.usesKey(event.code) : ['KeyW', 'KeyA', 'KeyS', 'KeyD', 'KeyQ', 'KeyE', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ShiftLeft'].includes(event.code);
  if (!bound || !keyboardTarget()) return;
  event.preventDefault(); keys.add(event.code); if (simReady) gameplayBindings.observeKeys(keys, event.repeat ? null : event.code);
});
const inIntro = () => simReady && !menuOpen && simFrame?.race?.phase === 'intro';
window.addEventListener('keydown', event => { if (!event.repeat && !event.ctrlKey && !event.metaKey && inIntro()) introSkip = true; }, true);
window.addEventListener('pointerdown', event => { if (inIntro() && !$('toolbar').contains(event.target)) introSkip = true; }, true);
window.addEventListener('keyup', event => { keys.delete(event.code); gameplayBindings.observeKeys(keys); });
const clearInput = () => { speedRush.reset(); motorcyclePresentation.reset(); atvPresentation.reset(); presentedFrame = presentedCamera = undefined; keys.clear(); pointers.clear(); touchControls.clear(); touchLean.clear(); gameplayBindings.cancel(); simAccumulator = 0; cameraCycle = false; diagnostics.input = { throttle: 0, brake: 0, steer: 0, lean: 0, reverse: false }; };
gameplayBindings.subscribe(clearInput);
window.addEventListener('gamepaddisconnected', clearInput);
const syncAudio = () => { if (!focused || document.hidden || lost || menuOpen || pageSuspended) impactEvents.reset(); void audio.setActive(focused && !document.hidden && !lost && !menuOpen && !pageSuspended).catch(error => { diagnostics.audioError = error.message; }); };
window.addEventListener('blur', () => { focused = false; clearInput(); syncAudio(); scheduleRender(); });
window.addEventListener('focus', () => { focused = true; lastTime = 0; syncAudio(); scheduleRender(); });
document.addEventListener('visibilitychange', () => { usage.setVisible(!document.hidden); if (document.hidden) usage.report(1); clearInput(); lastTime = 0; syncAudio(); scheduleRender(); });
window.addEventListener('resize', () => { scheduleRender(); });
canvas.addEventListener('webglcontextlost', event => { event.preventDefault(); lost = true; for(const model of remoteModels.values()){model.chunks=null;model.gpuTextures=[];for(const entry of model.gear?.values()??[])entry.gpu=null;} for(const cpu of raceCpuModels)cpu.gpu=null; diagnostics.ready = false; clearInput(); syncAudio(); frontend?.setLoading(true, 'Restoring graphics…'); status('Restoring graphics…'); });
// Every old GPU handle belongs to the lost context, including vegetation instances.
// Discard those references before buildMesh runs its normal resource cleanup.
canvas.addEventListener('webglcontextrestored', () => { try { lost = false; glShadow?.reset(); contactShadows = undefined; shadowMap = undefined; anisotropyExtension = undefined; anisotropicTextures = new Map(); tireTracks?.restore(); lingeringDust?.restore(); speedRush = new SpeedRush(gl, { coarse: coarsePointer() }); snowRenderer=undefined; ambientEffectsRenderer=undefined; bloodRenderer = undefined; wheelEffectsRenderer = undefined; trainRenderer = undefined; waterRenderer = undefined; chunks = []; vehicleChunks = []; sceneChunks = []; sceneDraws = []; skyChunks = []; ecoGroups = []; ecoBillboard = null; gpuTextures = []; initializeGl(); if (terrain) { buildMesh(); uploadTerrainTextures(); } syncAudio(); diagnostics.ready = Boolean(terrain); frontend?.setLoading(false); scheduleRender(); status(simReady ? 'Physics reconstruction. Handling, collisions, animation and sound are incomplete.' : 'Static terrain.'); } catch (error) { status(error.message, true); } });
window.addEventListener('pagehide', event => {
  usage.report(1);
  cancelAnimationFrame(frame); frame = 0; clearInput();
  if (event.persisted) {
    // Back/forward cache retains this page. Its live Rust worker and audio
    // resources must survive too; closing them leaves a restored frozen ATV.
    pageSuspended = true; focused = false; syncAudio();
  } else { multiplayer?.close(true); audio.close(); worker.terminate(); }
});
window.addEventListener('pageshow', event => {
  if (!event.persisted || !pageSuspended) return;
  pageSuspended = false; focused = document.hasFocus(); lastTime = 0;
  clearInput(); syncAudio();
  if (!frame) frame = requestAnimationFrame(render);
});
try { if (!gl) throw new Error('WebGL 2 is required to render this terrain.'); initializeGl(); frame = requestAnimationFrame(render); const params = new URLSearchParams(location.search);
  if (params.get('source') === 'disc') { $('loading-screen').hidden = true; menuOpen = false; $('frontend').hidden = true; }
  else {
    hostedContent = openHostedContent();
    multiplayer = new Multiplayer({codec,command,spawn:async peer=>{
      simGeneration++; simAccumulator=0; clearInput();
      acceptFrame(await command(86,{peer,now:Math.floor(performance.now())}));
      await command(47,{}); orbit=0; acceptCameraFrame(await command(46,{dt:1/60,orbit}));
      await multiplayer.send();
    },ready:()=>simReady,paused:simulationPaused,listener:()=>({position:[camera.x,camera.y,camera.z],forward:[Math.sin(camera.yaw)*Math.cos(camera.pitch),Math.sin(camera.pitch),Math.cos(camera.yaw)*Math.cos(camera.pitch)],velocity:simFrame?.linear_velocity??[0,0,0]}),audio:()=>audio,assets:remoteAssets,clearAssets:()=>{for(const model of remoteModels.values())disposeRemoteGpu(model);remoteModels.clear();},report:message=>{diagnostics.multiplayerError=message;status(message,true);},onState:()=>{if(frontend){void frontend.refresh();}},onStart:async (environment,isCurrent,pose)=>{
      if(!isCurrent())return;
      frontend.busy=true;frontend.notice.textContent='';const started=performance.now();
      try {
        await loadGame(null,environment);
        // A vehicle change reloads in place rather than at the start line.
        if(pose&&isCurrent()&&simReady){simGeneration++;simAccumulator=0;acceptFrame(await command(23,{origin:pose.origin,yaw:pose.yaw}));await command(47,{});orbit=0;acceptCameraFrame(await command(46,{dt:1/60,orbit}));buildMesh();}
        if(isCurrent()){frontend.raceActive=false;frontend.activeSetup={map:environment,activity:'freestyle',quadId:frontend.quadId,riderId:frontend.riderId};frontend.resume();usage.rideStarted({...onlineRide(environment),players:multiplayer.state.lobby?.players?.length},{seconds:(performance.now()-started)/1000});}
      }
      catch(error){if(isCurrent()){usage.rideFailed(onlineRide(environment));await frontend.show(multiplayer.shared?'main':'multiplayer');frontend.fail(error);throw error;}}
      finally{if(isCurrent())frontend.busy=false;}
    },onLeave:message=>{usage.rideEnded();for(const model of remoteModels.values())disposeRemoteGpu(model);remoteModels.clear();if(frontend){void frontend.show(multiplayer.shared?'main':'multiplayer').then(()=>{frontend.notice.textContent=message;});}}});
    frontend = new Frontend({minimapSettings,raceOverlays,menuRider:menuRiderAssets,modelAssets:selectionAssets,gearPreview:gearTextures,crashSettings,graphicsSettings,vibrationSettings,multiplayer,codec, json, content: hostedContent, read, diagnosticsOverlay: stats, mobileControls: touchControls, audio: () => audio, start: (environment,racing) => startRide(environment,racing), active: open => { menuOpen = open; usage.setRiding(!open); if (open) usage.report(10); keyboardHelp.setRiding(!open); clearInput(); lastTime = 0; scheduleRender(); document.body.classList.toggle('menu-open', open); syncAudio(); if (!open) canvas.focus({preventScroll:true}); }});
    hud = new HeadUpDisplay(frontend, canvas);
    document.body.classList.add('menu-open'); void frontend.init();
    usage.track('session_start', { entry: parseLaunch(location.search).requested ? 'link' : 'menu' });
  } }
catch (error) { $('loading-screen').hidden = true; status(error.message, true); $('menu-notice').textContent = error.message; }
