// Enhanced/High lingering dust: a small capped pool of large, slow, long-lived
// puffs behind the rear wheels on zones whose original TOY sets GenerateDust,
// tinted by the original terrain texture under the wheel. Authored; the
// recovered wheel dust (WHEEL_EFFECTS.md) is unchanged and drawn as before.
export const DUST_FLOATS = 9; // x, y, z, birth, life, size, r, g, b

export class DustEmitter {
  constructor(capacity) { this.capacity = capacity; this.data = new Float32Array(capacity * DUST_FLOATS); this.reset(); }
  reset() { this.next = 0; this.live = 0; this.timers = [0, 0]; this.data.fill(0); this.emitted = 0; }
  // One simulation frame. `tint(x, z)` returns an RGB triple in 0..1.
  step(frame, zones, tint, dt = 1 / 60) {
    const effects = frame?.wheel_effects;
    if (!effects?.centers || frame.wreck?.phase !== 'Riding') return;
    const time = effects.time ?? 0, speed = Math.abs(frame.speed_magnitude ?? 0);
    // Fewer, sparser puffs on a smaller pool so short rides never exhaust it.
    const interval = Math.max(.08, 12 / this.capacity);
    for (let side = 0; side < 2; side++) {
      const i = 2 + side, center = effects.centers[i], zone = zones?.[effects.surfaces?.[i]];
      const dry = frame.grounded?.[i] && zone?.dust && !effects.mud?.[i] && !(effects.water_heights?.[i] > center[1] - (effects.radii?.[i] ?? .8));
      if (!dry || speed < 15) { this.timers[side] = 0; continue; }
      this.timers[side] += dt;
      if (this.timers[side] < interval) continue;
      this.timers[side] -= interval;
      const color = tint(center[0], center[2]);
      const jitter = ((this.emitted * 2654435761) % 1000) / 1000;
      this.data.set([center[0], center[1] - (effects.radii?.[i] ?? .8) * .4, center[2], time, 2.8 + jitter * 1.6, 2.2 + Math.min(speed, 80) / 40 + jitter, ...color], this.next * DUST_FLOATS);
      this.next = (this.next + 1) % this.capacity; this.live = Math.min(this.capacity, this.live + 1); this.emitted++;
    }
  }
}

export class LingeringDustRenderer {
  constructor(gl, capacity) { this.gl = gl; this.emitter = new DustEmitter(capacity); this.draws = 0; this.restore(); }
  restore() {
    const gl = this.gl;
    const compile = (type, source) => { const s = gl.createShader(type); gl.shaderSource(s, source); gl.compileShader(s); if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s)); return s; };
    const vertex = compile(gl.VERTEX_SHADER, `#version 300 es
layout(location=0) in vec2 corner;
layout(location=1) in vec4 puff; // x, y, z, birth
layout(location=2) in vec2 shape; // life, size
layout(location=3) in vec3 tint;
uniform mat4 viewProjection;
uniform vec3 eye, right, up, light, fogColor;
uniform float now, fogEnd;
out vec2 v; out vec4 color;
void main(){
  float age = now - puff.w, t = age / shape.x;
  float alive = step(0.0, age) * step(t, 1.0) * step(0.001, shape.x);
  // Rises and spreads slowly; fades in quickly and out over its life.
  float size = shape.y * (0.45 + 1.1 * sqrt(clamp(t, 0.0, 1.0)));
  vec3 center = puff.xyz + vec3(0.0, 0.6 + 1.4 * t, 0.0);
  vec3 world = center + (right * corner.x + up * corner.y) * size;
  float fog = smoothstep(fogEnd * 0.35, fogEnd, length(center - eye));
  color = vec4(mix(tint * light, fogColor, fog), alive * 0.24 * smoothstep(0.0, 0.12, t) * (1.0 - t) * (1.0 - fog));
  v = corner;
  gl_Position = alive > 0.0 ? viewProjection * vec4(world, 1.0) : vec4(2.0, 2.0, 2.0, 1.0);
}`);
    const fragment = compile(gl.FRAGMENT_SHADER, `#version 300 es
precision mediump float;
in vec2 v; in vec4 color;
out vec4 result;
void main(){ float r = dot(v, v); if (r >= 1.0) discard; float a = (1.0 - r); result = vec4(color.rgb, color.a * a * a); }`);
    const program = this.program = gl.createProgram(); gl.attachShader(program, vertex); gl.attachShader(program, fragment); gl.linkProgram(program);
    gl.deleteShader(vertex); gl.deleteShader(fragment);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
    this.uniforms = Object.fromEntries(['viewProjection', 'eye', 'right', 'up', 'light', 'fogColor', 'now', 'fogEnd'].map(n => [n, gl.getUniformLocation(program, n)]));
    this.vao = gl.createVertexArray(); this.corners = gl.createBuffer(); this.instances = gl.createBuffer();
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.corners); gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, 1, 1, -1, -1, 1, 1, -1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instances); gl.bufferData(gl.ARRAY_BUFFER, this.emitter.data, gl.DYNAMIC_DRAW);
    for (const [location, size, offset] of [[1, 4, 0], [2, 2, 16], [3, 3, 24]]) { gl.enableVertexAttribArray(location); gl.vertexAttribPointer(location, size, gl.FLOAT, false, DUST_FLOATS * 4, offset); gl.vertexAttribDivisor(location, 1); }
    gl.bindVertexArray(null);
    this.uploaded = -1;
    this.gpuBytes = this.emitter.data.byteLength + 48;
  }
  step(frame, zones, tint) { this.emitter.step(frame, zones, tint); }
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
