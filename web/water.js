// Original WTT/PSM inputs and the Rust physics field supply this portable pass.
// Native PS2 VU tessellation/reflection coordinates and froth remain incomplete.
import { fogVertexShader, fogFragmentShader } from './fog.js';
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
${fogVertexShader}
void main() {
  vec2 screen = vec2(gridPoint.x, mix(screenBounds.x, screenBounds.y, gridPoint.y));
  vec3 direction = forward + right * screen.x * projection.x + up * screen.y * projection.y;
  float denominator = abs(direction.y) < 0.000001 ? (direction.y < 0.0 ? -0.000001 : 0.000001) : direction.y;
  float distance = clamp((baseHeight - eye.y) / denominator, 1.0, farDistance);
  vec3 point = eye + direction * distance;
  point.y = heightAt(point.xz);
  waterPoint = point;
  gl_Position = viewProjection * vec4(point, 1.0);
  prepareFog(length(point - eye), gl_Position);
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
uniform vec4 gradientWaves[4]; // prepared origin channel, first atlas layer, GS FIX / 128
uniform vec2 gradientSecond[4]; // second atlas layer, GS FIX / 128
uniform vec3 environmentColor, fogColor;
out vec4 color;
${heightQueryShader}
${fogFragmentShader}
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
    vec4 derivatives = texture(gradientMaps, vec3(local * inverseWidths[channel] * gradientWaves[i].w, gradientWaves[i].y));
    gradient += vec2(derivatives.r - derivatives.g, derivatives.b - derivatives.a) * gradientWaves[i].z * (255.0 / 128.0);
    if (gradientSecond[i].y != 0.0) {
      derivatives = texture(gradientMaps, vec3(local * inverseWidths[channel] * gradientWaves[i].w, gradientSecond[i].x));
      gradient += vec2(derivatives.r - derivatives.g, derivatives.b - derivatives.a) * gradientSecond[i].y * (255.0 / 128.0);
    }
  }
  vec3 normal = normalize(vec3(gradient.x, 1.0, gradient.y));
  vec3 reflected = reflect(normalize(point - eye), normal);
  vec2 environmentUV = reflected.xz * 0.5 + 0.5;
  vec4 sampled = texture(environmentMap, environmentUV);
  vec3 surface = sampled.rgb * environmentColor;
  color = vec4(mix(fogColor, surface, fogCoverage()), clamp(sampled.a * (255.0 / 128.0), 0.0, 1.0));
}`;
function filteredTextureBytes(width,height,layers=1){
  let bytes=0;for(;;){bytes+=width*height*layers*4;if(width===1&&height===1)return bytes;width=Math.max(1,width>>1);height=Math.max(1,height>>1);}
}
function compile(gl, kind, source) {
  const shader=gl.createShader(kind);gl.shaderSource(shader,source);gl.compileShader(shader);
  if (!gl.getShaderParameter(shader,gl.COMPILE_STATUS)) {const message=gl.getShaderInfoLog(shader);gl.deleteShader(shader);throw new Error(`Water shader: ${message}`);}
  return shader;
}
//00190aa0 scales strip groups by viewport width and dense rows by height.
//Each native strip adds19 samples to the row stride, not three depth rows.
//Portable triangulation stays bounded below Uint16 capacity on tall/wide screens.
export function projectedGridSize(width,height,vertexBudget=24000){
  if(!Number.isFinite(width)||!Number.isFinite(height)||width<=0||height<=0||!Number.isInteger(vertexBudget)||vertexBudget<1400||vertexBudget>24000)throw new Error('Invalid water viewport.');
  const strips=Math.max(1,Math.min(8,Math.floor(3*width/640+.5))),columns=strips*19+1;
  const requestedRows=Math.max(8,Math.min(1024,Math.floor(334*height/448+.5)));
  const rows=Math.min(requestedRows,Math.floor(vertexBudget/columns)-1);
  return{columns,rows};
}
export function projectedGrid(columns,rows){
  const grid=new Float32Array(columns*(rows+1)*2),indices=new Uint16Array((columns-1)*rows*6);
  for(let y=0;y<=rows;y++)for(let x=0;x<columns;x++)grid.set([x/(columns-1)*2-1,y/rows],(y*columns+x)*2);
  let at=0;for(let y=0;y<rows;y++)for(let x=0;x<columns-1;x++){const a=y*columns+x,b=a+1,c=a+columns,d=c+1;indices.set([a,c,b,b,c,d],at);at+=6;}
  return{grid,indices};
}
export class WaterRenderer {
  constructor(gl, source) {
    this.gl=gl;this.timer=gl.getExtension('EXT_disjoint_timer_query_webgl2');this.timerPending=[];this.timerFree=[];this.gpuMilliseconds=null;this.cpuMilliseconds=0;this.material=source.material;this.frame=source.frame;this.gpuBytes=0;this.draws=0;
    const height=source.height;
    if (!height?.textures?.length || height.channels.length>8 || !source.environment) throw new Error('Invalid original water render inputs.');
    const dimension=Math.max(...height.textures.map(texture=>texture.dimension));
    if (height.textures.some(t=>t.dimension<1 || dimension%t.dimension!==0 || t.pixels.length!==t.dimension*t.dimension)) throw new Error('Invalid water height atlas.');
    if(height.textures.length>gl.getParameter(gl.MAX_ARRAY_TEXTURE_LAYERS)) throw new Error('Water height atlas exceeds GPU layer capacity.');this.channelCount=height.channels.length;
    this.gradientSource=source.gradients;this.heightSource=height;this.gradientWaves=new Float32Array(16);this.gradientSecond=new Float32Array(8);this.origins=new Float32Array(16);this.inverseWidths=new Float32Array(16);this.widths=new Float32Array(16);this.channels=new Float32Array(32);this.tiled=new Int32Array(8);
    height.channels.forEach((channel,i)=>{this.inverseWidths.set(channel.inverse_width,i*2);this.widths.set(channel.width,i*2);this.channels.set([channel.dimension,channel.scale,channel.positive?1:-1,channel.layer],i*4);this.tiled[i]=Number(channel.tiled);});
    const vertex=compile(gl,gl.VERTEX_SHADER,vertexSource),fragment=compile(gl,gl.FRAGMENT_SHADER,fragmentSource);
    this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);gl.deleteShader(vertex);gl.deleteShader(fragment);
    if (!gl.getProgramParameter(this.program,gl.LINK_STATUS)) throw new Error(`Water program: ${gl.getProgramInfoLog(this.program)}`);
    this.uniforms=Object.fromEntries(['viewProjection','eye','forward','right','up','projection','screenBounds','baseHeight','farDistance','channelCount','origins[0]','inverseWidths[0]','widths[0]','channels[0]','tiled[0]','heightMaps','environmentMap','gradientMaps','gradientCount','gradientWaves[0]','gradientSecond[0]','environmentColor','fogColor','fogPacking','fogEnabled'].map(name=>[name,gl.getUniformLocation(this.program,name)]));
    this.vao=gl.createVertexArray();this.vertices=gl.createBuffer();this.indices=gl.createBuffer();
    gl.bindVertexArray(this.vao);gl.bindBuffer(gl.ARRAY_BUFFER,this.vertices);
    gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,2,gl.FLOAT,false,8,0);gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,this.indices);
    this.meshBytes=0;this.gridColumns=0;this.gridRows=0;this.resizeGrid(gl.drawingBufferWidth,gl.drawingBufferHeight);
    this.heightMaps=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D_ARRAY,this.heightMaps);
    const pixels=new Uint8Array(height.textures.length*dimension*dimension);height.textures.forEach((t,i)=>{for(let y=0;y<dimension;y++)for(let x=0;x<dimension;x++)pixels[i*dimension*dimension+y*dimension+x]=t.pixels[(y%t.dimension)*t.dimension+x%t.dimension];});
    gl.texImage3D(gl.TEXTURE_2D_ARRAY,0,gl.R8UI,dimension,dimension,height.textures.length,0,gl.RED_INTEGER,gl.UNSIGNED_BYTE,pixels);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    this.environment=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,this.environment);
    const image=source.environment;
    if (image.rgba.length!==image.width*image.height*4) throw new Error('Invalid original water environment texture.');
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,image.width,image.height,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array(image.rgba));
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR_MIPMAP_LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    const gradients=source.gradients;
    if (!gradients?.textures?.length || gradients.waves.length>4) throw new Error('Missing original water derivative signatures.');
    const normalDimension=Math.max(...gradients.textures.map(texture=>texture.dimension));this.normalDimension=normalDimension;
    if (gradients.textures.some(t=>t.dimension<1 || normalDimension%t.dimension!==0 || t.pixels.length!==t.dimension*t.dimension*4)) throw new Error('Invalid water derivative atlas.');
    if(gradients.textures.length>gl.getParameter(gl.MAX_ARRAY_TEXTURE_LAYERS)) throw new Error('Water normal atlas exceeds GPU layer capacity.');this.gradientCount=gradients.waves.length;

    const normalPixels=new Uint8Array(gradients.textures.length*normalDimension*normalDimension*4);
    gradients.textures.forEach((t,i)=>{for(let y=0;y<normalDimension;y++)for(let x=0;x<normalDimension;x++){const from=((y%t.dimension)*t.dimension+x%t.dimension)*4,to=(i*normalDimension*normalDimension+y*normalDimension+x)*4;normalPixels[to]=t.pixels[from];normalPixels[to+1]=t.pixels[from+1];normalPixels[to+2]=t.pixels[from+2];normalPixels[to+3]=t.pixels[from+3];}});
    this.gradientMaps=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D_ARRAY,this.gradientMaps);
    gl.texImage3D(gl.TEXTURE_2D_ARRAY,0,gl.RGBA8,normalDimension,normalDimension,gradients.textures.length,0,gl.RGBA,gl.UNSIGNED_BYTE,normalPixels);
    gl.generateMipmap(gl.TEXTURE_2D_ARRAY);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_MIN_FILTER,gl.LINEAR_MIPMAP_LINEAR);gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_WRAP_S,gl.REPEAT);gl.texParameteri(gl.TEXTURE_2D_ARRAY,gl.TEXTURE_WRAP_T,gl.REPEAT);
    this.textureBytes=pixels.byteLength+filteredTextureBytes(image.width,image.height)+filteredTextureBytes(normalDimension,normalDimension,gradients.textures.length);this.gpuBytes=this.meshBytes+this.textureBytes;this.accept(source.frame);
  }
  resizeGrid(width,height){
    //Portrait keeps more depth samples than width samples with bounded fetch work.
    const {columns,rows}=projectedGridSize(width,height,height>width?8000:12000);
    if(columns===this.gridColumns&&rows===this.gridRows)return;
    const gl=this.gl,{grid,indices}=projectedGrid(columns,rows);
    gl.bindVertexArray(this.vao);gl.bindBuffer(gl.ARRAY_BUFFER,this.vertices);gl.bufferData(gl.ARRAY_BUFFER,grid,gl.STATIC_DRAW);gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,this.indices);gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,indices,gl.STATIC_DRAW);
    this.gridColumns=columns;this.gridRows=rows;this.indexCount=indices.length;this.meshBytes=grid.byteLength+indices.byteLength;this.gpuBytes=this.meshBytes+(this.textureBytes??0);
  }
  accept(frame) {
    if (!frame || !Number.isFinite(frame.base_height) || frame.origins?.length!==this.channelCount || frame.origins.some(origin=>origin.length!==2 || !origin.every(Number.isFinite))) throw new Error('Invalid prepared shared water frame.');
    this.frame=frame;frame.origins.forEach((origin,i)=>this.origins.set(origin,i*2));
    this.heightSource.channels.forEach((channel,i)=>{
      const pair=frame.signature_frames?.[i] ?? [0,0], layers=channel.layers ?? [channel.layer];
      if (!pair.every(index=>Number.isInteger(index)&&index>=0&&index<layers.length)) throw new Error('Invalid shared water animation frame.');
      //Physics uses its explicitly retained shared blend-clock weight1/0.
      //The shader selects the actual first frame, without regenerating an atlas.
      this.channels[i*4+3]=layers[pair[0]];
    });
    this.gradientSource.waves.forEach((wave,i)=>{
      const pair=frame.signature_frames?.[wave.origin_channel] ?? [0,0], layers=wave.layers ?? [wave.layer];
      const weight=frame.first_frame_weights?.[wave.origin_channel] ?? 1;
      if (!pair.every(index=>Number.isInteger(index)&&index>=0&&index<layers.length)||!Number.isFinite(weight)||weight<0||weight>1) throw new Error('Invalid shared water gradient frame.');
      //00197980 rounds GS FIX independently for each original frame pass.
      const first=Math.trunc(128*wave.scale*weight+.5)/128,second=pair[0]===pair[1]?0:Math.trunc(128*wave.scale*(1-weight)+.5)/128;
      this.gradientWaves.set([wave.origin_channel,layers[pair[0]],first,this.gradientSource.textures[layers[pair[0]]].dimension/this.normalDimension],i*4);
      this.gradientSecond.set([layers[pair[1]],second],i*2);
    });
  }
  draw({camera,viewProjection,focalLength,aspect,farDistance,fogColor,fog}) {
    const gl=this.gl,u=this.uniforms;this.draws=0;
    if (!this.frame || gl.isContextLost() || !gl.drawingBufferWidth || !gl.drawingBufferHeight) return;
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
    this.resizeGrid(gl.drawingBufferWidth,gl.drawingBufferHeight);
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
    gl.uniform3fv(u.environmentColor,this.material.environment_color);gl.uniform3fv(u.fogColor,fogColor);
    gl.uniform2f(u.fogPacking,fog?.encoded_bias??524288,fog?.coefficient??0);gl.uniform1i(u.fogEnabled,Boolean(fog)&&this.material.environment_fog);
    gl.activeTexture(gl.TEXTURE4);gl.bindTexture(gl.TEXTURE_2D_ARRAY,this.heightMaps);gl.uniform1i(u.heightMaps,4);
    gl.activeTexture(gl.TEXTURE5);gl.bindTexture(gl.TEXTURE_2D,this.environment);gl.uniform1i(u.environmentMap,5);
    gl.activeTexture(gl.TEXTURE6);gl.bindTexture(gl.TEXTURE_2D_ARRAY,this.gradientMaps);gl.uniform1i(u.gradientMaps,6);
    gl.uniform1i(u.gradientCount,this.gradientCount);gl.uniform4fv(u['gradientWaves[0]'],this.gradientWaves);gl.uniform2fv(u['gradientSecond[0]'],this.gradientSecond);
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
