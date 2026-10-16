// Enhanced/High tire tracks: a fixed ring of terrain-draped quads behind each
// wheel, faded by simulation age, submitted in one draw. Where they appear
// follows the original terrain-zone flags: GenerateTireTrack zones (mud) leave
// dark ruts; other zones that dirty the quad (DirtAnimChangeTime, for example
// dirt and snow) leave lighter authored marks. Presentation only.
export const TRACK_FLOATS = 7; // x, y, z, across, along, birth time, strength
const VERTICES = 6;

export function trackStrength(zone, mud) {
  if (mud || zone?.tireTrack) return .5;
  return zone?.time > 0 && zone.amount > 0 ? .26 : 0;
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
      const strength = frame.grounded?.[i] && !wet && speed > 2 ? trackStrength(zones?.[effects.surfaces?.[i]], effects.mud?.[i]) : 0;
      if (!strength) { this.wheels[i] = null; continue; }
      const x = center[0], z = center[2], y = height(x, z);
      if (!Number.isFinite(y)) { this.wheels[i] = null; continue; }
      const last = this.wheels[i];
      if (!last) { this.wheels[i] = { x, y, z, along: 0, left: null, right: null }; continue; }
      const dx = x - last.x, dz = z - last.z, d = Math.hypot(dx, dz);
      if (d > 4) { this.wheels[i] = { x, y, z, along: 0, left: null, right: null }; continue; }
      if (d < .6) continue;
      const half = Math.max(.25, radius * .45), nx = -dz / d * half, nz = dx / d * half;
      const left = [x + nx, height(x + nx, z + nz) + .04, z + nz], right = [x - nx, height(x - nx, z - nz) + .04, z - nz];
      const lastLeft = last.left ?? [last.x + nx, height(last.x + nx, last.z + nz) + .04, last.z + nz];
      const lastRight = last.right ?? [last.x - nx, height(last.x - nx, last.z - nz) + .04, last.z - nz];
      if (![...left, ...right, ...lastLeft, ...lastRight].every(Number.isFinite)) { this.wheels[i] = null; continue; }
      const along = last.along + d;
      const v = (p, across, a) => [...p, across, a, time, strength];
      const quad = [v(lastLeft, -1, last.along), v(lastRight, 1, last.along), v(left, -1, along), v(lastRight, 1, last.along), v(right, 1, along), v(left, -1, along)];
      const at = this.next * VERTICES * TRACK_FLOATS;
      quad.forEach((vertex, k) => this.data.set(vertex, at + k * TRACK_FLOATS));
      this.mark(this.next, this.next + 1);
      this.next = (this.next + 1) % this.capacity; this.count = Math.min(this.capacity, this.count + 1); this.total++;
      this.wheels[i] = { x, y, z, along, left, right };
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
layout(location=1) in vec4 track; // across, along, birth, strength
uniform mat4 viewProjection;
uniform float now, life;
out vec2 v; out float alpha;
void main(){
  float age = now - track.z;
  alpha = track.w * clamp(1.0 - age / life, 0.0, 1.0) * step(0.0, age + 0.5);
  v = track.xy;
  gl_Position = track.w > 0.0 ? viewProjection * vec4(position, 1.0) : vec4(2.0, 2.0, 2.0, 1.0);
}`);
    const fragment = compile(gl.FRAGMENT_SHADER, `#version 300 es
precision mediump float;
in vec2 v; in float alpha;
out vec4 color;
void main(){
  // Knobby tread: darker lugs every ~0.4 ft, soft outer edges.
  float lug = 0.82 + 0.18 * smoothstep(0.3, 0.7, abs(fract(v.y * 2.5) * 2.0 - 1.0));
  float edge = 1.0 - smoothstep(0.55, 1.0, abs(v.x));
  color = vec4(0.0, 0.0, 0.0, alpha * lug * edge);
}`);
    const program = this.program = gl.createProgram(); gl.attachShader(program, vertex); gl.attachShader(program, fragment); gl.linkProgram(program);
    gl.deleteShader(vertex); gl.deleteShader(fragment);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
    this.uniforms = { viewProjection: gl.getUniformLocation(program, 'viewProjection'), now: gl.getUniformLocation(program, 'now'), life: gl.getUniformLocation(program, 'life') };
    this.vao = gl.createVertexArray(); this.buffer = gl.createBuffer();
    gl.bindVertexArray(this.vao); gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer); gl.bufferData(gl.ARRAY_BUFFER, this.builder.data, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, TRACK_FLOATS * 4, 0);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 4, gl.FLOAT, false, TRACK_FLOATS * 4, 12);
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
    gl.enable(gl.BLEND); gl.blendFunc(gl.ZERO, gl.ONE_MINUS_SRC_ALPHA); gl.depthMask(false); gl.depthFunc(gl.LEQUAL);
    gl.enable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(-1, -2);
    gl.drawArrays(gl.TRIANGLES, 0, b.count * VERTICES);
    gl.disable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(0, 0); gl.depthFunc(gl.LESS); gl.depthMask(true); gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
    this.draws = 1;
    return 1;
  }
  dispose() { const gl = this.gl; gl.deleteVertexArray(this.vao); gl.deleteBuffer(this.buffer); gl.deleteProgram(this.program); }
}
