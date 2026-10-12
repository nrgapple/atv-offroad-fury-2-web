// Original content and vehicle component simulation run in the Rust worker.
import { Multiplayer } from './multiplayer.js';
import { Frontend } from './frontend.js';
import { mountTouchControls, TouchLeanIntent } from './touch.js';
import { GameAudio } from './audio.js';
import { openHostedContent } from './content.js';
import { gameplayBindings } from './bindings.js';
import { HeadUpDisplay } from './hud.js';
import { DiagnosticsOverlay } from './diagnostics.js';
import { WaterRenderer } from './water.js';
import { loadTrain, TrainRenderer } from './train.js';
import { loadWheelEffects, WheelEffectsRenderer } from './wheel-effects.js';
const $ = id => document.getElementById(id);
const canvas = $('game');
const gl = canvas.getContext('webgl2', { antialias: true, alpha: false, powerPreference: 'high-performance' });
const status = (text, error = false) => { $('status').textContent = text; $('status').classList.toggle('error', error); };
const decoder = new TextDecoder();
const encoder = new TextEncoder();
const worker = new Worker('worker.js', { type: 'module' });
const pending = new Map();
let requestId = 0, generation = 0, terrain, vehicle, program, uniforms, chunks = [], vehicleChunks = [], gpuTextures = [], fallbackTexture, terrainAtlas, terrainTextureMap, terrainDetailMap, frame = 0, lastTime = 0, lost = false;
let focused = document.hasFocus(), gamepadsAvailable = true, menuOpen = true;
let frontend, hostedContent, hud, multiplayer;
const remoteModels = new Map();
let simReady = false, simBusy = false, simAccumulator = 0, simFrame, simStart, simGeneration = 0, orbit = 0, cameraFrame, cameraCycle = false;
let sceneChunks = [], sceneDraws = [], skyChunks = [], ecoGroups = [];
let waterRenderer;
let trainRenderer;
let wheelEffectsRenderer;
const identity = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
const camera = { x: 0, y: 0, z: 0, yaw: 0, pitch: -.35 };
const keys = new Set(), pointers = new Map();
const touchLean = new TouchLeanIntent(() => touchControls.steeringSensitivity);
const touchControls = mountTouchControls({
  surface: canvas,
  active: () => simReady && focused && !document.hidden && !menuOpen && !lost,
  focus: () => { focused = true; canvas.focus({ preventScroll: true }); syncAudio(); },
});
const diagnostics = { ready: false, frames: 0, vertices: 0, triangles: 0, chunks: 0, glError: 0, camera: [], source: null };
let audio = new GameAudio(codec, message => { diagnostics.audioError = message; status(`Audio: ${message}`, true); });
const stats = new DiagnosticsOverlay({ snapshot: () => ({ ...diagnostics, frame: simFrame, paused: menuOpen, focused, resolution: [canvas.width, canvas.height] }) });
// Read-only snapshot for verification; does not expose the locally opened file.
Object.defineProperty(window, 'atvTerrainStatus', { get: () => ({ ...diagnostics, performance: stats.snapshot(), audio: audio.snapshot(), simulation: simReady, multiplayer: multiplayer ? {state:structuredClone(multiplayer.state),remote:structuredClone(multiplayer.remote),models:[...remoteModels.keys()]} : null, frame: simFrame ? structuredClone(simFrame) : null, camera: [camera.x, camera.y, camera.z, camera.yaw, camera.pitch] }) });
worker.onmessage = ({ data }) => {
  if (data.startupError) { status(data.startupError, true); return; }
  const request = pending.get(data.id);
  if (!request) return;
  pending.delete(data.id);
  if (Number.isFinite(data.wasmBytes)) diagnostics.wasmBytes = data.wasmBytes;
  if (request.op === 50 && Number.isFinite(data.operationMs)) diagnostics.physicsBatchMs = data.operationMs;
  data.error ? request.reject(new Error(data.error)) : request.resolve(data.bytes);
};
worker.onerror = event => {
  status(`Decoder failed: ${event.message}`, true);
  for (const request of pending.values()) request.reject(new Error('Decoder failed; reload the page.'));
  pending.clear();
};
function codec(op, bytes, index = 0) {
  return new Promise((resolve, reject) => {
    const id = ++requestId;
    pending.set(id, { resolve, reject, op });
    worker.postMessage({ id, op, index, bytes });
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
async function terrainFromContent(file, files, token, environment = "env01") {
  if (!["env01", "env02", "env03", "env04", "env05", "env06", "env07", "env08", "free05"].includes(environment)) throw new Error("Environment is not connected yet.");
  const number = environment.slice(-2), vancouver = environment === 'free05';
  const directory = vancouver ? 'FREE05' : `ENV${number}`;
  const archiveStem = vancouver ? 'F05' : `E${number}`;
  const sceneStem = vancouver ? environment : `${environment}fr`;
  const memberNamed = (entry, name) => entry.name.split(/[\\/]/).at(-1).toLowerCase() === name.toLowerCase();
  status('Loading original engine sounds…');
  await audio.load({ file, files, read, isCurrent: () => token === generation, quadId: frontend?.quadId ?? 1 });
  const archive = files.find(entry => entry.path === `/TRACKS/TRACKSA/${directory}/${archiveStem}_FR.RES`);
  if (!archive) throw new Error(`Original ${environment.toUpperCase()} terrain archive is missing.`);
  status('Reading original terrain…');
  const resource = await read(file, archive.lba * 2048, archive.bytes);
  const entries = await json(2, resource);
  const index = entries.findIndex(entry => memberNamed(entry, `${environment}.tr2`));
  if (index < 0) throw new Error('Original terrain is missing from the archive.');
  const sceneIndex = entries.findIndex(entry => memberNamed(entry, `${sceneStem}.scn`));
  if (sceneIndex < 0) throw new Error('Original start position is missing.');
  const modeIndex = entries.findIndex(entry => memberNamed(entry, `${sceneStem}.toy`));
  const mode = modeIndex < 0 ? null : decoder.decode(await codec(3, resource, modeIndex));
  const sceneMetadata = await command(60, { scene: decoder.decode(await codec(3, resource, sceneIndex)), mode });
  const waterIndex = sceneMetadata.water_file ? entries.findIndex(entry => memberNamed(entry, sceneMetadata.water_file.toLowerCase())) : -1;
  if (sceneMetadata.water_file && waterIndex < 0) throw new Error('Original water definition is missing.');
  const water = waterIndex < 0 ? null : { resource, index: waterIndex };
  const cameraIndex = entries.findIndex(entry => entry.name.split(/[\\/]/).at(-1).toLowerCase() === 'followcampresets.pb');
  if (cameraIndex < 0) throw new Error('Original camera presets are missing.');
  const cameraPresets = await codec(3, resource, cameraIndex);
  const scene = { position: sceneMetadata.start_position, yaw: Math.atan2(sceneMetadata.start_direction[0], sceneMetadata.start_direction[2]) };
  const model = await json(89, new Uint8Array([frontend?.riderId ?? 0]), frontend?.quadId ?? 1);
  diagnostics.playerModel = model;
  const quadFile = files.find(entry => entry.path === model.quad);
  if (!quadFile) throw new Error('Original ATV archive is missing.');
  status('Reading original ATV…');
  const quadArchive = await read(file, quadFile.lba * 2048, quadFile.bytes);
  const quadEntries = await json(2, quadArchive);
  const quadIndex = quadEntries.findIndex(entry => memberNamed(entry, model.quad_mesh));
  if (quadIndex < 0) throw new Error('Original ATV mesh is missing.');
  const meshBytes = await codec(3, quadArchive, quadIndex), mesh = await json(11, meshBytes);
  const physicsIndex = quadEntries.findIndex(entry => memberNamed(entry, model.physics));
  if (physicsIndex < 0) throw new Error('Original ATV physics is missing.');
  const physics = await codec(3, quadArchive, physicsIndex);
  const chassisIndex = quadEntries.findIndex(entry => memberNamed(entry, model.chassis));
  if (chassisIndex < 0) throw new Error('Original chassis collision is missing.');
  const chassis = await codec(3, quadArchive, chassisIndex);
  const actorIndex = quadEntries.findIndex(entry => memberNamed(entry, model.actor));
  if (actorIndex < 0) throw new Error('Original actor collision rays are missing.');
  const actorRays = await codec(3, quadArchive, actorIndex);
  let toyIndex = entries.findIndex(entry => memberNamed(entry, `${environment}.toy`)), toy;
  if (toyIndex >= 0) toy = await codec(3, resource, toyIndex);
  else {
    const sceneArchiveFile = files.find(entry => entry.path === `/TRACKS/TRACKSA/ENV${number}/E${number}_ENDL.RES`);
    if (!sceneArchiveFile) throw new Error('Original terrain materials are missing.');
    const sceneArchive = await read(file, sceneArchiveFile.lba * 2048, sceneArchiveFile.bytes), sceneEntries = await json(2, sceneArchive);
    toyIndex = sceneEntries.findIndex(entry => memberNamed(entry, `${environment}.toy`));
    if (toyIndex < 0) throw new Error('Original terrain materials are missing.');
    toy = await codec(3, sceneArchive, toyIndex);
  }
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
  const riderArchive = await archiveAt(model.rider), motions = await archiveAt('/ANIMSRDR.RES');
  const riderMesh = await json(27, await member(riderArchive, '/' + model.rider_mesh));
  const riding = await json(51, await member(riderArchive, '/' + model.rider_catalog));
  for (let index = 0; index < riding.length; index++) {
    if (token !== generation) throw new Error('Opening cancelled.');
    await codec(52, await member(motions, '/' + riding[index].path.split('/').at(-1)), index);
  }
  const pose = await codec(53, new Uint8Array()).then(bytes => JSON.parse(decoder.decode(bytes)));
  const riderMaterials = await archiveAt('/RIDER/RIDER_MT.RES');
  const riderTextures = await baseTextures(riderMesh, riderMaterials.bytes, riderMaterials.entries);
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
      const textures = await baseTextures(model, resource, entries, sceneTextures);
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
  const wheelEffects = await loadWheelEffects({ resource, entries, quadArchive, quadEntries, codec, json, scene: decoder.decode(await codec(3, resource, sceneIndex)), mode });
  const train = environment === 'env08' ? await loadTrain({ resource, entries, modeIndex, codec, json, baseTextures, isCurrent: () => token === generation }) : null;
  if (train && mode) await audio.loadSpatial({ toy: new TextEncoder().encode(mode), isCurrent: () => token === generation });
  return { bytes, environment, name: entries[index].name, scene, cameraPresets, mesh, meshBytes, physics, chassis, actorRays, collisions: { meshes: collisionMeshes, placements: collisionPlacements }, water, train, wheelEffects, toy, textures, sky, terrainTextures: { layout, images, detail }, ecosystem, world: { metadata: sceneMetadata, meshes: sceneMeshes, textures: sceneTextures, unsupported }, rider: { mesh: riderMesh, pose, textures: riderTextures } };
}
async function baseTextures(mesh, materialArchive, materialEntries, cache = new Map()) {
  const findMaterial = name => materialEntries.findIndex(entry => entry.name.split(/[\\/]/).at(-1).toLowerCase() === name.toLowerCase());
  const textures = new Map();
  for (const material of new Set(mesh.lods[0].groups.map(group => mesh.materials[group.material]))) {
    if (cache.has(material)) { textures.set(material, cache.get(material)); continue; }
    const materialIndex = findMaterial(`${material}.psm`);
    const materialText = materialIndex < 0 ? '' : decoder.decode(await codec(3, materialArchive, materialIndex));
    //001bf430 ->001b5080 binds the same texture stem with coordinate set0
    // when no authored PSM exists. Dynamic runtime replacements are separate.
    const binding = materialIndex < 0 ? material : textureBinding(materialText);
    if (!binding) continue;
    const shader = materialText.split(/\r?\n/).map(line => line.split(';')[0].trim()).find(line => /^microProgramName\s*=/i.test(line))?.split('=')[1].trim();
    if (/refl|envmap/i.test(shader ?? '')) { const unsupported = { shader }; textures.set(material, unsupported); cache.set(material, unsupported); continue; }
    const textureIndex = findMaterial(`${binding}.pst`);
    const pass = materialIndex < 0 ? null : await json(90, new TextEncoder().encode(materialText));
    const atmIndex = pass?.texture_animation ? findMaterial(`${binding}.atm`) : -1;
    if (textureIndex < 0 && atmIndex < 0) { if (materialIndex < 0) continue; throw new Error(`Original material texture is missing: ${binding}`); }
    const pst = atmIndex >= 0 ? await codec(3, materialArchive, atmIndex) : await codec(3, materialArchive, textureIndex);
    let image;
    try {
      const info = await json(atmIndex >= 0 ? 56 : 7, pst);
      const animationFrames = atmIndex >= 0 ? await Promise.all(Array.from({length:info.frames},(_,index)=>codec(57,pst,index))) : null;
      const rgba = animationFrames ? animationFrames[0] : await codec(8, pst);
      const alphaMatch = materialText.match(/alphaReferenceValue\s*=\s*(\d+)/i);
      // Explicit GEQUAL cutout materials, including the original tower lattice.
      // FB_ONLY/source-alpha edge blending is not yet a complete GS match.
      const sceneCutout = /^\s*alphaTestOnOff\s*=\s*1\s*$/mi.test(materialText)
        && /^\s*alphaTestMethod\s*=\s*5\s*$/mi.test(materialText)
        && /^\s*processingMethodWhenFailedInAlphaTest\s*=\s*[01]\s*$/mi.test(materialText);
      image = { ...info, rgba, animationFrames, shader, pass, materialText, alphaTest: /alphaTestOnOff\s*=\s*0/i.test(materialText) ? 0 : Number(alphaMatch?.[1] ?? 0) / 255, sceneAlphaFailure: sceneCutout ? Number(materialText.match(/processingMethodWhenFailedInAlphaTest\s*=\s*(\d+)/i)?.[1] ?? 0) : 0, sceneAlphaCutoff: sceneCutout ? Number(alphaMatch?.[1] ?? 0) / 255 : 0 };
    } catch (error) { image = { shader, error: error.message }; }
    textures.set(material, image); cache.set(material, image);
  }
  return textures;
}
function textureBinding(text) {
  let inBase = false, texture, coordinates = 0;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.split(';')[0].trim();
    if (line.startsWith('[')) { inBase = line.toLowerCase() === '[shaderpass_1]'; continue; }
    if (!inBase) continue;
    const split = line.indexOf('='); if (split < 0) continue;
    const key = line.slice(0, split).trim().toLowerCase(), value = line.slice(split + 1).trim();
    if (key === 'texturemap_1') texture = value;
    if (key === 'texturecoordinateindex_1') coordinates = Number(value);
  }
  return coordinates === 0 && texture ? texture : null;
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
uniform vec2 sceneryUvOffset;
out vec4 authoredColor;
uniform bool useEcosystem;
uniform vec2 ecoRotation;
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
void main() {
  texcoord = uv + sceneryUvOffset;
  authoredColor = vertexColor;
  mat4 localPose = useQuadPose ? quadBones[int(quadNode)] : mat4(1.0);
  vec4 worldPosition = model * localPose * vec4(position, 1.0);
  if (useEcosystem) {
    vec3 scaled = position * vec3(ecoScale.x, ecoScale.y, ecoScale.x);
    worldPosition = vec4(ecoPosition + vec3(ecoRotation.x * scaled.x - ecoRotation.y * scaled.z, scaled.y, ecoRotation.y * scaled.x + ecoRotation.x * scaled.z), 1.0);
  }
  worldXZ = worldPosition.xz;
  surfaceNormal = mat3(model) * mat3(localPose) * normal;
  worldHeight = worldPosition.y;
  distanceToEye = length(worldPosition.xyz - eye);
  gl_Position = viewProjection * worldPosition;
  if (skyPass) gl_Position = gl_Position.xyww;
}`;
const fragmentSource = `#version 300 es
precision highp float;
in vec3 surfaceNormal;
in float distanceToEye;
in float worldHeight;
in vec2 texcoord;
in vec2 worldXZ;
in vec4 authoredColor;
uniform bool useVertexColor;
uniform bool sceneryBlend;
uniform bool alphaFailPass;
uniform vec2 heights;
uniform float fogDistance;
uniform vec3 surfaceColor;
uniform sampler2D baseTexture;
uniform bool useTexture;
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
out vec4 color;
void main() {
  vec3 n = normalize(surfaceNormal);
  float altitude = clamp((worldHeight - heights.x) / max(1.0, heights.y - heights.x), 0.0, 1.0);
  vec3 base = mix(vec3(.29,.32,.22), vec3(.60,.54,.41), altitude);
  base = mix(base, vec3(.40,.38,.34), smoothstep(.14,.65,1.0-n.y));
  if (surfaceColor.x >= 0.0) base = surfaceColor;
  float sourceAlpha = 1.0;
  if (useTexture) {
    vec4 sampled = texture(baseTexture, texcoord);
    if (useVertexColor) sampled *= authoredColor / 128.0;
    if (alphaFailPass ? sampled.a >= alphaCutoff : sampled.a < alphaCutoff) discard;
    base = sampled.rgb;
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
  float fog = 1.0-exp(-pow(distanceToEye/fogDistance,2.0));
  color = vec4(skyPass ? base : mix(lit, fogColor, min(.94,fog)),sourceAlpha);
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
  uniforms = Object.fromEntries(['viewProjection', 'model', 'eye', 'heights', 'fogDistance', 'surfaceColor', 'baseTexture', 'useTexture', 'useTerrainTexture', 'terrainColors', 'terrainMapping', 'terrainDetail', 'terrainDetailScale', 'terrainSpacing', 'ambientLight', 'sunlightColor', 'sunlightDirection', 'fogColor', 'unlit', 'skyPass', 'useVertexColor', 'sceneryBlend', 'sceneryUvOffset', 'alphaFailPass'].map(name => [name, gl.getUniformLocation(program, name)]));
  uniforms.useEcosystem = gl.getUniformLocation(program, 'useEcosystem');
  uniforms.ecoRotation = gl.getUniformLocation(program, 'ecoRotation');
  uniforms.alphaCutoff = gl.getUniformLocation(program, 'alphaCutoff');
  uniforms.useQuadPose = gl.getUniformLocation(program, 'useQuadPose');
  uniforms.quadBones = gl.getUniformLocation(program, 'quadBones[0]');
  fallbackTexture = texture({ width: 1, height: 1, rgba: new Uint8Array([255, 255, 255, 255]) });
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
  for (const model of remoteModels.values()) { disposeRemoteGpu(model); }
  wheelEffectsRenderer?.dispose(); wheelEffectsRenderer = undefined;
  trainRenderer?.dispose(); trainRenderer = undefined;
  waterRenderer?.dispose(); waterRenderer = undefined;
  for (const chunk of [...chunks, ...vehicleChunks, ...sceneChunks, ...skyChunks]) { gl.deleteVertexArray(chunk.vao); gl.deleteBuffer(chunk.vertices); gl.deleteBuffer(chunk.indices); }
  for (const texture of gpuTextures) gl.deleteTexture(texture);
  for (const group of ecoGroups) { gl.deleteBuffer(group.instances); for (const chunk of group.chunks) { gl.deleteVertexArray(chunk.vao); gl.deleteBuffer(chunk.vertices); gl.deleteBuffer(chunk.indices); } }
  ecoGroups = [];
  gpuTextures = [];
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
  diagnostics.terrainTextures = images.length;
  diagnostics.terrainTextureResolution = [256, 256];
  diagnostics.terrainDetailTexture = [detail.width, detail.height];
  diagnostics.terrainLighting = 'original base/detail blend; projected pass selection pending';
}
function upload(vertices, indices, hasUv = false, hasNodes = false, hasColors = false) {
  const chunk = { vao: gl.createVertexArray(), vertices: gl.createBuffer(), indices: gl.createBuffer(), count: indices.length };
  gl.bindVertexArray(chunk.vao); gl.bindBuffer(gl.ARRAY_BUFFER, chunk.vertices); gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
  const stride = hasColors ? 48 : hasNodes ? 36 : hasUv ? 32 : 24;
  gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, stride, 0);
  gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 3, gl.FLOAT, false, stride, 12);
  if (hasUv) { gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2, 2, gl.FLOAT, false, stride, 24); }
  if (hasNodes) { gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3, 1, gl.FLOAT, false, stride, 32); }
  if (hasColors) { gl.enableVertexAttribArray(6); gl.vertexAttribPointer(6, 4, gl.FLOAT, false, stride, 32); }
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, chunk.indices); gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW);
  return chunk;
}
function terrainHeight(x, z) {
  const { width, depth, spacing, heights } = terrain;
  const gx = Math.max(0, Math.min(width - 1.00001, x / spacing)), gz = Math.max(0, Math.min(depth - 1.00001, z / spacing));
  const ix = Math.floor(gx), iz = Math.floor(gz), fx = gx - ix, fz = gz - iz;
  const a = heights[iz * width + ix], b = heights[iz * width + ix + 1], c = heights[(iz + 1) * width + ix], d = heights[(iz + 1) * width + ix + 1];
  return fx + fz <= 1 ? a + fx * (b - a) + fz * (c - a) : d + (1 - fx) * (c - d) + (1 - fz) * (b - d);
}
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
  vehicle = { groups, nodes: source.mesh.nodes, nodeMatrices: new Float32Array(source.mesh.nodes.flatMap(node => node.world_matrix)), position, yaw: source.scene.yaw, length: max[2] - min[2], height: max[1] - min[1], materials: source.mesh.materials, textures: source.textures, rider: source.rider };
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
      const chunk = upload(vertices,new Uint16Array(group.indices),true,true); chunk.rider=rider; chunk.texture=gpu.get(mesh.materials[group.material]); model.chunks.push(chunk);
    }
  }
}
async function remoteAssets(quad, rider) {
  const key = `${quad}:${rider}`;
  if (remoteModels.has(key)) return remoteModels.get(key).pending;
  const model = {source:null,chunks:null,gpuTextures:[]}; remoteModels.set(key,model);
  model.pending = (async () => {
    const content = await (hostedContent ??= openHostedContent());
    const selected = await json(89,new Uint8Array([rider]),quad);
    const archive = async path => { const f=content.files.find(f=>f.path===path); if (!f) throw new Error(`Player resource unavailable: ${path}`); const bytes=await read(content.file,f.lba*2048,f.bytes); return {bytes,entries:await json(2,bytes)}; };
    const member = async (a,name) => { const index=a.entries.findIndex(e=>e.name.replaceAll('\\','/').split('/').at(-1).toLowerCase()===name.toLowerCase()); if(index<0) throw new Error('Remote player mesh missing.'); return codec(3,a.bytes,index); };
    const quadArchive=await archive(selected.quad), riderArchive=await archive(selected.rider);
    const mesh=await json(11,await member(quadArchive,selected.quad_mesh));
    const riderMesh=await json(11,await member(riderArchive,selected.rider_mesh));
    const quadMaterials=await archive(selected.quad_materials), riderMaterials=await archive('/RIDER/RIDER_MT.RES');
    model.source={mesh,riderMesh,textures:await baseTextures(mesh,quadMaterials.bytes,quadMaterials.entries),riderTextures:await baseTextures(riderMesh,riderMaterials.bytes,riderMaterials.entries)};
    return model;
  })().catch(error=> {if(remoteModels.get(key)===model)remoteModels.delete(key);throw error;});
  return model.pending;
}
function drawRemotePlayers() {
  let draws=0;
  for (const peer of multiplayer?.remote ?? []) {
    const model=remoteModels.get(`${peer.atv_model}:${peer.rider_model}`); if (!model?.source) continue;
    if (peer.quad_node_matrices.length!==model.source.mesh.nodes.length || peer.rider_node_matrices.length!==model.source.riderMesh.nodes.length) continue;
    buildRemoteGpu(model);
    const quad=new Float32Array(peer.quad_node_matrices.flat()), rider=new Float32Array(peer.rider_node_matrices.flat());
    const quadWorld=new Float32Array(peer.world_matrix), riderWorld=new Float32Array(peer.rider_world_matrix);
    gl.uniform1i(uniforms.useQuadPose,true);
    for (const chunk of model.chunks ?? []) {
      gl.uniformMatrix4fv(uniforms.model,false,chunk.rider?riderWorld:quadWorld);
      gl.uniformMatrix4fv(uniforms.quadBones,false,chunk.rider?rider:quad);
      gl.uniform1i(uniforms.useTexture,Boolean(chunk.texture));gl.bindTexture(gl.TEXTURE_2D,chunk.texture??fallbackTexture);
      gl.bindVertexArray(chunk.vao);gl.drawElements(gl.TRIANGLES,chunk.count,gl.UNSIGNED_SHORT,0);draws++;
    }
  }
  diagnostics.multiplayerDraws=draws;
}
function buildEcosystem() {
  const eco = terrain.ecosystem;
  diagnostics.vegetationPlacements = eco?.placements.length ?? 0;
  if (!eco) return;
  const scale = Math.max(terrain.sizeX, terrain.sizeZ) / 65536, textures = new Map();
  for (const [name, decoded] of eco.textures) {
    if (!decoded.rgba) continue;
    const gpu = texture(decoded); textures.set(name, gpu); gpuTextures.push(gpu);
  }
  for (const species of eco.species) {
    const { mesh, bounds } = eco.models.get(species.slot);
    if (!(bounds.half_extents[0] > 0 && bounds.half_extents[1] > 0)) throw new Error('Invalid vegetation mesh bounds.');
    const placements = eco.placements.filter(p => p.slot === species.slot).map(p => {
      const position = p.coordinates.map(c => c * scale);
      // 00129b70 quantizes an explicitly resampled height back to u16.
      if (p.resample_height) position[1] = (Math.trunc(terrainHeight(position[0], position[2]) / scale) & 65535) * scale;
      return { position, scale: [p.radius / bounds.half_extents[0], p.height * .5 / bounds.half_extents[1]], radius: Math.max(p.radius, p.height) };
    });
    const group = { placements, chunks: [], instances: gl.createBuffer(), visible: new Float32Array(Math.min(1500, placements.length) * 5), count: 0 };
    gl.bindBuffer(gl.ARRAY_BUFFER, group.instances); gl.bufferData(gl.ARRAY_BUFFER, group.visible.byteLength, gl.DYNAMIC_DRAW);
    for (const source of mesh.lods[0].groups) {
      if (source.vertices.length > 65536) throw new Error('Vegetation mesh exceeds index bounds.');
      const vertices = new Float32Array(source.vertices.length * 8);
      source.vertices.forEach((v, i) => vertices.set([...v.position, ...v.normal, ...v.uv], i * 8));
      const chunk = upload(vertices, new Uint16Array(source.indices), true);
      const material = mesh.materials[source.material], decoded = eco.textures.get(material);
      chunk.texture = textures.get(material);
        chunk.alphaCutoff = decoded?.alphaTest ?? 0;
      gl.bindVertexArray(chunk.vao); gl.bindBuffer(gl.ARRAY_BUFFER, group.instances);
      gl.enableVertexAttribArray(4); gl.vertexAttribPointer(4, 3, gl.FLOAT, false, 20, 0); gl.vertexAttribDivisor(4, 1);
      gl.enableVertexAttribArray(5); gl.vertexAttribPointer(5, 2, gl.FLOAT, false, 20, 12); gl.vertexAttribDivisor(5, 1);
      group.chunks.push(chunk);
    }
    ecoGroups.push(group);
  }
}
function drawEcosystem() {
  diagnostics.vegetationTriangles = 0;
  diagnostics.vegetationDraws = 0;
  if (!ecoGroups.length) return;
  // Original placement, dimensions and model normalization; browser visibility
  // selection is provisional. Batch repeated original meshes, not thousands of draws.
  const forwardX = Math.sin(camera.yaw), forwardZ = Math.cos(camera.yaw), candidates = [];
  for (const group of ecoGroups) {
    group.count = 0;
    for (const p of group.placements) {
      const dx = p.position[0] - camera.x, dz = p.position[2] - camera.z, distance = dx * dx + dz * dz;
      if (distance < 1800 * 1800 && dx * forwardX + dz * forwardZ > -p.radius) candidates.push({ group, p, distance });
    }
  }
  candidates.sort((a, b) => a.distance - b.distance);
  for (const { group, p } of candidates.slice(0, 1500)) group.visible.set([...p.position, ...p.scale], group.count++ * 5);
  gl.uniform1i(uniforms.useEcosystem, true); gl.uniform1i(uniforms.unlit, true); gl.uniform1i(uniforms.useQuadPose, false);
  gl.uniformMatrix4fv(uniforms.model, false, identity);
  // 002fe710 rotates the vegetation models about Y using the view basis.
  gl.uniform2f(uniforms.ecoRotation, -Math.cos(camera.yaw), Math.sin(camera.yaw));
  let draws = 0;
  for (const group of ecoGroups) {
    if (!group.count) continue;
    gl.bindBuffer(gl.ARRAY_BUFFER, group.instances); gl.bufferSubData(gl.ARRAY_BUFFER, 0, group.visible.subarray(0, group.count * 5));
    for (const chunk of group.chunks) {
      gl.uniform1f(uniforms.alphaCutoff, chunk.alphaCutoff); gl.uniform1i(uniforms.useTexture, Boolean(chunk.texture));
      gl.bindTexture(gl.TEXTURE_2D, chunk.texture ?? fallbackTexture); gl.bindVertexArray(chunk.vao);
      gl.drawElementsInstanced(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0, group.count); draws++;
      diagnostics.vegetationTriangles += chunk.count / 3 * group.count;
    }
  }
  diagnostics.vegetationVisible = Math.min(candidates.length, 1500); diagnostics.vegetationDraws = draws;
  gl.uniform1i(uniforms.useEcosystem, false); gl.uniform1f(uniforms.alphaCutoff, 0);
}
function buildTerrainMesh() {
  for (const chunk of chunks) { gl.deleteVertexArray(chunk.vao); gl.deleteBuffer(chunk.vertices); gl.deleteBuffer(chunk.indices); }
  chunks = [];
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
  let vertexCount = 0, triangleCount = 0;
  function grid(gridX, gridZ, fine = false) {
    const nx = gridX.length, nz = gridZ.length;
    const vertices = new Float32Array(nx * nz * 6), indices = [];
    let offset = 0;
    for (let zi = 0; zi < nz; zi++) for (let xi = 0; xi < nx; xi++) {
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
    for (let z = 0; z < nz - 1; z++) for (let x = 0; x < nx - 1; x++) {
      if (!fine && patch && gridX[x] >= patch.x0 && gridX[x + 1] <= patch.x1 && gridZ[z] >= patch.z0 && gridZ[z + 1] <= patch.z1) continue;
      const a = z * nx + x, b = a + 1, c = a + nx, d = c + 1;
      indices.push(a, c, b, b, c, d);
    }
    chunks.push(upload(vertices, new Uint16Array(indices))); vertexCount += vertices.length / 6; triangleCount += indices.length / 3;
  }
  for (let z0 = 0; z0 < zs.length - 1; z0 += 64) for (let x0 = 0; x0 < xs.length - 1; x0 += 64) grid(xs.slice(x0, x0 + 65), zs.slice(z0, z0 + 65));
  if (patch) {
    const fineX = Array.from({ length: patch.x1 - patch.x0 + 1 }, (_, i) => patch.x0 + i), fineZ = Array.from({ length: patch.z1 - patch.z0 + 1 }, (_, i) => patch.z0 + i);
    for (let z0 = 0; z0 < fineZ.length - 1; z0 += 64) for (let x0 = 0; x0 < fineX.length - 1; x0 += 64) grid(fineX.slice(x0,x0+65),fineZ.slice(z0,z0+65),true);
  }
  gl.bindVertexArray(null);
  Object.assign(diagnostics, { vertices: vertexCount, triangles: triangleCount, chunks: chunks.length, finePatch: patch, terrainGrid: [width,depth], terrainSampleSpacing: spacing, terrainFarStep: step, terrainLod: 'full stored samples nearby; provisional far grid', terrainDetailCenter: [centerX,centerZ], terrainDetailUpdates: (diagnostics.terrainDetailUpdates ?? 0)+1 });
}
function buildMesh() {
  clearMesh(); buildTerrainMesh(); buildEcosystem();
  if (terrain.wheelEffectsSource) { wheelEffectsRenderer = new WheelEffectsRenderer(gl, terrain.wheelEffectsSource); wheelEffectsRenderer.accept(simFrame?.wheel_particles); }
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
    chunk.quad = true;
    chunk.texture = materialTextures.get(vehicle.materials[group.material]);
    vehicleChunks.push(chunk);
  }
  let riderTriangles = 0, riderTextureCount = 0;
  if (vehicle?.rider) {
    const rider = vehicle.rider, riderTextures = new Map();
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
      chunk.rider = true;
      chunk.texture = riderTextures.get(rider.mesh.materials[group.material]);
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
    for (const [name, decoded] of world.textures) { if (!decoded.rgba) continue; const gpu = texture(decoded); textures.set(name, gpu); gpuTextures.push(gpu); }
    for (const [name, mesh] of world.meshes) {
      const groups = [];
      for (const group of mesh.lods[0].groups) {
        const material = mesh.materials[group.material], decoded = world.textures.get(material);
        // Environment/reflection shaders need their original coordinate generation.
        // Keep those groups absent until recovered rather than drawing a false UV map.
        if (/refl|envmap/i.test(decoded?.shader ?? '')) { skippedSceneGroups++; continue; }
        const prelit = decoded?.pass?.prelit ?? false;
        const vertices = new Float32Array(group.vertices.length * (prelit ? 12 : 8));
        group.vertices.forEach((vertex, index) => vertices.set([...vertex.position, ...vertex.normal, ...vertex.uv, ...(prelit ? vertex.color ?? [128,128,128,128] : [])], index * (prelit ? 12 : 8)));
        const chunk = upload(vertices, new Uint16Array(group.indices), true, false, prelit);
        chunk.texture = textures.get(material); chunk.alphaCutoff = decoded?.sceneAlphaCutoff ?? 0;
        chunk.animationFrames = decoded?.animationFrames?.map(rgba => { const gpu = texture({...decoded,rgba}); gpuTextures.push(gpu); return gpu; });
        chunk.prelit = prelit; chunk.material = material; chunk.blend = decoded?.pass?.blend ?? false; chunk.alphaFailure = decoded?.sceneAlphaFailure ?? 0;
        chunk.depthWrite = decoded?.pass?.depth_write ?? true;
        groups.push(chunk); sceneChunks.push(chunk);
      }
      geometry.set(name, groups);
    }
    for (const model of world.metadata.models) {
      const groups = geometry.get(model.name);
      if (!groups?.length) continue;
      sceneInstances++;
      for (const chunk of groups) { sceneDraws.push({ chunk, matrix: new Float32Array(model.world_matrix), unlit: chunk.prelit || !model.use_lighting }); sceneTriangles += chunk.count / 3; }
    }
  }
  gl.bindVertexArray(null);
  diagnostics.skyTiles = skyChunks.length;
  diagnostics.sceneryPrelitGroups = sceneChunks.filter(chunk => chunk.prelit).length;
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
function vehicleMatrix() {
  const [w, x, y, z] = simFrame?.orientation ?? [Math.cos(vehicle.yaw / 2), 0, Math.sin(vehicle.yaw / 2), 0];
  const position = simFrame?.origin ?? vehicle.position;
  return new Float32Array([
    1 - 2 * (y * y + z * z), 2 * (x * y + w * z), 2 * (x * z - w * y), 0,
    2 * (x * y - w * z), 1 - 2 * (x * x + z * z), 2 * (y * z + w * x), 0,
    2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y), 0,
    ...position, 1,
  ]);
}
function matrix() {
  if (cameraFrame) presentCameraFrame();
  const { x, y, z, yaw, pitch } = camera;
  const forward = [Math.sin(yaw) * Math.cos(pitch), Math.sin(pitch), Math.cos(yaw) * Math.cos(pitch)];
  const right = [-Math.cos(yaw), 0, Math.sin(yaw)];
  const up = [-Math.sin(yaw) * Math.sin(pitch), Math.cos(pitch), -Math.cos(yaw) * Math.sin(pitch)];
  const view = new Float32Array([right[0], up[0], -forward[0], 0, right[1], up[1], -forward[1], 0, right[2], up[2], -forward[2], 0, -(right[0] * x + right[2] * z), -(up[0] * x + up[1] * y + up[2] * z), forward[0] * x + forward[1] * y + forward[2] * z, 1]);
  // Original00325e80 uses horizontal FOV. Preserve the4:3 vertical framing
  // while extending the view horizontally on widescreen displays.
  const near = 1, far = Math.max(terrain.sizeX, terrain.sizeZ) * 5;
  const f = cameraFrame ? diagnostics.cameraPresentation.focalLength : 1 / Math.tan(Math.PI / 6);
  const projection = new Float32Array([f / (canvas.width / canvas.height), 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) / (near - far), -1, 0, 0, 2 * far * near / (near - far), 0]);
  const result = new Float32Array(16);
  for (let column = 0; column < 4; column++) for (let row = 0; row < 4; row++) for (let k = 0; k < 4; k++) result[column * 4 + row] += projection[k * 4 + row] * view[column * 4 + k];
  return result;
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
  input.throttle = Math.max(0, clamp(input.throttle)); input.brake = Math.max(0, clamp(input.brake));
  // Browser axes point right; the recovered steering scalar points left.
  input.steer = -clamp(input.steer); // Rust clamps the combined physical lean; one-handed touch also supplies lean.
  return { input, actions };
}
function acceptFrame(frame) {
  wheelEffectsRenderer?.accept(frame.wheel_particles);
  if (diagnostics.wheelEffects) { diagnostics.wheelEffects.frame = frame.wheel_particles; diagnostics.wheelEffects.particles = frame.wheel_particles?.particles.length ?? 0; }
  if (!frame?.origin?.every(Number.isFinite) || frame.origin.length !== 3 || !frame.orientation?.every(Number.isFinite) || frame.orientation.length !== 4 || !Number.isFinite(frame.speed) || !Number.isFinite(frame.rpm)) throw new Error('Simulation returned a nonfinite pose.');
  simFrame = frame;
  // A completed old batch may arrive after pause/blur/cancel. Keep its physical
  // frame, but do not overwrite cleared controls with the old touch generation.
  if (frame.applied_input && frame.applied_input.input_generation === gameplayBindings.generation + touchControls.generation) diagnostics.input = frame.applied_input;
  touchControls.feedback(frame);
  vehicle.position = frame.origin.slice();
  if (frame.quad_node_matrices?.length !== vehicle.nodes.length || frame.quad_node_matrices.some(matrix => matrix.length !== 16 || !matrix.every(Number.isFinite))) throw new Error('Simulation returned an invalid quad linkage pose.');
  vehicle.nodeMatrices = new Float32Array(frame.quad_node_matrices.flat());
  diagnostics.animatedQuadNodes = frame.quad_node_matrices.length;
  if (vehicle.rider) {
    if (frame.rider_node_matrices?.length !== vehicle.rider.mesh.nodes.length || frame.rider_node_matrices.some(matrix => matrix.length !== 16 || !matrix.every(Number.isFinite))) throw new Error('Simulation returned an invalid rider pose.');
    vehicle.rider.pose.world_matrices = frame.rider_node_matrices;
    diagnostics.animatedRiderNodes = frame.rider_node_matrices.length;
    diagnostics.riderMode = frame.rider_mode;
    if (frame.rider_world_matrix && (frame.rider_world_matrix.length !== 16 || !frame.rider_world_matrix.every(Number.isFinite))) throw new Error('Simulation returned an invalid detached rider transform.');
    diagnostics.riderWreck = frame.rider_wreck ?? null;
  }
}
async function startSimulation(source) {
  diagnostics.simulationSteps = 0;
  simReady = false; simFrame = undefined; cameraFrame = undefined; simAccumulator = 0; simGeneration++;
  if (!source.physics) return;
  await codec(20, source.bytes); await codec(21, source.physics); await codec(22, source.meshBytes); await codec(26, source.toy);
  await codec(65, source.ecosystem?.bytes ?? new Uint8Array());
  diagnostics.water = source.water ? await json(64, source.water.resource, source.water.index) : null;
  terrain.waterSource = source.water ? await json(66, source.water.resource, source.water.index) : null;
  waterRenderer?.dispose();
  waterRenderer = terrain.waterSource ? new WaterRenderer(gl, terrain.waterSource) : undefined;
  diagnostics.waterRender = waterRenderer ? { channels: waterRenderer.channelCount, gpuBytes: waterRenderer.gpuBytes, draws: 0, scope: terrain.waterSource.scope } : null;
  diagnostics.chassis = await json(44, source.chassis);
  const actor = await json(48, source.actorRays, 0), collisionIds = new Map();
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
  const origin=source.scene.position.slice();
  if (multiplayer?.state.peer != null) { const slot=multiplayer.state.peer; origin[0]+=Math.cos(source.scene.yaw)*slot*8; origin[2]-=Math.sin(source.scene.yaw)*slot*8; }
  simStart = { origin, yaw: source.scene.yaw };
  acceptFrame(await command(23, simStart));
  diagnostics.cameraPresets = (await json(45, source.cameraPresets)).map(preset => preset.name);
  orbit = 0; acceptCameraFrame(await command(46, { dt: 1 / 60, orbit }));
  if (source.train) {
    source.train.frame = await json(67, source.train.resource, source.train.modeIndex);
    diagnostics.trainCollision = await command(69, {});
    trainRenderer?.accept(source.train.frame);
    diagnostics.train = { time: source.train.frame.time, duration: source.train.frame.duration, cars: source.train.frame.cars.filter(car => car.use_collision).length, wheelNodes: source.train.frame.wheel_matrices?.length ?? 0, frame: source.train.frame };
  } else diagnostics.train = null;
  const uvSources = Object.fromEntries([...source.world.textures].filter(([, texture]) => texture.pass?.uv_scroll || texture.pass?.texture_animation).map(([name, texture]) => [name, {source:texture.materialText,frames:texture.animationFrames?.length}]));
  const uvState = await command(91, uvSources);
  diagnostics.sceneryUv = Object.fromEntries(uvState.map(([name]) => [name, [0,0]]));
  simReady = true;
}
async function pumpSimulation() {
  if (simBusy || !simReady || !focused || document.hidden || lost || menuOpen) return;
  const count = Math.min(4, Math.floor(simAccumulator * 60));
  if (count < 1) return;
  simBusy = true;
  const token = simGeneration;
  try {
    const { input, actions } = drivingInput();
    // Menu/reset transitions cancel input before any further physics is queued.
    if (actions.pause) { void frontend?.show('pause'); return; }
    if (actions.reset) { await $('reset').onclick(); return; }
    for (let step = 0; step < count; step++) simAccumulator -= 1 / 60;
    const cycle = cameraCycle || actions.camera; cameraCycle = false;
    if (cycle) orbit = 0;
    const submitted = performance.now();
    const frames = await command(50, { count, input, orbit, cycle });
    diagnostics.physicsRoundTripMs = performance.now() - submitted;
    diagnostics.physicsBatchSteps = count;
    if (token !== simGeneration) return;
    for (const { frame, camera: cameraResult, water, train, scenery_uv, scenery_textures } of frames) {
      acceptFrame(frame);
      diagnostics.sceneryUv = scenery_uv; diagnostics.sceneryTextureFrames = scenery_textures;
      if (trainRenderer && train) { trainRenderer.accept(train); diagnostics.train.time = train.time; diagnostics.train.frame = train; }
      if (waterRenderer && water) { waterRenderer.accept(water); terrain.waterSource.frame = water; diagnostics.waterRender.baseHeight = water.base_height; }
      diagnostics.simulationSteps++;
      acceptCameraFrame(cameraResult);
      void audio.update(1 / 60, frame.applied_input ?? input, frame, { train, camera: cameraResult });
    }
  } catch (error) {
    if (token === simGeneration) { simReady = false; status(`Simulation stopped: ${error.message}`, true); }
  } finally { simBusy = false; }
}
function acceptCameraFrame(frame) {
  if (frame?.position?.length !== 3 || frame?.target?.length !== 3 || ![...frame.position, ...frame.target, frame.fov].every(Number.isFinite)) throw new Error('Simulation returned an invalid camera pose.');
  cameraFrame = frame;
  presentCameraFrame();
  diagnostics.followCamera = { ...frame, presetName: diagnostics.cameraPresets?.[frame.preset] };
}
function presentCameraFrame() {
  const frame = cameraFrame;
  // Keep the recovered landscape camera intact. A narrow viewport needs extra
  // distance and a higher sightline so the quad does not hide the next jump.
  const aspect = canvas.width / canvas.height;
  const portrait = Math.max(0, Math.min(1, (1 - aspect) / .5));
  const distanceScale = 1 + .35 * portrait;
  [camera.x, camera.y, camera.z] = frame.position;
  const offset = frame.target.map((value, i) => value - frame.position[i]);
  camera.x -= offset[0] * (distanceScale - 1);
  camera.y -= offset[1] * (distanceScale - 1);
  camera.z -= offset[2] * (distanceScale - 1);
  camera.yaw = Math.atan2(offset[0], offset[2]);
  camera.pitch = Math.atan2(offset[1], Math.hypot(offset[0], offset[2])) + .12 * portrait;
  const focalLength = (4 / 3) / Math.tan(frame.fov * Math.PI / 360) * (1 - .12 * portrait);
  diagnostics.cameraPresentation = { aspect, portrait, distanceScale, pitchLift: .12 * portrait, focalLength };
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
  if (simReady) { followCamera(dt); return; }
  if (!focused || document.hidden) return;
  const pad = gamepad();
  let sideways = Number(keys.has('KeyD') || keys.has('ArrowRight')) - Number(keys.has('KeyA') || keys.has('ArrowLeft')) + deadzone(pad?.axes[0]);
  let forward = Number(keys.has('KeyW') || keys.has('ArrowUp')) - Number(keys.has('KeyS') || keys.has('ArrowDown')) - deadzone(pad?.axes[1]);
  let vertical = Number(keys.has('KeyE')) - Number(keys.has('KeyQ')) + (pad?.buttons[7]?.value ?? 0) - (pad?.buttons[6]?.value ?? 0);
  camera.yaw -= deadzone(pad?.axes[2]) * dt * 1.7;
  camera.pitch -= deadzone(pad?.axes[3]) * dt * 1.3;
  for (const pointer of pointers.values()) if (pointer.move) { sideways += pointer.dx / 70; forward -= pointer.dy / 70; }
  camera.pitch = Math.max(-1.45, Math.min(1.45, camera.pitch));
  const aboveGround = Math.max(0, camera.y - terrainHeight(camera.x, camera.z));
  const speed = Math.max(vehicle ? 12 : 30, Math.min(Math.max(terrain.sizeX, terrain.sizeZ) * .09, aboveGround * .6)) * (keys.has('ShiftLeft') ? 3 : 1) * dt;
  const length = Math.max(1, Math.hypot(sideways, forward, vertical));
  sideways /= length; forward /= length; vertical /= length;
  camera.x += (Math.sin(camera.yaw) * forward - Math.cos(camera.yaw) * sideways) * speed;
  camera.z += (Math.cos(camera.yaw) * forward + Math.sin(camera.yaw) * sideways) * speed;
  camera.y += (Math.sin(camera.pitch) * forward + vertical) * speed;
  const span = Math.max(terrain.sizeX, terrain.sizeZ);
  camera.x = Math.max(-span, Math.min(terrain.sizeX + span, camera.x));
  camera.z = Math.max(-span, Math.min(terrain.sizeZ + span, camera.z));
  camera.y = Math.max(terrain.min - span * .2, Math.min(terrain.max + span * 2, camera.y));
}
function render(time) {
  const started = performance.now();
  frame = requestAnimationFrame(render);
  touchControls.refresh();
  hud?.draw(simFrame?.hud, simReady && !lost && !document.hidden && !menuOpen);
  if (lost || !gl || !terrain || document.hidden || menuOpen) { lastTime = time; stats.sample(time, 0, false); return; }
  const dt = Math.min(.05, Math.max(0, (time - (lastTime || time)) / 1000)); lastTime = time;
  const ratio = Math.min(devicePixelRatio || 1, 1.5), width = Math.max(1, Math.round(innerWidth * ratio)), height = Math.max(1, Math.round(innerHeight * ratio));
  if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
  if (!menuOpen) updateCamera(dt);
  if (simReady && focused && !menuOpen) {
    simAccumulator = Math.min(.1, simAccumulator + dt); void pumpSimulation();
    const center = diagnostics.terrainDetailCenter;
    const x = Math.max(0,Math.min(terrain.width-1,vehicle.position[0]/terrain.spacing)), z = Math.max(0,Math.min(terrain.depth-1,vehicle.position[2]/terrain.spacing));
    if (center && (Math.abs(x-center[0]) > 48 || Math.abs(z-center[1]) > 48)) buildTerrainMesh();
  }
  gl.viewport(0, 0, width, height); gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT); gl.useProgram(program);
  gl.uniform1i(uniforms.useQuadPose, false); gl.uniform1i(uniforms.useEcosystem, false); gl.uniform1f(uniforms.alphaCutoff, 0);
  gl.uniform1i(uniforms.useVertexColor, false); gl.uniform1i(uniforms.sceneryBlend, false); gl.uniform1i(uniforms.alphaFailPass, false); gl.uniform2f(uniforms.sceneryUvOffset, 0, 0);
  const viewProjection = matrix();
  gl.uniformMatrix4fv(uniforms.viewProjection, false, viewProjection); gl.uniform3f(uniforms.eye, camera.x, camera.y, camera.z);
  gl.uniform2f(uniforms.heights, terrain.min, terrain.max); gl.uniform1f(uniforms.fogDistance, Math.max(terrain.sizeX, terrain.sizeZ) * .85);
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
    gl.activeTexture(gl.TEXTURE0); gl.depthMask(false); gl.disable(gl.DEPTH_TEST);
    for (const chunk of skyChunks) {
      gl.bindTexture(gl.TEXTURE_2D, chunk.texture); gl.bindVertexArray(chunk.vao);
      gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0);
    }
    gl.depthMask(true); gl.enable(gl.DEPTH_TEST);
  }
  gl.uniformMatrix4fv(uniforms.model, false, identity); gl.uniform1i(uniforms.skyPass, false); gl.uniform1i(uniforms.useTexture, false);
  gl.uniform1i(uniforms.useTerrainTexture, Boolean(terrain.textures));
  // The original single-player terrain VU program supplies constant RGB,
  // not a normal-based sun pass. Keep authored base colors free of our
  // provisional prop/vehicle lighting.
  gl.uniform1i(uniforms.unlit, Boolean(terrain.textures));
  for (const chunk of chunks) { gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0); }
  gl.uniform1i(uniforms.useTerrainTexture, false); gl.uniform1i(uniforms.unlit, false); gl.activeTexture(gl.TEXTURE0);
  gl.uniform3f(uniforms.surfaceColor, .68, .70, .73);
  for (const { chunk, matrix, unlit } of sceneDraws) {
    gl.uniformMatrix4fv(uniforms.model, false, matrix); gl.uniform1i(uniforms.unlit, unlit);
    gl.uniform1f(uniforms.alphaCutoff, chunk.alphaCutoff);
    gl.uniform1i(uniforms.useVertexColor, chunk.prelit); gl.uniform1i(uniforms.sceneryBlend, chunk.blend);
    gl.uniform2fv(uniforms.sceneryUvOffset, diagnostics.sceneryUv?.[chunk.material] ?? [0,0]);
    gl.depthMask(chunk.depthWrite);
    if (chunk.blend) { gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA); } else gl.disable(gl.BLEND);
    const sceneTexture = chunk.animationFrames?.[diagnostics.sceneryTextureFrames?.[chunk.material] ?? 0] ?? chunk.texture;
    gl.uniform1i(uniforms.useTexture, Boolean(sceneTexture)); gl.bindTexture(gl.TEXTURE_2D, sceneTexture ?? fallbackTexture);
    gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0);
    // GS FB_ONLY keeps failed fragments in the framebuffer without writing Z.
    // Split the mutually exclusive predicates so each source pixel blends once.
    if (chunk.alphaFailure === 1 && chunk.alphaCutoff > 0) {
      gl.uniform1i(uniforms.alphaFailPass, true); gl.depthMask(false);
      gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0);
      gl.uniform1i(uniforms.alphaFailPass, false);
    }
  }
  gl.uniform1f(uniforms.alphaCutoff, 0); gl.uniform1i(uniforms.useVertexColor, false); gl.uniform1i(uniforms.sceneryBlend, false);
  gl.uniform2f(uniforms.sceneryUvOffset, 0, 0); gl.depthMask(true); gl.disable(gl.BLEND);
  if (trainRenderer) { trainRenderer.draw(uniforms, fallbackTexture); diagnostics.train.draws = trainRenderer.draws; diagnostics.train.triangles = trainRenderer.triangles; }
  drawEcosystem();
  gl.uniform1i(uniforms.unlit, false);
  if (vehicle) gl.uniformMatrix4fv(uniforms.model, false, vehicleMatrix());
  if (vehicle) gl.uniformMatrix4fv(uniforms.quadBones, false, vehicle.nodeMatrices);
  for (const chunk of vehicleChunks) {
    gl.uniformMatrix4fv(uniforms.model, false, chunk.rider && simFrame?.rider_world_matrix ? new Float32Array(simFrame.rider_world_matrix) : vehicleMatrix());
    gl.uniform1i(uniforms.useQuadPose, Boolean(chunk.quad || chunk.rider));
    if (chunk.quad) gl.uniformMatrix4fv(uniforms.quadBones, false, vehicle.nodeMatrices);
    if (chunk.rider) gl.uniformMatrix4fv(uniforms.quadBones, false, new Float32Array(vehicle.rider.pose.world_matrices.flat()));
    gl.uniform1i(uniforms.useTexture, chunk.texture ? 1 : 0); gl.bindTexture(gl.TEXTURE_2D, chunk.texture ?? fallbackTexture);
    gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0);
  }
  drawRemotePlayers();
  void multiplayer?.sample();
  if (waterRenderer) {
    waterRenderer.draw({ camera, viewProjection, focalLength: cameraFrame ? diagnostics.cameraPresentation.focalLength : 1 / Math.tan(Math.PI / 6), aspect: canvas.width / canvas.height,
      farDistance: Math.max(terrain.sizeX, terrain.sizeZ) * 5, fogColor, fogDistance: Math.max(terrain.sizeX, terrain.sizeZ) * .85 });
    diagnostics.waterRender.draws = waterRenderer.draws;
    diagnostics.waterRender.gpuBytes = waterRenderer.gpuBytes;
    diagnostics.waterRender.cpuMilliseconds = waterRenderer.cpuMilliseconds;
    diagnostics.waterRender.gpuMilliseconds = waterRenderer.gpuMilliseconds;
  }
  if (wheelEffectsRenderer) {
    wheelEffectsRenderer.draw({ camera, viewProjection });
    diagnostics.wheelEffects.draws = wheelEffectsRenderer.draws; diagnostics.wheelEffects.triangles = wheelEffectsRenderer.triangles; diagnostics.wheelEffects.cpuMilliseconds = wheelEffectsRenderer.cpuMilliseconds; diagnostics.wheelEffects.wetLive = wheelEffectsRenderer.wetLive ?? 0;
  }
  diagnostics.frames++;
  if (stats.mode !== 'off') {
    diagnostics.drawCalls = chunks.length + sceneDraws.length + vehicleChunks.length + (terrain.sky ? skyChunks.length : 0) + diagnostics.vegetationDraws + (waterRenderer?.draws ?? 0) + (trainRenderer?.draws ?? 0) + (wheelEffectsRenderer?.draws ?? 0);
    diagnostics.drawTriangles = (chunks.reduce((sum, c) => sum + c.count, 0) + sceneDraws.reduce((sum, d) => sum + d.chunk.count, 0) + vehicleChunks.reduce((sum, c) => sum + c.count, 0) + (terrain.sky ? skyChunks.reduce((sum, c) => sum + c.count, 0) : 0)) / 3 + diagnostics.vegetationTriangles + (trainRenderer?.triangles ?? 0) + (wheelEffectsRenderer?.triangles ?? 0);
  }
  if (diagnostics.frames === 1 || diagnostics.frames % 120 === 0) {
    diagnostics.glError = gl.getError();
    if (diagnostics.glError) status(`Graphics error ${diagnostics.glError}.`, true);
  }
  stats.sample(time, performance.now() - started, focused);
}
async function loadGame(file = null, environment = "env01") {
  if (!gl || lost) throw new Error('Graphics context is unavailable; wait for recovery.');
  const token = ++generation; status('Opening terrain…'); diagnostics.ready = false;
  audio.close(); audio = new GameAudio(codec, message => { diagnostics.audioError = message; status(`Audio: ${message}`, true); });
  delete diagnostics.audioError; void audio.resume().catch(error => { diagnostics.audioError = error.message; });
  simReady = false; simGeneration++; simAccumulator = 0; simFrame = undefined; $('disc').disabled = true;
  try {
    let source;
    if (file) source = file.name.toLowerCase().endsWith('.iso') ? await discTerrain(file, token) : { bytes: await read(file, 0, file.size), name: file.name };
    else {
      const content = await (hostedContent ??= openHostedContent());
      if (token !== generation) return;
      source = await terrainFromContent(content.file, content.files, token, environment);
    }
    diagnostics.contentSource = file ? 'local-file' : 'hosted-original';
    const info = await json(9, source.bytes), raw = await codec(10, source.bytes);
    if (token !== generation) return;
    const width = info.width_cells + 1, depth = info.depth_cells + 1;
    if (!Number.isInteger(width) || !Number.isInteger(depth) || width < 2 || depth < 2 || !Number.isFinite(info.spacing) || info.spacing <= 0 || raw.byteLength !== width * depth * 4) throw new Error('Invalid terrain dimensions.');
    const heights = new Float32Array(width * depth), view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    let min = Infinity, max = -Infinity;
    for (let i = 0; i < heights.length; i++) { const value = view.getFloat32(i * 4, true); if (!Number.isFinite(value)) throw new Error('Nonfinite terrain height.'); heights[i] = value; min = Math.min(min, value); max = Math.max(max, value); }
    terrain = { width, depth, spacing: info.spacing, heights, min, max, sizeX: (width - 1) * info.spacing, sizeZ: (depth - 1) * info.spacing, textures: source.terrainTextures, world: source.world, sky: source.sky, ecosystem: source.ecosystem, trainSource: source.train, wheelEffectsSource: source.wheelEffects };
    diagnostics.terrainTextures = 0;
    uploadTerrainTextures();
    if (hud && source.physics) await codec(63, await hud.init(environment));
    setVehicle(source); buildMesh(); await startSimulation(source); resetCamera();
    Object.assign(diagnostics, { ready: true, source: source.name, width, depth, min, max, frames: 0 });
    $('reset').disabled = false; $('camera').disabled = !simReady; $('help').hidden = false;
    $('reset').textContent = simReady ? 'Reset ATV' : 'Reset view';
    $('help').textContent = simReady ? 'Pause → Controls: keyboard / controller bindings · left touch: steer / lean · right: drive / brake on ground; swipe, hold or circle in air' : 'Free camera · drag to look · WASD to move · Q/E up/down';
    status(simReady ? 'Physics reconstruction. Handling, collisions, animation and sound are incomplete.' : 'Static terrain. Open the disc to load the ATV.');
  } catch (error) { if (token === generation) status(error.message, true); throw error; }
  finally { if (token === generation) $('disc').disabled = false; }
}
$('disc').addEventListener('change', event => { const file = event.target.files[0]; if (file) void loadGame(file).catch(error => status(error.message, true)); });
$('reset').onclick = async () => {
  if (!simStart || !vehicle) { resetCamera(); return; }
  simGeneration++; simReady = false; simAccumulator = 0; clearInput();
  try { await audio.reset(); acceptFrame(await command(23, simStart)); await command(47, {}); orbit = 0; acceptCameraFrame(await command(46, { dt: 1 / 60, orbit })); simReady = true; buildMesh(); resetCamera(); }
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
  focused = true; canvas.focus({ preventScroll: true }); syncAudio(); canvas.setPointerCapture(event.pointerId);
  pointers.set(event.pointerId, { x: event.clientX, y: event.clientY, startX: event.clientX, startY: event.clientY, dx: 0, dy: 0, touch: event.pointerType === 'touch', move: event.pointerType === 'touch' && event.clientX < innerWidth / 2 });
});
canvas.addEventListener('pointermove', event => {
  const pointer = pointers.get(event.pointerId); if (!pointer) return;
  if (pointer.move) touchLean.move(event.pointerId, event.clientX, event.clientY);
  pointer.dx = Math.max(-70, Math.min(70, event.clientX - pointer.startX)); pointer.dy = Math.max(-70, Math.min(70, event.clientY - pointer.startY));
  if (!pointer.move) {
    if (simReady) { if (!pointer.touch) orbit -= (event.clientX - pointer.x) * .004; }
    else { camera.yaw -= (event.clientX - pointer.x) * .004; camera.pitch -= (event.clientY - pointer.y) * .004; }
  }
  pointer.x = event.clientX; pointer.y = event.clientY;
});
for (const name of ['pointerup', 'pointercancel', 'lostpointercapture']) canvas.addEventListener(name, event => { touchLean.end(event.pointerId, name !== 'pointerup'); pointers.delete(event.pointerId); });
window.addEventListener('keydown', event => {
  if (document.activeElement !== canvas) return;
  // A key held across blur/pause may resume with OS repeat events. It must be
  // released and pressed again after cancellation before becoming new input.
  if (event.repeat && !keys.has(event.code)) return;
  const bound = simReady ? gameplayBindings.usesKey(event.code) : ['KeyW', 'KeyA', 'KeyS', 'KeyD', 'KeyQ', 'KeyE', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ShiftLeft'].includes(event.code);
  if (bound) { event.preventDefault(); keys.add(event.code); if (simReady) gameplayBindings.observeKeys(keys, event.repeat ? null : event.code); }
});
window.addEventListener('keyup', event => { keys.delete(event.code); gameplayBindings.observeKeys(keys); });
const clearInput = () => { keys.clear(); pointers.clear(); touchControls.clear(); touchLean.clear(); gameplayBindings.cancel(); simAccumulator = 0; cameraCycle = false; diagnostics.input = { throttle: 0, brake: 0, steer: 0, lean: 0, reverse: false }; };
gameplayBindings.subscribe(clearInput);
window.addEventListener('gamepaddisconnected', clearInput);
let pageSuspended = false;
const syncAudio = () => { void audio.setActive(focused && !document.hidden && !lost && !menuOpen && !pageSuspended).catch(error => { diagnostics.audioError = error.message; }); };
window.addEventListener('blur', () => { focused = false; clearInput(); syncAudio(); });
window.addEventListener('focus', () => { focused = true; syncAudio(); });
document.addEventListener('visibilitychange', () => { clearInput(); syncAudio(); });
canvas.addEventListener('webglcontextlost', event => { event.preventDefault(); lost = true; for(const model of remoteModels.values()){model.chunks=null;model.gpuTextures=[];} diagnostics.ready = false; clearInput(); syncAudio(); status('Graphics context lost; waiting for recovery.'); });
// Every old GPU handle belongs to the lost context, including vegetation instances.
// Discard those references before buildMesh runs its normal resource cleanup.
canvas.addEventListener('webglcontextrestored', () => { try { lost = false; wheelEffectsRenderer = undefined; trainRenderer = undefined; waterRenderer = undefined; chunks = []; vehicleChunks = []; sceneChunks = []; sceneDraws = []; skyChunks = []; ecoGroups = []; gpuTextures = []; initializeGl(); if (terrain) { buildMesh(); uploadTerrainTextures(); } syncAudio(); diagnostics.ready = Boolean(terrain); status(simReady ? 'Physics reconstruction. Handling, collisions, animation and sound are incomplete.' : 'Static terrain.'); } catch (error) { status(error.message, true); } });
window.addEventListener('pagehide', event => {
  cancelAnimationFrame(frame); frame = 0; clearInput();
  if (event.persisted) {
    // Back/forward cache retains this page. Its live Rust worker and audio
    // resources must survive too; closing them leaves a restored frozen ATV.
    pageSuspended = true; focused = false; syncAudio();
  } else { multiplayer?.close(); audio.close(); worker.terminate(); }
});
window.addEventListener('pageshow', event => {
  if (!event.persisted || !pageSuspended) return;
  pageSuspended = false; focused = document.hasFocus(); lastTime = 0;
  clearInput(); syncAudio();
  if (!frame) frame = requestAnimationFrame(render);
});
try { if (!gl) throw new Error('WebGL 2 is required to render this terrain.'); initializeGl(); frame = requestAnimationFrame(render); const params = new URLSearchParams(location.search);
  if (params.get('source') === 'disc') { menuOpen = false; $('frontend').hidden = true; }
  else {
    hostedContent = openHostedContent();
    multiplayer = new Multiplayer({codec,command,ready:()=>simReady,paused:()=>menuOpen||!focused||document.hidden||lost||pageSuspended,listener:()=>({position:[camera.x,camera.y,camera.z],forward:[Math.sin(camera.yaw)*Math.cos(camera.pitch),Math.sin(camera.pitch),Math.cos(camera.yaw)*Math.cos(camera.pitch)],velocity:simFrame?.linear_velocity??[0,0,0]}),audio:()=>audio,assets:remoteAssets,clearAssets:()=>{for(const model of remoteModels.values())disposeRemoteGpu(model);remoteModels.clear();},report:message=>{diagnostics.multiplayerError=message;status(message,true);},onState:()=>{if(frontend){frontend.layoutKey='';void frontend.refresh();}},onStart:async map=>{frontend.busy=true;frontend.notice.textContent='Loading shared environment…';try {await loadGame(null,`env${String(map).padStart(2,'0')}`);frontend.resume();}finally{frontend.busy=false;}},onLeave:message=>{for(const model of remoteModels.values())disposeRemoteGpu(model);remoteModels.clear();if(frontend){void frontend.show('multiplayer').then(()=>{frontend.notice.textContent=message;});}}});
    frontend = new Frontend({multiplayer,codec, json, content: hostedContent, read, diagnosticsOverlay: stats, mobileControls: touchControls, audio: () => audio, start: environment => loadGame(null, environment), active: open => { menuOpen = open; clearInput(); document.body.classList.toggle('menu-open', open); syncAudio(); if (!open) canvas.focus({preventScroll:true}); }});
    hud = new HeadUpDisplay(frontend, canvas);
    if (params.get('drive') === '1') {
      menuOpen = false; $('frontend').hidden = true;
      void frontend.init(false).then(() => loadGame(null, params.get('environment') ?? 'env01')).catch(error => status(error.message, true));
    } else { document.body.classList.add('menu-open'); void frontend.init().then(()=>{if(params.has('room'))return frontend.show('multiplayer');}); }
  } }
catch (error) { status(error.message, true); $('menu-notice').textContent = error.message; }
