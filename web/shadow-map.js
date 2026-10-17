// High preset: a sun shadow map whose only casters are vehicles and riders
// (player, CPU racers, remote riders). Terrain and scenery receive it through
// the main shader. Authored; the original renderer casts no vehicle shadows.
export const SHADOW_RADIUS = 24, SHADOW_DEPTH = 160;

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const unit = v => { const l = Math.hypot(...v) || 1; return v.map(x => x / l); };

// Orthographic light view-projection around `center`, looking along the light
// (opposite `toSun`). The centre snaps to whole shadow texels so the edges of
// a moving box do not shimmer. Returns the clip matrix and its [0,1] texture form.
export function lightMatrices(center, toSun, size, radius = SHADOW_RADIUS, depth = SHADOW_DEPTH) {
  let sun = unit(toSun);
  if (!(sun[1] > .2)) sun = unit([sun[0], .2, sun[2]]);
  const forward = sun.map(v => -v);
  const right = unit(cross(forward, Math.abs(forward[1]) > .99 ? [0, 0, 1] : [0, 1, 0]));
  const up = cross(right, forward);
  const texel = 2 * radius / size;
  const snap = v => Math.round(v / texel) * texel;
  const cr = snap(dot(center, right)), cu = snap(dot(center, up)), cf = dot(center, forward);
  const at = [0, 1, 2].map(i => right[i] * cr + up[i] * cu + forward[i] * cf);
  const eye = at.map((v, i) => v - forward[i] * depth / 2);
  const view = [right[0], up[0], -forward[0], 0, right[1], up[1], -forward[1], 0, right[2], up[2], -forward[2], 0, -dot(right, eye), -dot(up, eye), dot(forward, eye), 1];
  const near = 0, far = depth, s = 1 / radius;
  const projection = [s, 0, 0, 0, 0, s, 0, 0, 0, 0, -2 / (far - near), 0, 0, 0, -(far + near) / (far - near), 1];
  const multiply = (a, b) => { const r = new Float32Array(16); for (let c = 0; c < 4; c++) for (let row = 0; row < 4; row++) for (let k = 0; k < 4; k++) r[c * 4 + row] += a[k * 4 + row] * b[c * 4 + k]; return r; };
  const clip = multiply(projection, view);
  const bias = [.5, 0, 0, 0, 0, .5, 0, 0, 0, 0, .5, 0, .5, .5, .5, 1];
  return { clip, texture: multiply(bias, clip), sun: sub([0, 0, 0], forward) };
}

// GLSL for the main fragment shader. `vehicleShadowFactor` is 1 outside the box.
export const shadowReceiverGlsl = `
uniform bool shadowReceive;
uniform highp sampler2DShadow vehicleShadow;
uniform mat4 shadowMatrix;
float vehicleShadowFactor(vec3 p) {
  vec4 c = shadowMatrix * vec4(p, 1.0);
  vec3 s = c.xyz / c.w;
  float edge = min(min(s.x, s.y), min(1.0 - s.x, 1.0 - s.y));
  if (edge <= 0.0 || s.z >= 1.0) return 1.0;
  float texel = 1.0 / float(textureSize(vehicleShadow, 0).x);
  float lit = 0.0;
  for (int i = 0; i < 4; i++) {
    vec2 o = vec2(i == 1 || i == 3 ? 1.0 : -1.0, i >= 2 ? 1.0 : -1.0) * texel;
    lit += texture(vehicleShadow, vec3(s.xy + o, s.z - 0.0015));
  }
  return mix(1.0, mix(0.55, 1.0, lit * 0.25), smoothstep(0.0, 0.08, edge));
}`;

export class VehicleShadowMap {
  constructor(gl, size) {
    this.gl = gl; this.size = size; this.draws = 0;
    const compile = (type, source) => { const s = gl.createShader(type); gl.shaderSource(s, source); gl.compileShader(s); if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s)); return s; };
    const vertex = compile(gl.VERTEX_SHADER, `#version 300 es
layout(location=0) in vec3 position;
layout(location=3) in float quadNode;
uniform mat4 lightViewProjection, model;
uniform mat4 quadBones[32];
void main(){ gl_Position = lightViewProjection * (model * (quadBones[int(quadNode)] * vec4(position, 1.0))); }`);
    const fragment = compile(gl.FRAGMENT_SHADER, `#version 300 es
precision lowp float;
void main(){}`);
    const program = this.program = gl.createProgram(); gl.attachShader(program, vertex); gl.attachShader(program, fragment); gl.linkProgram(program);
    gl.deleteShader(vertex); gl.deleteShader(fragment);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
    this.uniforms = { lightViewProjection: gl.getUniformLocation(program, 'lightViewProjection'), model: gl.getUniformLocation(program, 'model'), quadBones: gl.getUniformLocation(program, 'quadBones[0]') };
    this.texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.DEPTH_COMPONENT24, size, size);
    for (const [k, v] of [[gl.TEXTURE_MIN_FILTER, gl.LINEAR], [gl.TEXTURE_MAG_FILTER, gl.LINEAR], [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE], [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE], [gl.TEXTURE_COMPARE_MODE, gl.COMPARE_REF_TO_TEXTURE], [gl.TEXTURE_COMPARE_FUNC, gl.LEQUAL]]) gl.texParameteri(gl.TEXTURE_2D, k, v);
    this.framebuffer = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.framebuffer);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.TEXTURE_2D, this.texture, 0);
    const complete = gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    if (!complete) { this.dispose(); throw new Error('Vehicle shadow map framebuffer is incomplete.'); }
    this.gpuBytes = size * size * 4;
  }
  // casters: [{ chunk, count, model, bones }] using the main mesh VAOs.
  render(casters, clip, viewport) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.framebuffer);
    gl.viewport(0, 0, this.size, this.size);
    gl.clear(gl.DEPTH_BUFFER_BIT);
    gl.useProgram(this.program); gl.uniformMatrix4fv(this.uniforms.lightViewProjection, false, clip);
    gl.enable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(2, 4);
    let draws = 0, model = null, bones = null;
    for (const caster of casters) {
      if (caster.model !== model) gl.uniformMatrix4fv(this.uniforms.model, false, model = caster.model);
      if (caster.bones !== bones) gl.uniformMatrix4fv(this.uniforms.quadBones, false, bones = caster.bones);
      gl.bindVertexArray(caster.chunk.vao); gl.drawElements(gl.TRIANGLES, caster.count, gl.UNSIGNED_SHORT, 0); draws++;
    }
    gl.disable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(0, 0);
    gl.bindVertexArray(null);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(...viewport);
    this.draws = draws;
    return draws;
  }
  dispose() { const gl = this.gl; gl.deleteFramebuffer(this.framebuffer); gl.deleteTexture(this.texture); gl.deleteProgram(this.program); }
}
