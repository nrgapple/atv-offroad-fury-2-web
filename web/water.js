// Original WTT/PSM inputs and the Rust physics field supply this portable pass.
// Native PS2 VU tessellation/reflection coordinates and froth remain incomplete.
export const heightQueryShader = `uniform float baseHeight;
uniform int channelCount;
uniform vec2 origins[8], inverseWidths[8], widths[8];
uniform vec4 channels[8]; // dimension, scale, sign, layer
uniform int tiled[8];
uniform highp usampler2DArray heightMaps;
float heightAt(vec2 point) {
  float sum = 0.0;
  for (int i=0; i<8; i++) {
    if (i >= channelCount) break;
    vec2 local = point - origins[i];
    if (tiled[i] == 0 && (any(lessThan(local, vec2(0.0))) || any(greaterThan(local, widths[i])))) continue;
    uint dimension = uint(channels[i].x);
    // Native truncation then DIVU remainder. The shipped128 maps wrap identically
    // for negative signed inputs, without replacing integer sampling by filtering.
    uvec2 sampleIndex = uvec2(ivec2((inverseWidths[i] * channels[i].x) * local));
    ivec2 pixel = ivec2(sampleIndex % dimension);
    uint sampleValue = texelFetch(heightMaps, ivec3(pixel, int(channels[i].w)), 0).r;
    sum = sum + channels[i].z * (channels[i].y * float(sampleValue));
  }
  return baseHeight + sum * (1.0 / 4096.0);
}
`;
const vertexSource = `#version 300 es
precision highp float;
precision highp int;
layout(location=0) in vec2 gridPoint;
out vec3 waterPoint;
uniform mat4 viewProjection;
uniform vec3 eye, forward, right, up;
uniform vec2 projection, screenBounds;
uniform float farDistance;
${heightQueryShader}
void main() {
  vec2 screen = vec2(gridPoint.x, mix(screenBounds.x, screenBounds.y, gridPoint.y));
  vec3 direction = forward + right * screen.x * projection.x + up * screen.y * projection.y;
  float denominator = abs(direction.y) < 0.000001 ? (direction.y < 0.0 ? -0.000001 : 0.000001) : direction.y;
  float distance = clamp((baseHeight - eye.y) / denominator, 1.0, farDistance);
  vec3 point = eye + direction * distance;
  point.y = heightAt(point.xz);
  waterPoint = point;
  gl_Position = viewProjection * vec4(point, 1.0);
}`;
const fragmentSource = `#version 300 es
precision highp float;
precision highp int;
in vec3 waterPoint;
uniform mat4 viewProjection;
uniform vec3 eye, forward, right, up;
uniform vec2 projection;
uniform float farDistance;
uniform sampler2D environmentMap;
uniform highp sampler2DArray gradientMaps;
uniform int gradientCount;
uniform vec3 gradientWaves[4]; // prepared origin channel, atlas layer, GS FIX / 128
uniform vec3 environmentColor, fogColor;
uniform float fogDistance;
out vec4 color;
${heightQueryShader}
void main() {
  vec3 point = waterPoint;
  // Original Dx/Dz texture pairs and BumpScale supply the visual normals.
  // Reflection coordinates remain the portable mapping. No artistic Fresnel, generated tint or second wave clock.
  vec2 gradient = vec2(0.0);
  for (int i=0; i<4; i++) {
    if (i >= gradientCount) break;
    int channel = int(gradientWaves[i].x);
    vec2 local = point.xz - origins[channel];
    if (tiled[channel] == 0 && (any(lessThan(local, vec2(0.0))) || any(greaterThan(local, widths[channel])))) continue;
    vec4 derivatives = texture(gradientMaps, vec3(local * inverseWidths[channel], gradientWaves[i].y));
    gradient += vec2(derivatives.r - derivatives.g, derivatives.b - derivatives.a) * gradientWaves[i].z * (255.0 / 128.0);
  }
  vec3 normal = normalize(vec3(gradient.x, 1.0, gradient.y));
  vec3 reflected = reflect(normalize(point - eye), normal);
  vec2 environmentUV = reflected.xz * 0.5 + 0.5;
  vec4 sampled = texture(environmentMap, environmentUV);
  vec3 surface = sampled.rgb * environmentColor;
  float fog = 1.0 - exp(-pow(length(point - eye) / fogDistance, 2.0));
  color = vec4(mix(surface, fogColor, min(0.94, fog)), clamp(sampled.a * (255.0 / 128.0), 0.0, 1.0));
}`;
function compile(gl, kind, source) {
  const shader=gl.createShader(kind);gl.shaderSource(shader,source);gl.compileShader(shader);
  if (!gl.getShaderParameter(shader,gl.COMPILE_STATUS)) {const message=gl.getShaderInfoLog(shader);gl.deleteShader(shader);throw new Error(`Water shader: ${message}`);}
  return shader;
}
export class WaterRenderer {
  constructor(gl, source) {
    this.gl=gl;this.timer=gl.getExtension('EXT_disjoint_timer_query_webgl2');this.timerPending=[];this.timerFree=[];this.gpuMilliseconds=null;this.cpuMilliseconds=0;this.material=source.material;this.frame=source.frame;this.gpuBytes=0;this.draws=0;
    const height=source.height;
    if (!height?.textures?.length || height.channels.length>8 || !source.environment) throw new Error('Invalid original water render inputs.');
    const dimension=height.textures[0].dimension;
    if (height.textures.some(t=>t.dimension!==dimension || t.pixels.length!==dimension*dimension)) throw new Error('Invalid water height atlas.');
    this.channelCount=height.channels.length;
    this.gradientWaves=new Float32Array(12);this.origins=new Float32Array(16);this.inverseWidths=new Float32Array(16);this.widths=new Float32Array(16);this.channels=new Float32Array(32);this.tiled=new Int32Array(8);
    height.channels.forEach((channel,i)=>{this.inverseWidths.set(channel.inverse_width,i*2);this.widths.set(channel.width,i*2);this.channels.set([channel.dimension,channel.scale,channel.positive?1:-1,channel.layer],i*4);this.tiled[i]=Number(channel.tiled);});
    const vertex=compile(gl,gl.VERTEX_SHADER,vertexSource),fragment=compile(gl,gl.FRAGMENT_SHADER,fragmentSource);
    this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);gl.deleteShader(vertex);gl.deleteShader(fragment);
    if (!gl.getProgramParameter(this.program,gl.LINK_STATUS)) throw new Error(`Water program: ${gl.getProgramInfoLog(this.program)}`);
    this.uniforms=Object.fromEntries(['viewProjection','eye','forward','right','up','projection','screenBounds','baseHeight','farDistance','channelCount','origins[0]','inverseWidths[0]','widths[0]','channels[0]','tiled[0]','heightMaps','environmentMap','gradientMaps','gradientCount','gradientWaves[0]','environmentColor','fogColor','fogDistance'].map(name=>[name,gl.getUniformLocation(this.program,name)]));
    this.vao=gl.createVertexArray();this.vertices=gl.createBuffer();this.indices=gl.createBuffer();
    // 00187bc0 quality1:334 projected subdivisions and3 stripgroups.
    // Portable topology differs from native VU generation; buffers remain static.
    const columns=335, rows=3, grid=new Float32Array(columns*(rows+1)*2), indices=new Uint16Array((columns-1)*rows*6);
    for(let y=0;y<=rows;y++)for(let x=0;x<columns;x++)grid.set([x/(columns-1)*2-1,y/rows],(y*columns+x)*2);
    let at=0;for(let y=0;y<rows;y++)for(let x=0;x<columns-1;x++){const a=y*columns+x,b=a+1,c=a+columns,d=c+1;indices.set([a,c,b,b,c,d],at);at+=6;}
    gl.bindVertexArray(this.vao);gl.bindBuffer(gl.ARRAY_BUFFER,this.vertices);gl.bufferData(gl.ARRAY_BUFFER,grid,gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,2,gl.FLOAT,false,8,0);gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,this.indices);gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,indices,gl.STATIC_DRAW);
    this.indexCount=indices.length;this.meshBytes=grid.byteLength+indices.byteLength;
    this.heightMaps=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D_ARRAY,this.heightMaps);
    const pixels=new Uint8Array(height.textures.length*dimension*dimension);height.textures.forEach((t,i)=>pixels.set(t.pixels,i*dimension*dimension));
    gl.texImage3D(gl.TEXTURE_2D_ARRAY,0,gl.R8UI,dimension,dimension,height.textures.length,0,gl.RED_INTEGER,gl.UNSIGNED_BYTE,pixels);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    this.environment=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,this.environment);
    const image=source.environment;
    if (image.rgba.length!==image.width*image.height*4) throw new Error('Invalid original water environment texture.');
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,image.width,image.height,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array(image.rgba));
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    const gradients=source.gradients;
    if (!gradients?.textures?.length || gradients.waves.length>4) throw new Error('Missing original water derivative signatures.');
    const normalDimension=gradients.textures[0].dimension;
    if (gradients.textures.some(t=>t.dimension!==normalDimension || t.pixels.length!==normalDimension*normalDimension*4)) throw new Error('Invalid water derivative atlas.');
    this.gradientCount=gradients.waves.length;
    gradients.waves.forEach((wave,i)=>this.gradientWaves.set([wave.origin_channel,wave.layer,wave.fixed_alpha/128],i*3));
    const normalPixels=new Uint8Array(gradients.textures.length*normalDimension*normalDimension*4);
    gradients.textures.forEach((t,i)=>normalPixels.set(t.pixels,i*normalDimension*normalDimension*4));
    this.gradientMaps=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D_ARRAY,this.gradientMaps);
    gl.texImage3D(gl.TEXTURE_2D_ARRAY,0,gl.RGBA8,normalDimension,normalDimension,gradients.textures.length,0,gl.RGBA,gl.UNSIGNED_BYTE,normalPixels);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_WRAP_S,gl.REPEAT);gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_WRAP_T,gl.REPEAT);
    this.gpuBytes=this.meshBytes+pixels.byteLength+image.rgba.length+normalPixels.byteLength;this.accept(source.frame);
  }
  accept(frame) {
    if (!frame || !Number.isFinite(frame.base_height) || frame.origins?.length!==this.channelCount || frame.origins.some(origin=>origin.length!==2 || !origin.every(Number.isFinite))) throw new Error('Invalid prepared shared water frame.');
    this.frame=frame;frame.origins.forEach((origin,i)=>this.origins.set(origin,i*2));
  }
  draw({camera,viewProjection,focalLength,aspect,farDistance,fogColor,fogDistance}) {
    const gl=this.gl,u=this.uniforms;this.draws=0;
    if (!this.frame) return;
    const started=performance.now();
    for (let i=this.timerPending.length-1;i>=0;i--) {
      const query=this.timerPending[i];
      if (gl.getQueryParameter(query,gl.QUERY_RESULT_AVAILABLE)) {
        if (!gl.getParameter(this.timer.GPU_DISJOINT_EXT)) this.gpuMilliseconds=gl.getQueryParameter(query,gl.QUERY_RESULT)/1e6;
        this.timerPending.splice(i,1);this.timerFree.push(query);
      }
    }
    let timerQuery;
    if (this.timer && this.timerPending.length<4) {
      timerQuery=this.timerFree.pop() ?? gl.createQuery();
      gl.beginQuery(this.timer.TIME_ELAPSED_EXT,timerQuery);
    }
    gl.useProgram(this.program);gl.bindVertexArray(this.vao);
    gl.uniformMatrix4fv(u.viewProjection,false,viewProjection);gl.uniform3f(u.eye,camera.x,camera.y,camera.z);
    const cy=Math.cos(camera.yaw),sy=Math.sin(camera.yaw),cp=Math.cos(camera.pitch),sp=Math.sin(camera.pitch);
    gl.uniform3f(u.forward,sy*cp,sp,cy*cp);gl.uniform3f(u.right,-cy,0,sy);gl.uniform3f(u.up,-sy*sp,cp,-cy*sp);
    const difference=this.frame.base_height-camera.y;
    const boundary=(difference/farDistance-sp)*focalLength/cp;
    const minimum=difference<0?-1:Math.max(-1,Math.min(1,boundary));
    const maximum=difference<0?Math.max(-1,Math.min(1,boundary)):1;
    gl.uniform2f(u.screenBounds,minimum,maximum);
    gl.uniform2f(u.projection,aspect/focalLength,1/focalLength);gl.uniform1f(u.baseHeight,this.frame.base_height);gl.uniform1f(u.farDistance,farDistance);
    gl.uniform1i(u.channelCount,this.channelCount);gl.uniform2fv(u['origins[0]'],this.origins);gl.uniform2fv(u['inverseWidths[0]'],this.inverseWidths);gl.uniform2fv(u['widths[0]'],this.widths);gl.uniform4fv(u['channels[0]'],this.channels);gl.uniform1iv(u['tiled[0]'],this.tiled);
    gl.uniform3fv(u.environmentColor,this.material.environment_color);gl.uniform3fv(u.fogColor,fogColor);gl.uniform1f(u.fogDistance,fogDistance);
    gl.activeTexture(gl.TEXTURE4);gl.bindTexture(gl.TEXTURE_2D_ARRAY,this.heightMaps);gl.uniform1i(u.heightMaps,4);
    gl.activeTexture(gl.TEXTURE5);gl.bindTexture(gl.TEXTURE_2D,this.environment);gl.uniform1i(u.environmentMap,5);
    gl.activeTexture(gl.TEXTURE6);gl.bindTexture(gl.TEXTURE_2D_ARRAY,this.gradientMaps);gl.uniform1i(u.gradientMaps,6);
    gl.uniform1i(u.gradientCount,this.gradientCount);gl.uniform3fv(u['gradientWaves[0]'],this.gradientWaves);
    gl.depthMask(this.material.environment_depth_write);
    if (this.material.environment_blend) {gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);}
    gl.drawElements(gl.TRIANGLES,this.indexCount,gl.UNSIGNED_SHORT,0);this.draws=1;
    gl.disable(gl.BLEND);gl.depthMask(true);gl.activeTexture(gl.TEXTURE0);
    if (timerQuery) {gl.endQuery(this.timer.TIME_ELAPSED_EXT);this.timerPending.push(timerQuery);}
    this.cpuMilliseconds=performance.now()-started;
  }
  dispose() {
    const gl=this.gl;for (const query of [...this.timerPending,...this.timerFree]) gl.deleteQuery(query);gl.deleteTexture(this.heightMaps);gl.deleteTexture(this.environment);gl.deleteTexture(this.gradientMaps);gl.deleteBuffer(this.vertices);gl.deleteBuffer(this.indices);gl.deleteVertexArray(this.vao);gl.deleteProgram(this.program);this.frame=null;
  }
}
