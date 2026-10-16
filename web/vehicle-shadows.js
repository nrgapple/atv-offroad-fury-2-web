// Enhanced/High soft contact shadows: one ground-conforming elliptical blob per
// vehicle or detached rider, all submitted in a single draw. Authored; the
// original renderer has no vehicle shadows. Presentation only.
const GRID = 7, CELLS = GRID - 1, FLOATS = 6;

// Footprint for one caster from its world matrix (column-major, unit basis).
// `lift` is the body's clearance above its resting height; the blob softens,
// spreads and fades as it rises, and is gone by about 12 feet.
export function shadowFootprint({ matrix, length, width, lift = 0, distance = 0, strength = .6 }) {
  const fx = matrix[8], fz = matrix[10], rx = matrix[0], rz = matrix[2];
  const f = Math.hypot(fx, fz), r = Math.hypot(rx, rz);
  // Viewed side-on (rolled or looped) the horizontal axes shrink; use the
  // longer one and keep the blob round rather than degenerate.
  const forward = f > .2 ? [fx / f, fz / f] : r > .2 ? [-rz / r, rx / r] : [0, 1];
  const right = [forward[1], -forward[0]];
  const height = Math.max(0, lift);
  const spread = 1 + Math.min(height, 12) * .06;
  const fade = Math.max(0, 1 - height / 12) * Math.max(0, Math.min(1, (240 - distance) / 90));
  return { center: [matrix[12], matrix[14]], forward, right, halfLength: Math.max(.5, length * .66) * spread, halfWidth: Math.max(.5, width * .8) * spread, alpha: strength * fade * fade };
}

export class ContactShadows {
  constructor(gl, capacity = 12) {
    this.gl = gl; this.capacity = capacity; this.draws = 0; this.casters = 0;
    const compile = (type, source) => { const s = gl.createShader(type); gl.shaderSource(s, source); gl.compileShader(s); if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s)); return s; };
    const vertex = compile(gl.VERTEX_SHADER, `#version 300 es
layout(location=0) in vec3 position;
layout(location=1) in vec3 shade;
uniform mat4 viewProjection;
out vec3 v;
void main(){ v = shade; gl_Position = viewProjection * vec4(position, 1.0); }`);
    const fragment = compile(gl.FRAGMENT_SHADER, `#version 300 es
precision mediump float;
in vec3 v;
out vec4 color;
void main(){
  float r = length(v.xy);
  float a = v.z * (1.0 - smoothstep(.4, 1.0, r));
  color = vec4(0.0, 0.0, 0.0, a * a * (3.0 - 2.0 * a));
}`);
    const program = this.program = gl.createProgram(); gl.attachShader(program, vertex); gl.attachShader(program, fragment); gl.linkProgram(program);
    gl.deleteShader(vertex); gl.deleteShader(fragment);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
    this.viewProjection = gl.getUniformLocation(program, 'viewProjection');
    this.vertices = new Float32Array(capacity * GRID * GRID * FLOATS);
    const indices = new Uint16Array(capacity * CELLS * CELLS * 6);
    for (let c = 0, at = 0; c < capacity; c++) for (let z = 0; z < CELLS; z++) for (let x = 0; x < CELLS; x++) {
      const a = c * GRID * GRID + z * GRID + x, b = a + 1, d = a + GRID, e = d + 1;
      indices.set([a, d, b, b, d, e], at); at += 6;
    }
    this.vao = gl.createVertexArray(); this.buffer = gl.createBuffer(); this.indices = gl.createBuffer();
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer); gl.bufferData(gl.ARRAY_BUFFER, this.vertices.byteLength, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, FLOATS * 4, 0);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 3, gl.FLOAT, false, FLOATS * 4, 12);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.indices); gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    this.gpuBytes = this.vertices.byteLength + indices.byteLength;
  }
  // `height(x, z)` is the visible terrain the blob drapes over.
  draw({ viewProjection, footprints, height }) {
    const gl = this.gl;
    let count = 0, at = 0;
    for (const p of footprints) {
      if (count >= this.capacity || !(p.alpha > .01)) continue;
      for (let j = 0; j < GRID; j++) for (let i = 0; i < GRID; i++) {
        const u = i / CELLS * 2 - 1, v = j / CELLS * 2 - 1;
        const x = p.center[0] + p.right[0] * u * p.halfWidth + p.forward[0] * v * p.halfLength;
        const z = p.center[1] + p.right[1] * u * p.halfWidth + p.forward[1] * v * p.halfLength;
        const y = height(x, z);
        this.vertices.set([x, Number.isFinite(y) ? y + .05 : -1e6, z, u, v, p.alpha], at); at += FLOATS;
      }
      count++;
    }
    this.casters = count; this.draws = 0;
    if (!count) return 0;
    gl.useProgram(this.program); gl.uniformMatrix4fv(this.viewProjection, false, viewProjection);
    gl.bindVertexArray(this.vao); gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer); gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.vertices, 0, at);
    gl.enable(gl.BLEND); gl.blendFunc(gl.ZERO, gl.ONE_MINUS_SRC_ALPHA); gl.depthMask(false); gl.depthFunc(gl.LEQUAL);
    gl.enable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(-1, -2);
    gl.drawElements(gl.TRIANGLES, count * CELLS * CELLS * 6, gl.UNSIGNED_SHORT, 0);
    gl.disable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(0, 0); gl.depthFunc(gl.LESS); gl.depthMask(true); gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
    this.draws = 1;
    return 1;
  }
  dispose() {
    const gl = this.gl;
    gl.deleteVertexArray(this.vao); gl.deleteBuffer(this.buffer); gl.deleteBuffer(this.indices); gl.deleteProgram(this.program);
  }
}
