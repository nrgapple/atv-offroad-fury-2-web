// Alpha hype WebGL effects (authored): a landing burst of dust and thrown dirt,
// and a dark radial blur around the edges in big air.
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

// Big-air tunnel: copies the frame and blurs it radially towards the centre,
// darkening and desaturating the edges. The rider in the middle stays sharp.
export class AirBlurRenderer {
  constructor(gl) { this.gl = gl; this.draws = 0; this.failed = false; this.width = this.height = 0; this.restore(); }
  restore() {
    const gl = this.gl;
    this.width = this.height = 0;
    try {
      this.program = link(gl, `#version 300 es
void main(){ vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0); gl_Position = vec4(p, 0.0, 1.0); }`, `#version 300 es
precision mediump float;
uniform sampler2D scene;
uniform vec2 resolution;
uniform float amount;
out vec4 result;
void main(){
  vec2 uv = gl_FragCoord.xy / resolution;
  vec2 d = (uv - 0.5) * vec2(resolution.x / resolution.y, 1.0);
  float r = length(d);
  float edge = smoothstep(0.18, 0.85, r) * amount;
  // Zoom blur: ten taps stepping towards the centre, longer at the edges.
  vec2 step = (uv - 0.5) * edge * 0.035;
  vec3 c = vec3(0.0);
  for (int i = 0; i < 10; i++) c += texture(scene, uv - step * float(i)).rgb;
  c /= 10.0;
  float grey = dot(c, vec3(0.299, 0.587, 0.114));
  c = mix(c, vec3(grey), 0.45 * edge);
  c *= 1.0 - 0.7 * smoothstep(0.25, 1.0, r) * amount;
  result = vec4(c, 1.0);
}`);
      this.uniforms = Object.fromEntries(['scene', 'resolution', 'amount'].map(n => [n, gl.getUniformLocation(this.program, n)]));
      this.vao = gl.createVertexArray(); this.texture = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, this.texture);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    } catch (error) { this.failed = true; this.error = String(error); }
  }
  draw({ width, height, amount }) {
    this.draws = 0;
    if (this.failed || !(amount > .002)) return 0;
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, this.texture);
    if (width !== this.width || height !== this.height) { gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB8, width, height, 0, gl.RGB, gl.UNSIGNED_BYTE, null); this.width = width; this.height = height; }
    gl.copyTexSubImage2D(gl.TEXTURE_2D, 0, 0, 0, 0, 0, width, height);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.BLEND); gl.depthMask(false);
    gl.useProgram(this.program); gl.bindVertexArray(this.vao);
    gl.uniform1i(this.uniforms.scene, 0); gl.uniform2f(this.uniforms.resolution, width, height); gl.uniform1f(this.uniforms.amount, Math.min(1, amount));
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null); gl.depthMask(true); gl.enable(gl.DEPTH_TEST);
    this.draws = 1;
    return 1;
  }
  get textureBytes() { return this.width * this.height * 3; }
  dispose() { const gl = this.gl; if (this.vao) gl.deleteVertexArray(this.vao); if (this.texture) gl.deleteTexture(this.texture); if (this.program) gl.deleteProgram(this.program); this.vao = this.texture = this.program = null; }
}
