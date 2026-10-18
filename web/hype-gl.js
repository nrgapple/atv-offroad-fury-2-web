// Alpha hype WebGL effects (authored): a landing burst of dust and thrown dirt,
// and a screen-space sun glare around the apex of big jumps.
export const BURST_FLOATS = 13; // x, y, z, birth, vx, vy, vz, life, size, kind, r, g, b

function compile(gl, type, source) {
  const s = gl.createShader(type); gl.shaderSource(s, source); gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) { const log = gl.getShaderInfoLog(s); gl.deleteShader(s); throw new Error(log); }
  return s;
}
function link(gl, vertex, fragment) {
  const v = compile(gl, gl.VERTEX_SHADER, vertex), f = compile(gl, gl.FRAGMENT_SHADER, fragment), program = gl.createProgram();
  gl.attachShader(program, v); gl.attachShader(program, f); gl.linkProgram(program); gl.deleteShader(v); gl.deleteShader(f);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
  return program;
}

// Deterministic jitter so tests and replays see the same burst.
const hash = n => { const x = Math.sin(n * 12.9898) * 43758.5453; return x - Math.floor(x); };

export class BurstEmitter {
  constructor(capacity = 192) { this.capacity = capacity; this.data = new Float32Array(capacity * BURST_FLOATS); this.reset(); }
  reset() { this.next = 0; this.live = 0; this.emitted = 0; this.bursts = 0; this.data.fill(0); }
  push(values) {
    this.data.set(values, this.next * BURST_FLOATS);
    this.next = (this.next + 1) % this.capacity; this.live = Math.min(this.capacity, this.live + 1); this.emitted++;
  }
  // A ring of dust puffs plus dirt clods, scaled by landing severity (ft/s).
  burst(position, severity, time, tint, heading = [0, 0]) {
    const power = Math.max(0, Math.min(1, (severity - 10) / 45));
    if (power <= 0) return 0;
    const seed = this.bursts++ * 7.31, [r, g, b] = tint, puffs = Math.round(12 + 14 * power), clods = Math.round(8 + 18 * power);
    for (let i = 0; i < puffs; i++) {
      const angle = (i / puffs) * Math.PI * 2 + hash(seed + i) * .5, speed = (9 + 18 * power) * (.7 + .5 * hash(seed + i * 3.1));
      this.push([position[0], position[1] + .4, position[2], time, Math.cos(angle) * speed + heading[0] * .25, 1.5 + 3 * hash(seed + i * 5.7), Math.sin(angle) * speed + heading[1] * .25,
        1.2 + .9 * power + .5 * hash(seed + i * 2.3), 1.1 + 1.2 * power, 0, r, g, b]);
    }
    for (let i = 0; i < clods; i++) {
      const angle = hash(seed + i * 9.1) * Math.PI * 2, speed = (5 + 13 * power) * (.5 + hash(seed + i * 4.4));
      this.push([position[0], position[1] + .3, position[2], time, Math.cos(angle) * speed + heading[0] * .35, 9 + 16 * power * hash(seed + i * 6.6), Math.sin(angle) * speed + heading[1] * .35,
        .7 + .5 * hash(seed + i * 8.8), .22 + .3 * hash(seed + i * 1.9), 1, r * .55, g * .5, b * .45]);
    }
    return puffs + clods;
  }
}

export class BurstRenderer {
  constructor(gl, capacity) { this.gl = gl; this.emitter = new BurstEmitter(capacity); this.draws = 0; this.restore(); }
  restore() {
    const gl = this.gl;
    this.program = link(gl, `#version 300 es
layout(location=0) in vec2 corner;
layout(location=1) in vec4 seed; // x, y, z, birth
layout(location=2) in vec4 motion; // vx, vy, vz, life
layout(location=3) in vec2 shape; // size, kind
layout(location=4) in vec3 tint;
uniform mat4 viewProjection;
uniform vec3 eye, right, up, light, fogColor;
uniform float now, fogEnd;
out vec2 v; out vec4 color;
void main(){
  float age = now - seed.w, t = age / motion.w;
  float alive = step(0.0, age) * step(t, 1.0) * step(0.001, motion.w);
  vec3 center; float size, alpha;
  if (shape.y < 0.5) {
    // Dust: fast outward shove with heavy drag, a slow rise and spread.
    float k = 2.6;
    center = seed.xyz + motion.xyz * (1.0 - exp(-k * age)) / k + vec3(0.0, 0.9 * age, 0.0);
    size = shape.x * (0.5 + 1.2 * sqrt(clamp(t, 0.0, 1.0)));
    alpha = 0.3 * smoothstep(0.0, 0.05, t) * pow(1.0 - t, 1.6);
  } else {
    // Clods: ballistic under gravity (ft/s²), opaque until the end.
    center = seed.xyz + motion.xyz * age + vec3(0.0, -16.0 * age * age, 0.0);
    size = shape.x;
    alpha = 1.0 - smoothstep(0.75, 1.0, t);
  }
  vec3 world = center + (right * corner.x + up * corner.y) * size;
  float fog = smoothstep(fogEnd * 0.35, fogEnd, length(center - eye));
  // Puffs next to the lens would fill the frame; thin them out up close.
  float near = shape.y < 0.5 ? smoothstep(4.0, 14.0, length(center - eye)) : 1.0;
  color = vec4(mix(tint * light, fogColor, fog), alive * alpha * near * (1.0 - fog));
  v = corner;
  gl_Position = alive > 0.0 ? viewProjection * vec4(world, 1.0) : vec4(2.0, 2.0, 2.0, 1.0);
}`, `#version 300 es
precision mediump float;
in vec2 v; in vec4 color;
out vec4 result;
void main(){ float r = dot(v, v); if (r >= 1.0) discard; float a = 1.0 - r; result = vec4(color.rgb, color.a * a * a); }`);
    this.uniforms = Object.fromEntries(['viewProjection', 'eye', 'right', 'up', 'light', 'fogColor', 'now', 'fogEnd'].map(n => [n, gl.getUniformLocation(this.program, n)]));
    this.vao = gl.createVertexArray(); this.corners = gl.createBuffer(); this.instances = gl.createBuffer();
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.corners); gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, 1, 1, -1, -1, 1, 1, -1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instances); gl.bufferData(gl.ARRAY_BUFFER, this.emitter.data, gl.DYNAMIC_DRAW);
    for (const [location, size, offset] of [[1, 4, 0], [2, 4, 16], [3, 2, 32], [4, 3, 40]]) {
      gl.enableVertexAttribArray(location); gl.vertexAttribPointer(location, size, gl.FLOAT, false, BURST_FLOATS * 4, offset); gl.vertexAttribDivisor(location, 1);
    }
    gl.bindVertexArray(null);
    this.uploaded = -1;
  }
  reset() { this.emitter.reset(); this.uploaded = -1; }
  draw({ viewProjection, camera, now, light, fogColor, fogEnd }) {
    const gl = this.gl, e = this.emitter;
    this.draws = 0;
    if (!e.live) return 0;
    gl.useProgram(this.program); gl.bindVertexArray(this.vao);
    if (this.uploaded !== e.emitted) { gl.bindBuffer(gl.ARRAY_BUFFER, this.instances); gl.bufferSubData(gl.ARRAY_BUFFER, 0, e.data); this.uploaded = e.emitted; }
    const { x, y, z, yaw, pitch } = camera;
    const right = [Math.cos(yaw), 0, -Math.sin(yaw)], up = [-Math.sin(yaw) * Math.sin(pitch), Math.cos(pitch), -Math.cos(yaw) * Math.sin(pitch)];
    const u = this.uniforms;
    gl.uniformMatrix4fv(u.viewProjection, false, viewProjection); gl.uniform3f(u.eye, x, y, z); gl.uniform3fv(u.right, right); gl.uniform3fv(u.up, up);
    gl.uniform3fv(u.light, light); gl.uniform3fv(u.fogColor, fogColor); gl.uniform1f(u.now, now); gl.uniform1f(u.fogEnd, fogEnd);
    gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA); gl.depthMask(false);
    gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, e.live);
    gl.depthMask(true); gl.disable(gl.BLEND); gl.bindVertexArray(null);
    this.draws = 1;
    return 1;
  }
  dispose() { const gl = this.gl; gl.deleteVertexArray(this.vao); gl.deleteBuffer(this.corners); gl.deleteBuffer(this.instances); gl.deleteProgram(this.program); }
}

// Screen position (0..1, y up) of a world direction at infinity, or null when
// it lies behind the camera. `m` is a column-major view-projection matrix.
export function projectDirection(m, d) {
  const x = m[0] * d[0] + m[4] * d[1] + m[8] * d[2], y = m[1] * d[0] + m[5] * d[1] + m[9] * d[2], w = m[3] * d[0] + m[7] * d[1] + m[11] * d[2];
  if (!(w > 1e-4)) return null;
  return [(x / w + 1) / 2, (y / w + 1) / 2];
}

export class GlareRenderer {
  constructor(gl) { this.gl = gl; this.draws = 0; this.failed = false; this.sun = null; this.restore(); }
  restore() {
    const gl = this.gl;
    try {
      this.program = link(gl, `#version 300 es
void main(){ vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0); gl_Position = vec4(p, 0.0, 1.0); }`, `#version 300 es
precision mediump float;
uniform vec2 resolution, sun;
uniform float amount, visible;
uniform vec3 tint;
out vec4 result;
float blob(vec2 uv, vec2 at, float size, float aspect){ vec2 d = (uv - at) * vec2(aspect, 1.0); return exp(-dot(d, d) * size); }
void main(){
  vec2 uv = gl_FragCoord.xy / resolution;
  float aspect = resolution.x / resolution.y;
  vec2 d = (uv - sun) * vec2(aspect, 1.0);
  float r2 = dot(d, d);
  vec3 c = tint * (1.6 * exp(-r2 * 60.0) + 0.55 * exp(-r2 * 5.0) + 0.35 * exp(-abs(d.y) * 120.0) * exp(-abs(d.x) * 1.8)) * visible;
  // Lens ghosts on the line through the screen centre.
  vec2 axis = vec2(0.5) - sun;
  c += vec3(1.0, 0.75, 0.45) * 0.20 * blob(uv, sun + axis * 0.6, 220.0, aspect) * visible;
  c += vec3(0.55, 0.85, 1.0) * 0.16 * blob(uv, sun + axis * 1.25, 90.0, aspect) * visible;
  c += vec3(0.85, 0.6, 1.0) * 0.12 * blob(uv, sun + axis * 1.7, 500.0, aspect) * visible;
  // A warm exposure lift across the whole frame sells the bright sky.
  c += tint * 0.1;
  result = vec4(c * amount, 1.0);
}`);
      this.uniforms = Object.fromEntries(['resolution', 'sun', 'amount', 'visible', 'tint'].map(n => [n, gl.getUniformLocation(this.program, n)]));
      this.vao = gl.createVertexArray();
    } catch (error) { this.failed = true; this.error = String(error); }
  }
  draw({ width, height, amount, viewProjection, sunDirection, sunColor = [1, .95, .8], yaw = 0 }) {
    this.draws = 0;
    if (this.failed || !(amount > .002)) return 0;
    const gl = this.gl, length = Math.hypot(...sunDirection) || 1, dir = sunDirection.map(v => v / length);
    let sun = projectDirection(viewProjection, dir), visible = 1;
    if (!sun) {
      // Sun behind the camera: leak its light in over the top edge on its side.
      // Mirror it in front of the camera to find its side, whatever handedness.
      const f = [Math.sin(yaw), 0, Math.cos(yaw)], along = dir[0] * f[0] + dir[2] * f[2];
      const mirrored = projectDirection(viewProjection, [dir[0] - 2 * along * f[0], Math.max(dir[1], .2), dir[2] - 2 * along * f[2]]);
      sun = [mirrored ? Math.max(-.1, Math.min(1.1, mirrored[0])) : .5, 1.18]; visible = .6;
    }
    // Off-screen suns fade their core and ghosts but keep the warm wash.
    const outside = Math.max(0, Math.abs(sun[0] - .5) - .5, Math.abs(sun[1] - .5) - .5);
    visible *= Math.exp(-outside * 2.5);
    this.sun = sun;
    const tint = sunColor.map(v => Math.min(1, .55 + .6 * v));
    gl.disable(gl.DEPTH_TEST); gl.depthMask(false); gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE);
    gl.useProgram(this.program); gl.bindVertexArray(this.vao);
    const u = this.uniforms;
    gl.uniform2f(u.resolution, width, height); gl.uniform2f(u.sun, sun[0], sun[1]); gl.uniform1f(u.amount, Math.min(1, amount));
    gl.uniform1f(u.visible, visible); gl.uniform3fv(u.tint, tint);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.disable(gl.BLEND); gl.depthMask(true); gl.enable(gl.DEPTH_TEST); gl.bindVertexArray(null);
    this.draws = 1;
    return 1;
  }
  dispose() { if (this.vao) this.gl.deleteVertexArray(this.vao); if (this.program) this.gl.deleteProgram(this.program); this.vao = this.program = null; }
}
