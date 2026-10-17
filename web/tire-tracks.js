// Enhanced/High tire tracks: a fixed ring of terrain-draped quads behind each
// wheel, faded by simulation age, submitted in one draw. Where they appear
// follows the original terrain-zone flags: GenerateTireTrack zones (mud) are
// soft and rut under every rolling wheel; other zones that dirty the quad
// (DirtAnimChangeTime, for example dirt and snow) are firm, so plain rolling
// only faintly presses them. What a wheel leaves comes from its physics
// contact: speed deepens rolling ruts, and sideways scrub or wheelspin/lockup
// (wheel_effects.slip) tears wider, untreaded scuffs. Marks modulate the lit
// ground instead of painting one colour. Presentation only.
export const TRACK_FLOATS = 9; // x, y, z, across, along, birth time, rut, scuff, wet
const VERTICES = 6;
const smooth = (a, b, x) => { const t = Math.min(1, Math.max(0, (x - a) / (b - a))); return t * t * (3 - 2 * t); };

// How soft the ground is: 1 mud ruts, .35 firm dirt/snow, 0 no marks.
export function trackStrength(zone, mud) {
  if (mud || zone?.tireTrack) return 1;
  return zone?.time > 0 && zone.amount > 0 ? .35 : 0;
}

// Per-wheel mark from the contact physics (ft/s): `slip` is [sideways scrub,
// tread minus ground speed]. Slow rolling barely marks; turning without
// sliding keeps the slip under the scuff threshold.
export function wheelMark(softness, speed, slip) {
  if (!softness) return { rut: 0, scuff: 0 };
  const scrub = smooth(3, 14, Math.abs(slip?.[0] ?? 0)), spin = smooth(4, 18, Math.abs(slip?.[1] ?? 0));
  const roll = softness * (softness >= 1 ? .35 + .65 * smooth(3, 25, speed) : .45 * smooth(8, 40, speed));
  return { rut: roll, scuff: Math.max(scrub, spin) * (.45 + .55 * softness) };
}

// Pure builder over a Float32Array ring; the renderer uploads `dirty` ranges.
export class TrackBuilder {
  constructor(capacity) {
    this.capacity = capacity; this.data = new Float32Array(capacity * VERTICES * TRACK_FLOATS);
    this.reset();
  }
  reset() { this.next = 0; this.count = 0; this.wheels = [null, null, null, null]; this.dirty = null; this.data.fill(0); this.total = 0; }
  mark(from, to) { this.dirty = this.dirty ? [Math.min(this.dirty[0], from), Math.max(this.dirty[1], to)] : [from, to]; }
  // One simulation frame. `zones` from parseDirtZones; `height(x, z)` visible ground.
  step(frame, zones, height) {
    const effects = frame?.wheel_effects;
    if (!effects?.centers || frame.wreck?.phase !== 'Riding') { this.wheels.fill(null); return; }
    const time = effects.time ?? 0, speed = Math.abs(frame.speed_magnitude ?? 0);
    for (let i = 0; i < 4; i++) {
      const center = effects.centers[i], radius = effects.radii?.[i] ?? .8;
      const wet = effects.water_heights?.[i] > center[1] - radius;
      const softness = frame.grounded?.[i] && !wet && speed > 2 ? trackStrength(zones?.[effects.surfaces?.[i]], effects.mud?.[i]) : 0;
      if (!softness) { this.wheels[i] = null; continue; }
      const x = center[0], z = center[2], y = height(x, z);
      if (!Number.isFinite(y)) { this.wheels[i] = null; continue; }
      const { rut, scuff } = wheelMark(softness, speed, effects.slip?.[i]), mud = softness >= 1 ? 1 : 0;
      const last = this.wheels[i];
      if (!last) { this.wheels[i] = { x, y, z, along: 0, left: null, right: null, rut: 0, scuff: 0, peakRut: rut, peakScuff: scuff }; continue; }
      // A short burst of slip between emitted segments still counts.
      last.peakRut = Math.max(last.peakRut, rut); last.peakScuff = Math.max(last.peakScuff, scuff);
      const dx = x - last.x, dz = z - last.z, d = Math.hypot(dx, dz);
      if (d > 4) { this.wheels[i] = { x, y, z, along: 0, left: null, right: null, rut: 0, scuff: 0, peakRut: rut, peakScuff: scuff }; continue; }
      if (d < .6) continue;
      const endRut = last.peakRut, endScuff = last.peakScuff;
      // Nothing worth drawing at either end: move the strip start along.
      if (endRut + endScuff < .06 && last.rut + last.scuff < .06) { this.wheels[i] = { x, y, z, along: last.along + d, left: null, right: null, rut: 0, scuff: 0, peakRut: 0, peakScuff: 0 }; continue; }
      // Sliding smears a wider patch than the tread.
      const half = Math.max(.25, radius * .45) * (1 + .8 * endScuff), nx = -dz / d * half, nz = dx / d * half;
      const left = [x + nx, height(x + nx, z + nz) + .04, z + nz], right = [x - nx, height(x - nx, z - nz) + .04, z - nz];
      const lastLeft = last.left ?? [last.x + nx, height(last.x + nx, last.z + nz) + .04, last.z + nz];
      const lastRight = last.right ?? [last.x - nx, height(last.x - nx, last.z - nz) + .04, last.z - nz];
      if (![...left, ...right, ...lastLeft, ...lastRight].every(Number.isFinite)) { this.wheels[i] = null; continue; }
      const along = last.along + d;
      const begin = (p, across) => [...p, across, last.along, time, last.rut, last.scuff, mud];
      const end = (p, across) => [...p, across, along, time, endRut, endScuff, mud];
      const quad = [begin(lastLeft, -1), begin(lastRight, 1), end(left, -1), begin(lastRight, 1), end(right, 1), end(left, -1)];
      const at = this.next * VERTICES * TRACK_FLOATS;
      quad.forEach((vertex, k) => this.data.set(vertex, at + k * TRACK_FLOATS));
      this.mark(this.next, this.next + 1);
      this.next = (this.next + 1) % this.capacity; this.count = Math.min(this.capacity, this.count + 1); this.total++;
      this.wheels[i] = { x, y, z, along, left, right, rut: endRut, scuff: endScuff, peakRut: rut, peakScuff: scuff };
    }
  }
}

export class TireTrackRenderer {
  constructor(gl, { capacity, life }) {
    this.gl = gl; this.life = life; this.builder = new TrackBuilder(capacity); this.draws = 0;
    this.restore();
  }
  // (Re)creates GPU objects and uploads the whole retained ring (context restore).
  restore() {
    const gl = this.gl;
    const compile = (type, source) => { const s = gl.createShader(type); gl.shaderSource(s, source); gl.compileShader(s); if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s)); return s; };
    const vertex = compile(gl.VERTEX_SHADER, `#version 300 es
layout(location=0) in vec3 position;
layout(location=1) in vec4 track; // across, along, birth, rut
layout(location=2) in vec2 surface; // scuff, wet
uniform mat4 viewProjection;
uniform float now, life;
out vec2 v; out vec2 world; out vec3 mark;
void main(){
  float age = now - track.z;
  float fade = clamp(1.0 - age / life, 0.0, 1.0) * step(0.0, age + 0.5);
  v = track.xy; world = position.xz;
  mark = vec3(track.w, surface.x, surface.y) * vec3(fade, fade, 1.0);
  // Unused ring slots are all zero; live vertices sit at across = +-1.
  gl_Position = abs(track.x) > 0.5 ? viewProjection * vec4(position, 1.0) : vec4(2.0, 2.0, 2.0, 1.0);
}`);
    const fragment = compile(gl.FRAGMENT_SHADER, `#version 300 es
precision mediump float;
in vec2 v; in vec2 world; in vec3 mark;
out vec4 color;
float hash(vec2 p){ return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise(vec2 p){ vec2 i = floor(p), f = fract(p); f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1, 0)), f.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), f.x), f.y); }
void main(){
  float rut = mark.x, scuff = mark.y, wet = mark.z, side = abs(v.x);
  // Ground is uneven: marks come and go along the path.
  float clump = smoothstep(0.15, 0.7, noise(world * 0.35) * 0.7 + noise(world * 1.7) * 0.3);
  float edge = 1.0 - smoothstep(0.6, 1.0, side);
  // Rolling: compacted tread with knobby lugs and a slightly raised, lighter berm.
  float lug = 0.75 + 0.25 * smoothstep(0.3, 0.7, abs(fract(v.y * 2.5) * 2.0 - 1.0));
  float press = rut * mix(0.45, 1.0, clump) * edge * lug;
  float berm = rut * (1.0 - wet) * smoothstep(0.55, 0.85, side) * (1.0 - smoothstep(0.85, 1.0, side));
  float m = 1.0 - press * mix(0.45, 0.55, wet) + berm * 0.08;
  // Sliding/spinning: soil torn into grooves and loose ridges streaked along the
  // path (darker and lighter than the ground, so it reads on dirt and snow);
  // mud only smears darker.
  float streak = noise(vec2(v.x * 3.0, v.y * 0.25) + world * 0.05);
  float tear = scuff * mix(0.4, 1.0, clump) * (1.0 - smoothstep(0.45, 1.0, side));
  m *= mix(mix(0.7, 1.14, smoothstep(0.25, 0.75, streak)), 0.55 + 0.15 * streak, wet) * tear + 1.0 - tear;
  // Modulate-2x blend: 0.5 leaves the ground unchanged.
  color = vec4(vec3(0.5 * m), 1.0);
}`);
    const program = this.program = gl.createProgram(); gl.attachShader(program, vertex); gl.attachShader(program, fragment); gl.linkProgram(program);
    gl.deleteShader(vertex); gl.deleteShader(fragment);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
    this.uniforms = { viewProjection: gl.getUniformLocation(program, 'viewProjection'), now: gl.getUniformLocation(program, 'now'), life: gl.getUniformLocation(program, 'life') };
    this.vao = gl.createVertexArray(); this.buffer = gl.createBuffer();
    gl.bindVertexArray(this.vao); gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer); gl.bufferData(gl.ARRAY_BUFFER, this.builder.data, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, TRACK_FLOATS * 4, 0);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 4, gl.FLOAT, false, TRACK_FLOATS * 4, 12);
    gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2, 2, gl.FLOAT, false, TRACK_FLOATS * 4, 28);
    gl.bindVertexArray(null);
    this.builder.dirty = null;
    this.gpuBytes = this.builder.data.byteLength;
  }
  step(frame, zones, height) { this.builder.step(frame, zones, height); }
  reset() { this.builder.reset(); if (this.buffer) { const gl = this.gl; gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer); gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.builder.data); } this.builder.dirty = null; }
  draw({ viewProjection, now }) {
    const gl = this.gl, b = this.builder;
    this.draws = 0;
    if (!b.count) return 0;
    gl.useProgram(this.program); gl.bindVertexArray(this.vao); gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    if (b.dirty) {
      const per = VERTICES * TRACK_FLOATS, [from, to] = b.dirty;
      gl.bufferSubData(gl.ARRAY_BUFFER, from * per * 4, b.data, from * per, (to - from) * per);
      b.dirty = null;
    }
    gl.uniformMatrix4fv(this.uniforms.viewProjection, false, viewProjection); gl.uniform1f(this.uniforms.now, now); gl.uniform1f(this.uniforms.life, this.life);
    gl.enable(gl.BLEND); gl.blendFunc(gl.DST_COLOR, gl.SRC_COLOR); gl.depthMask(false); gl.depthFunc(gl.LEQUAL);
    gl.enable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(-1, -2);
    gl.drawArrays(gl.TRIANGLES, 0, b.count * VERTICES);
    gl.disable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(0, 0); gl.depthFunc(gl.LESS); gl.depthMask(true); gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
    this.draws = 1;
    return 1;
  }
  dispose() { const gl = this.gl; gl.deleteVertexArray(this.vao); gl.deleteBuffer(this.buffer); gl.deleteProgram(this.program); }
}
