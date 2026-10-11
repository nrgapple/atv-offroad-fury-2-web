// Original content and vehicle component simulation run in the Rust worker.
import { GameAudio } from './audio.js';
const $ = id => document.getElementById(id);
const canvas = $('game');
const gl = canvas.getContext('webgl2', { antialias: true, alpha: false, powerPreference: 'high-performance' });
const status = (text, error = false) => { $('status').textContent = text; $('status').classList.toggle('error', error); };
const decoder = new TextDecoder();
const encoder = new TextEncoder();
const worker = new Worker('worker.js', { type: 'module' });
const pending = new Map();
let requestId = 0, generation = 0, terrain, vehicle, program, uniforms, chunks = [], vehicleChunks = [], gpuTextures = [], fallbackTexture, terrainAtlas, terrainTextureMap, frame = 0, lastTime = 0, lost = false;
let focused = document.hasFocus(), gamepadsAvailable = true;
let simReady = false, simBusy = false, simAccumulator = 0, simFrame, simStart, simGeneration = 0, orbit = 0;
let sceneChunks = [], sceneDraws = [], skyChunks = [];
const identity = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
const camera = { x: 0, y: 0, z: 0, yaw: 0, pitch: -.35 };
const keys = new Set(), pointers = new Map();
const diagnostics = { ready: false, frames: 0, vertices: 0, triangles: 0, chunks: 0, glError: 0, camera: [], source: null };
let audio = new GameAudio(codec, message => { diagnostics.audioError = message; status(`Audio: ${message}`, true); });
// Read-only snapshot for verification; does not expose the locally opened file.
Object.defineProperty(window, 'atvTerrainStatus', { get: () => ({ ...diagnostics, audio: audio.snapshot(), simulation: simReady, frame: simFrame ? structuredClone(simFrame) : null, camera: [camera.x, camera.y, camera.z, camera.yaw, camera.pitch] }) });
worker.onmessage = ({ data }) => {
  if (data.startupError) { status(data.startupError, true); return; }
  const request = pending.get(data.id);
  if (!request) return;
  pending.delete(data.id);
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
    pending.set(id, { resolve, reject });
    worker.postMessage({ id, op, index, bytes });
  });
}
async function json(op, bytes) { return JSON.parse(decoder.decode(await codec(op, bytes))); }
async function command(op, value) { return json(op, encoder.encode(JSON.stringify(value))); }
async function read(file, offset, count) {
  if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(count) || offset < 0 || count < 0 || count > 128 * 1048576 || offset + count > file.size) throw new Error('Invalid disc file extent.');
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
  status('Reading original engine sounds…');
  await audio.load({ file, files, read, isCurrent: () => token === generation });
  const archive = files.find(entry => entry.path === '/TRACKS/TRACKSA/ENV01/E01_FR.RES');
  if (!archive) throw new Error('Original ENV01 terrain archive is missing.');
  status('Reading original terrain…');
  const resource = await read(file, archive.lba * 2048, archive.bytes);
  const entries = await json(2, resource);
  const index = entries.findIndex(entry => /(^|[\\/])env01\.tr2$/i.test(entry.name));
  if (index < 0) throw new Error('Original terrain is missing from the archive.');
  const sceneIndex = entries.findIndex(entry => /(^|[\\/])env01fr\.scn$/i.test(entry.name));
  if (sceneIndex < 0) throw new Error('Original start position is missing.');
  const sceneMetadata = await json(37, await codec(3, resource, sceneIndex));
  const scene = { position: sceneMetadata.start_position, yaw: Math.atan2(sceneMetadata.start_direction[0], sceneMetadata.start_direction[2]) };
  const quadFile = files.find(entry => entry.path === '/QUAD/BIKESA/ATV01.RES');
  if (!quadFile) throw new Error('Original ATV archive is missing.');
  status('Reading original ATV…');
  const quadArchive = await read(file, quadFile.lba * 2048, quadFile.bytes);
  const quadEntries = await json(2, quadArchive);
  const quadIndex = quadEntries.findIndex(entry => /(^|[\\/])atv01\.psg$/i.test(entry.name));
  if (quadIndex < 0) throw new Error('Original ATV mesh is missing.');
  const meshBytes = await codec(3, quadArchive, quadIndex), mesh = await json(11, meshBytes);
  const physicsIndex = quadEntries.findIndex(entry => /(^|[\\/])atv01s01\.phy$/i.test(entry.name));
  if (physicsIndex < 0) throw new Error('Original ATV physics is missing.');
  const physics = await codec(3, quadArchive, physicsIndex);
  let toyIndex = entries.findIndex(entry => /(^|[\\/])env01\.toy$/i.test(entry.name)), toy;
  if (toyIndex >= 0) toy = await codec(3, resource, toyIndex);
  else {
    const sceneArchiveFile = files.find(entry => entry.path === '/TRACKS/TRACKSA/ENV01/E01_ENDL.RES');
    if (!sceneArchiveFile) throw new Error('Original terrain materials are missing.');
    const sceneArchive = await read(file, sceneArchiveFile.lba * 2048, sceneArchiveFile.bytes), sceneEntries = await json(2, sceneArchive);
    toyIndex = sceneEntries.findIndex(entry => /(^|[\\/])env01\.toy$/i.test(entry.name));
    if (toyIndex < 0) throw new Error('Original terrain materials are missing.');
    toy = await codec(3, sceneArchive, toyIndex);
  }
  const materialFile = files.find(entry => entry.path === '/QUAD/BIKESA/ATV01_MT.RES');
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
  const riderArchive = await archiveAt('/RIDER/MRIDER.RES'), motions = await archiveAt('/ANIMSRDR.RES');
  const riderMesh = await json(27, await member(riderArchive, '/mrider.psg'));
  await codec(28, await member(motions, '/m_turnm.mot'));
  const pose = await command(29, { time: .8 });
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
      const model = await json(11, await codec(3, resource, modelIndex));
      const textures = await baseTextures(model, resource, entries, sceneTextures);
      for (const [name, texture] of textures) sceneTextures.set(name, texture);
      sceneMeshes.set(name, model);
    } catch (error) { unsupported.push({ name, error: error.message }); }
  }
  return { bytes, name: entries[index].name, scene, mesh, meshBytes, physics, toy, textures, sky, terrainTextures: { layout, images }, world: { metadata: sceneMetadata, meshes: sceneMeshes, textures: sceneTextures, unsupported }, rider: { mesh: riderMesh, pose, textures: riderTextures } };
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
    if (textureIndex < 0) { if (materialIndex < 0) continue; throw new Error(`Original material texture is missing: ${binding}`); }
    const pst = await codec(3, materialArchive, textureIndex);
    const info = await json(7, pst), rgba = await codec(8, pst);
    const image = { ...info, rgba, shader }; textures.set(material, image); cache.set(material, image);
  }
  return textures;
}
function textureBinding(text) {
  let inBase = false, texture, coordinates;
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
uniform mat4 viewProjection;
uniform mat4 model;
uniform vec3 eye;
uniform bool skyPass;
out vec3 surfaceNormal;
out float distanceToEye;
out float worldHeight;
out vec2 texcoord;
out vec2 worldXZ;
void main() {
  texcoord = uv;
  vec4 worldPosition = model * vec4(position, 1.0);
  worldXZ = worldPosition.xz;
  surfaceNormal = mat3(model) * normal;
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
uniform vec2 heights;
uniform float fogDistance;
uniform vec3 surfaceColor;
uniform sampler2D baseTexture;
uniform bool useTexture;
uniform bool useTerrainTexture;
uniform highp sampler2DArray terrainColors;
uniform highp usampler2D terrainMapping;
uniform float terrainSpacing;
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
  if (useTexture) base = texture(baseTexture, texcoord).rgb;
  if (useTerrainTexture) {
    vec2 grid = worldXZ / (terrainSpacing * 64.0);
    ivec2 tile = clamp(ivec2(floor(grid)), ivec2(0), textureSize(terrainMapping, 0) - 1);
    uint layer = texelFetch(terrainMapping, tile, 0).r;
    // 001325c0 selects the 64-cell subtexture; 00131790 submits its 8px border.
    vec2 coord = fract(grid) * .9375 + .03125;
    base = texture(terrainColors, vec3(coord, float(layer))).rgb;
  }
  float sunlight = max(dot(n, normalize(sunlightDirection)),0.0);
  vec3 lit = unlit ? base : base * (ambientLight + sunlightColor * sunlight);
  float fog = 1.0-exp(-pow(distanceToEye/fogDistance,2.0));
  color = vec4(skyPass ? base : mix(lit, fogColor, min(.94,fog)),1.0);
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
  uniforms = Object.fromEntries(['viewProjection', 'model', 'eye', 'heights', 'fogDistance', 'surfaceColor', 'baseTexture', 'useTexture', 'useTerrainTexture', 'terrainColors', 'terrainMapping', 'terrainSpacing', 'ambientLight', 'sunlightColor', 'sunlightDirection', 'fogColor', 'unlit', 'skyPass'].map(name => [name, gl.getUniformLocation(program, name)]));
  fallbackTexture = texture({ width: 1, height: 1, rgba: new Uint8Array([255, 255, 255, 255]) });
  terrainAtlas = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D_ARRAY, terrainAtlas);
  gl.texImage3D(gl.TEXTURE_2D_ARRAY, 0, gl.RGBA8, 1, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 255, 255]));
  gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  terrainTextureMap = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, terrainTextureMap);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.R16UI, 1, 1, 0, gl.RED_INTEGER, gl.UNSIGNED_SHORT, new Uint16Array([0]));
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.enable(gl.DEPTH_TEST); gl.disable(gl.CULL_FACE); gl.clearColor(.34, .43, .53, 1);
}
function clearMesh() {
  for (const chunk of [...chunks, ...vehicleChunks, ...sceneChunks, ...skyChunks]) { gl.deleteVertexArray(chunk.vao); gl.deleteBuffer(chunk.vertices); gl.deleteBuffer(chunk.indices); }
  for (const texture of gpuTextures) gl.deleteTexture(texture);
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
  const { layout, images } = source;
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
  diagnostics.terrainTextures = images.length;
}
function upload(vertices, indices, hasUv = false) {
  const chunk = { vao: gl.createVertexArray(), vertices: gl.createBuffer(), indices: gl.createBuffer(), count: indices.length };
  gl.bindVertexArray(chunk.vao); gl.bindBuffer(gl.ARRAY_BUFFER, chunk.vertices); gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
  const stride = hasUv ? 32 : 24;
  gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, stride, 0);
  gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 3, gl.FLOAT, false, stride, 12);
  if (hasUv) { gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2, 2, gl.FLOAT, false, stride, 24); }
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
  vehicle = { groups, position, yaw: source.scene.yaw, length: max[2] - min[2], height: max[1] - min[1], materials: source.mesh.materials, textures: source.textures, rider: source.rider };
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
function buildMesh() {
  clearMesh();
  const { width, depth, spacing, heights } = terrain;
  // Cap the first renderer at roughly 200k triangles; original samples are retained.
  const step = Math.max(1, Math.ceil(Math.sqrt((width - 1) * (depth - 1) / 100000)));
  const xs = [], zs = [];
  for (let x = 0; x < width - 1; x += step) xs.push(x);
  for (let z = 0; z < depth - 1; z += step) zs.push(z);
  xs.push(width - 1); zs.push(depth - 1);
  const height = (x, z) => heights[z * width + x];
  const centerX = vehicle ? Math.max(0, Math.min(width - 1, vehicle.position[0] / spacing)) : 0;
  const centerZ = vehicle ? Math.max(0, Math.min(depth - 1, vehicle.position[2] / spacing)) : 0;
  const patch = vehicle ? {
    x0: Math.max(0, Math.floor((centerX - 32) / step) * step),
    x1: Math.min(width - 1, Math.ceil((centerX + 32) / step) * step),
    z0: Math.max(0, Math.floor((centerZ - 32) / step) * step),
    z1: Math.min(depth - 1, Math.ceil((centerZ + 32) / step) * step),
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
    grid(fineX, fineZ, true);
  }
  const materialTextures = new Map();
  if (vehicle) for (const [name, decoded] of vehicle.textures) { const gpu = texture(decoded); materialTextures.set(name, gpu); gpuTextures.push(gpu); }
  if (vehicle) for (const group of vehicle.groups) {
    if (group.vertices.length > 65536) throw new Error('ATV mesh group exceeds index bounds.');
    const vertices = new Float32Array(group.vertices.length * 8);
    group.vertices.forEach(({ position: p, normal: n, uv }, index) => {
      vertices.set([...p, ...n, ...uv], index * 8);
    });
    const chunk = upload(vertices, new Uint16Array(group.indices), true);
    chunk.texture = materialTextures.get(vehicle.materials[group.material]);
    vehicleChunks.push(chunk);
  }
  let riderTriangles = 0, riderTextureCount = 0;
  if (vehicle?.rider) {
    const rider = vehicle.rider, riderTextures = new Map();
    for (const [name, decoded] of rider.textures) { const gpu = texture(decoded); riderTextures.set(name, gpu); gpuTextures.push(gpu); }
    riderTextureCount = riderTextures.size;
    for (const group of rider.mesh.lods[0].groups) {
      if (group.vertices.length > 65536) throw new Error('Rider mesh group exceeds index bounds.');
      const vertices = new Float32Array(group.vertices.length * 8);
      group.vertices.forEach((vertex, index) => {
        const bone = rider.mesh.nodes[vertex.node], pose = rider.pose.world_matrices[vertex.node];
        if (!bone || !pose) throw new Error('Rider animation node is missing.');
        const position = transform(pose, vertex.local_position, true);
        const normal = transform(pose, inverseDirection(bone.world_matrix, vertex.normal));
        vertices.set([...position, ...normal, ...vertex.uv], index * 8);
      });
      const chunk = upload(vertices, new Uint16Array(group.indices), true);
      chunk.texture = riderTextures.get(rider.mesh.materials[group.material]);
      vehicleChunks.push(chunk); riderTriangles += group.indices.length / 3;
    }
  }
  let sceneTriangles = 0, sceneInstances = 0, skippedSceneGroups = 0;
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
        const vertices = new Float32Array(group.vertices.length * 8);
        group.vertices.forEach((vertex, index) => vertices.set([...vertex.position, ...vertex.normal, ...vertex.uv], index * 8));
        const chunk = upload(vertices, new Uint16Array(group.indices), true);
        chunk.texture = textures.get(material); groups.push(chunk); sceneChunks.push(chunk);
      }
      geometry.set(name, groups);
    }
    for (const model of world.metadata.models) {
      const groups = geometry.get(model.name);
      if (!groups?.length) continue;
      sceneInstances++;
      for (const chunk of groups) { sceneDraws.push({ chunk, matrix: new Float32Array(model.world_matrix), unlit: !model.use_lighting }); sceneTriangles += chunk.count / 3; }
    }
  }
  gl.bindVertexArray(null);
  Object.assign(diagnostics, { vertices: vertexCount, triangles: triangleCount, chunks: chunks.length, vehicleVertices: vehicle?.groups.reduce((sum, group) => sum + group.vertices.length, 0) ?? 0, vehicleTriangles: vehicle?.groups.reduce((sum, group) => sum + group.indices.length / 3, 0) ?? 0, vehicleTextures: materialTextures.size, riderTriangles, riderTextures: riderTextureCount, vehiclePosition: vehicle?.position, finePatch: patch, sceneInstances, sceneTriangles, skippedSceneGroups, unsupportedSceneMeshes: terrain.world?.unsupported ?? [] });
}
function resetCamera() {
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
  const { x, y, z, yaw, pitch } = camera;
  const forward = [Math.sin(yaw) * Math.cos(pitch), Math.sin(pitch), Math.cos(yaw) * Math.cos(pitch)];
  const right = [-Math.cos(yaw), 0, Math.sin(yaw)];
  const up = [-Math.sin(yaw) * Math.sin(pitch), Math.cos(pitch), -Math.cos(yaw) * Math.sin(pitch)];
  const view = new Float32Array([right[0], up[0], -forward[0], 0, right[1], up[1], -forward[1], 0, right[2], up[2], -forward[2], 0, -(right[0] * x + right[2] * z), -(up[0] * x + up[1] * y + up[2] * z), forward[0] * x + forward[1] * y + forward[2] * z, 1]);
  const near = .5, far = Math.max(terrain.sizeX, terrain.sizeZ) * 5, f = 1 / Math.tan(Math.PI / 6);
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
  if (!focused || document.hidden) return empty;
  const pad = gamepad(), clamp = value => Math.max(-1, Math.min(1, value));
  const input = {
    throttle: Math.max(Number(keys.has('KeyW') || keys.has('ArrowUp')), pad?.buttons[7]?.value ?? 0),
    brake: Math.max(Number(keys.has('KeyS') || keys.has('ArrowDown')), pad?.buttons[6]?.value ?? 0),
    steer: Number(keys.has('KeyD') || keys.has('ArrowRight')) - Number(keys.has('KeyA') || keys.has('ArrowLeft')) + deadzone(pad?.axes[0]),
    lean: Number(keys.has('KeyE')) - Number(keys.has('KeyQ')) - deadzone(pad?.axes[1]),
    reverse: keys.has('KeyR') || Boolean(pad?.buttons[1]?.pressed),
  };
  for (const pointer of pointers.values()) {
    if (pointer.move) {
      input.steer += pointer.dx / 70;
      input.throttle = Math.max(input.throttle, -pointer.dy / 70);
      input.brake = Math.max(input.brake, pointer.dy / 70);
    } else if (pointer.touch) input.lean -= pointer.dy / 70;
  }
  input.throttle = Math.max(0, clamp(input.throttle)); input.brake = Math.max(0, clamp(input.brake));
  // Browser axes point right; the recovered steering scalar points left.
  input.steer = -clamp(input.steer); input.lean = clamp(input.lean);
  return input;
}
function acceptFrame(frame) {
  if (!frame?.origin?.every(Number.isFinite) || frame.origin.length !== 3 || !frame.orientation?.every(Number.isFinite) || frame.orientation.length !== 4 || !Number.isFinite(frame.speed) || !Number.isFinite(frame.rpm)) throw new Error('Simulation returned a nonfinite pose.');
  simFrame = frame;
  vehicle.position = frame.origin.slice();
}
async function startSimulation(source) {
  simReady = false; simFrame = undefined; simAccumulator = 0; simGeneration++;
  if (!source.physics) return;
  await codec(20, source.bytes); await codec(21, source.physics); await codec(22, source.meshBytes); await codec(26, source.toy);
  simStart = { origin: source.scene.position, yaw: source.scene.yaw };
  acceptFrame(await command(23, simStart));
  simReady = true; orbit = 0;
}
async function pumpSimulation() {
  if (simBusy || !simReady || !focused || document.hidden || lost) return;
  simBusy = true;
  const token = simGeneration;
  try {
    for (let count = 0; count < 4 && simAccumulator >= 1 / 60 && focused && !document.hidden && !lost; count++) {
      simAccumulator -= 1 / 60;
      const input = drivingInput(); diagnostics.input = input;
      const frame = await command(24, { dt: 1 / 60, input });
      if (token !== simGeneration) return;
      acceptFrame(frame);
      void audio.update(1 / 60, input, frame);
    }
  } catch (error) {
    if (token === simGeneration) { simReady = false; status(`Simulation stopped: ${error.message}`, true); }
  } finally { simBusy = false; }
}
function followCamera(dt) {
  const pad = focused ? gamepad() : null;
  orbit -= deadzone(pad?.axes[2]) * dt * 1.7;
  const matrix = vehicleMatrix(), heading = Math.atan2(matrix[8], matrix[10]) + orbit, distance = vehicle.length * 2.2;
  const target = [vehicle.position[0], vehicle.position[1] + vehicle.height * .55, vehicle.position[2]];
  const desiredX = target[0] - Math.sin(heading) * distance, desiredZ = target[2] - Math.cos(heading) * distance;
  const desiredY = Math.max(vehicle.position[1] + vehicle.height * 1.9, terrainHeight(desiredX, desiredZ) + vehicle.height);
  const smoothing = Math.min(1, dt * 10);
  camera.x += (desiredX - camera.x) * smoothing; camera.y += (desiredY - camera.y) * smoothing; camera.z += (desiredZ - camera.z) * smoothing;
  camera.yaw = Math.atan2(target[0] - camera.x, target[2] - camera.z);
  camera.pitch = Math.atan2(target[1] - camera.y, Math.hypot(target[0] - camera.x, target[2] - camera.z));
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
  frame = requestAnimationFrame(render);
  if (lost || !gl || !terrain || document.hidden) { lastTime = time; return; }
  const dt = Math.min(.05, Math.max(0, (time - (lastTime || time)) / 1000)); lastTime = time;
  const ratio = Math.min(devicePixelRatio || 1, 1.5), width = Math.max(1, Math.round(innerWidth * ratio)), height = Math.max(1, Math.round(innerHeight * ratio));
  if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
  updateCamera(dt);
  if (simReady && focused) {
    simAccumulator = Math.min(.1, simAccumulator + dt); void pumpSimulation();
    const patch = diagnostics.finePatch, x = vehicle.position[0] / terrain.spacing, z = vehicle.position[2] / terrain.spacing;
    if (patch && (x < patch.x0 + 12 || x > patch.x1 - 12 || z < patch.z0 + 12 || z > patch.z1 - 12) && x >= 32 && z >= 32 && x <= terrain.width - 33 && z <= terrain.depth - 33) buildMesh();
  }
  gl.viewport(0, 0, width, height); gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT); gl.useProgram(program);
  gl.uniformMatrix4fv(uniforms.viewProjection, false, matrix()); gl.uniform3f(uniforms.eye, camera.x, camera.y, camera.z);
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
  gl.uniform1f(uniforms.terrainSpacing, terrain.spacing); gl.uniform1i(uniforms.useTerrainTexture, Boolean(terrain.textures));
  for (const chunk of chunks) { gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0); }
  gl.uniform1i(uniforms.useTerrainTexture, false); gl.activeTexture(gl.TEXTURE0);
  gl.uniform3f(uniforms.surfaceColor, .68, .70, .73);
  for (const { chunk, matrix, unlit } of sceneDraws) {
    gl.uniformMatrix4fv(uniforms.model, false, matrix); gl.uniform1i(uniforms.unlit, unlit);
    gl.uniform1i(uniforms.useTexture, Boolean(chunk.texture)); gl.bindTexture(gl.TEXTURE_2D, chunk.texture ?? fallbackTexture);
    gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0);
  }
  gl.uniform1i(uniforms.unlit, false);
  if (vehicle) gl.uniformMatrix4fv(uniforms.model, false, vehicleMatrix());
  for (const chunk of vehicleChunks) {
    gl.uniform1i(uniforms.useTexture, chunk.texture ? 1 : 0); gl.bindTexture(gl.TEXTURE_2D, chunk.texture ?? fallbackTexture);
    gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0);
  }
  diagnostics.frames++;
  if (diagnostics.frames === 1 || diagnostics.frames % 120 === 0) {
    diagnostics.glError = gl.getError();
    if (diagnostics.glError) status(`Graphics error ${diagnostics.glError}.`, true);
  }
}
$('disc').addEventListener('change', async event => {
  const file = event.target.files[0]; if (!file || !gl || lost) return;
  const token = ++generation; status('Opening terrain…'); diagnostics.ready = false;
  audio.close(); audio = new GameAudio(codec, message => { diagnostics.audioError = message; status(`Audio: ${message}`, true); });
  delete diagnostics.audioError; void audio.resume().catch(error => { diagnostics.audioError = error.message; });
  simReady = false; simGeneration++; simAccumulator = 0; simFrame = undefined; $('disc').disabled = true;
  try {
    const source = file.name.toLowerCase().endsWith('.iso') ? await discTerrain(file, token) : { bytes: await read(file, 0, file.size), name: file.name };
    const info = await json(9, source.bytes), raw = await codec(10, source.bytes);
    if (token !== generation) return;
    const width = info.width_cells + 1, depth = info.depth_cells + 1;
    if (!Number.isInteger(width) || !Number.isInteger(depth) || width < 2 || depth < 2 || !Number.isFinite(info.spacing) || info.spacing <= 0 || raw.byteLength !== width * depth * 4) throw new Error('Invalid terrain dimensions.');
    const heights = new Float32Array(width * depth), view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    let min = Infinity, max = -Infinity;
    for (let i = 0; i < heights.length; i++) { const value = view.getFloat32(i * 4, true); if (!Number.isFinite(value)) throw new Error('Nonfinite terrain height.'); heights[i] = value; min = Math.min(min, value); max = Math.max(max, value); }
    terrain = { width, depth, spacing: info.spacing, heights, min, max, sizeX: (width - 1) * info.spacing, sizeZ: (depth - 1) * info.spacing, textures: source.terrainTextures, world: source.world, sky: source.sky };
    diagnostics.terrainTextures = 0;
    uploadTerrainTextures();
    setVehicle(source); buildMesh(); await startSimulation(source); resetCamera();
    Object.assign(diagnostics, { ready: true, source: source.name, width, depth, min, max, frames: 0 });
    $('reset').disabled = false; $('help').hidden = false;
    $('reset').textContent = simReady ? 'Reset ATV' : 'Reset view';
    $('help').textContent = simReady ? 'WASD / arrows: drive · R: reverse · triggers / left stick: controller · left touch: drive' : 'Free camera · drag to look · WASD to move · Q/E up/down';
    status(simReady ? 'Physics reconstruction. Handling, collisions, animation and sound are incomplete.' : 'Static terrain. Open the disc to load the ATV.');
  } catch (error) { if (token === generation) status(error.message, true); }
  finally { if (token === generation) $('disc').disabled = false; }
});
$('reset').onclick = async () => {
  if (!simStart || !vehicle) { resetCamera(); return; }
  simGeneration++; simReady = false; simAccumulator = 0; clearInput();
  try { await audio.reset(); acceptFrame(await command(23, simStart)); simReady = true; orbit = 0; buildMesh(); resetCamera(); }
  catch (error) { status(error.message, true); }
};
$('fullscreen').onclick = async () => {
  try { document.fullscreenElement ? await document.exitFullscreen() : await document.documentElement.requestFullscreen(); }
  catch { status('Fullscreen is unavailable in this browser; the view already fills the window.'); }
};
canvas.addEventListener('pointerdown', event => {
  if (!terrain) return;
  canvas.focus({ preventScroll: true }); canvas.setPointerCapture(event.pointerId);
  pointers.set(event.pointerId, { x: event.clientX, y: event.clientY, startX: event.clientX, startY: event.clientY, dx: 0, dy: 0, touch: event.pointerType === 'touch', move: event.pointerType === 'touch' && event.clientX < innerWidth / 2 });
});
canvas.addEventListener('pointermove', event => {
  const pointer = pointers.get(event.pointerId); if (!pointer) return;
  pointer.dx = Math.max(-70, Math.min(70, event.clientX - pointer.startX)); pointer.dy = Math.max(-70, Math.min(70, event.clientY - pointer.startY));
  if (!pointer.move) {
    if (simReady) orbit -= (event.clientX - pointer.x) * .004;
    else { camera.yaw -= (event.clientX - pointer.x) * .004; camera.pitch -= (event.clientY - pointer.y) * .004; }
  }
  pointer.x = event.clientX; pointer.y = event.clientY;
});
for (const name of ['pointerup', 'pointercancel', 'lostpointercapture']) canvas.addEventListener(name, event => pointers.delete(event.pointerId));
window.addEventListener('keydown', event => {
  if (document.activeElement !== canvas) return;
  if (['KeyW', 'KeyA', 'KeyS', 'KeyD', 'KeyQ', 'KeyE', 'KeyR', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ShiftLeft'].includes(event.code)) { event.preventDefault(); keys.add(event.code); }
});
window.addEventListener('keyup', event => keys.delete(event.code));
const clearInput = () => { keys.clear(); pointers.clear(); simAccumulator = 0; diagnostics.input = { throttle: 0, brake: 0, steer: 0, lean: 0, reverse: false }; };
const syncAudio = () => { void audio.setActive(focused && !document.hidden && !lost).catch(error => { diagnostics.audioError = error.message; }); };
window.addEventListener('blur', () => { focused = false; clearInput(); syncAudio(); });
window.addEventListener('focus', () => { focused = true; syncAudio(); });
document.addEventListener('visibilitychange', () => { clearInput(); syncAudio(); });
canvas.addEventListener('webglcontextlost', event => { event.preventDefault(); lost = true; diagnostics.ready = false; clearInput(); syncAudio(); status('Graphics context lost; waiting for recovery.'); });
canvas.addEventListener('webglcontextrestored', () => { try { lost = false; chunks = []; vehicleChunks = []; sceneChunks = []; sceneDraws = []; skyChunks = []; gpuTextures = []; initializeGl(); if (terrain) { buildMesh(); uploadTerrainTextures(); } syncAudio(); diagnostics.ready = Boolean(terrain); status(simReady ? 'Physics reconstruction. Handling, collisions, animation and sound are incomplete.' : 'Static terrain.'); } catch (error) { status(error.message, true); } });
window.addEventListener('pagehide', () => { cancelAnimationFrame(frame); audio.close(); worker.terminate(); clearInput(); });
try { if (!gl) throw new Error('WebGL 2 is required to render this terrain.'); initializeGl(); frame = requestAnimationFrame(render); }
catch (error) { status(error.message, true); }
