import { cameraAxes } from './render-support.js';
// Static source type11 birth packets. The authoritative Rust clock ages a
// bounded 400x12 ring; only new packets cross the CPU/GPU boundary.
import {fogVertexShader,fogFragmentShader} from './fog.js';
export const STREAM_CAPACITY=400*12,STREAM_STRIDE=20;
const vertexSource=`#version 300 es
precision highp float;
layout(location=0) in vec2 corner;
layout(location=1) in vec4 pointSize;
layout(location=2) in vec4 velocityBirth;
layout(location=3) in vec4 lifeGrowthAlpha;
layout(location=4) in vec4 rectangle;
layout(location=5) in vec4 colorFloor;
uniform mat4 viewProjection;
uniform vec3 right,up,eye;
uniform float now,farDistance;
out vec2 texcoord;
out vec4 modulation;
${fogVertexShader}
void main(){
  float age=now-velocityBirth.w,life=lifeGrowthAlpha.x;
  vec3 center=pointSize.xyz+velocityBirth.xyz*age+vec3(0.0,-16.1,0.0)*age*age;
  center.y=max(center.y,colorFloor.w);
  float size=max(0.0,pointSize.w+lifeGrowthAlpha.y*age);
  vec3 point=center+(right*corner.x+up*corner.y)*size;
  gl_Position=viewProjection*vec4(point,1.0);
  texcoord=mix(rectangle.xy,rectangle.zw,corner*.5+.5);
  modulation=vec4(colorFloor.rgb/128.0,mix(lifeGrowthAlpha.z,lifeGrowthAlpha.w,clamp(1.0-age/max(life,.000001),0.0,1.0))/128.0);
  prepareFog(length(point-eye),gl_Position);
  if(life<=0.0||age<0.0||age>life||distance(center,eye)>farDistance+size)gl_Position=vec4(2.0,2.0,2.0,1.0);
}`;
const fragmentSource=`#version 300 es
precision highp float;
in vec2 texcoord;
in vec4 modulation;
uniform sampler2D ambientStreamAtlas;
uniform vec3 fogColor;
out vec4 color;
${fogFragmentShader}
void main(){vec4 sampled=texture(ambientStreamAtlas,texcoord);color=vec4(mix(fogColor,sampled.rgb*modulation.rgb,fogCoverage()),clamp(sampled.a*(255.0/128.0)*modulation.a,0.0,1.0));}`;

export function streamState(){return {instances:new Float32Array(STREAM_CAPACITY*STREAM_STRIDE),id:0,count:0,wraps:0};}
export function addStream(state,birth){
  if(!Number.isSafeInteger(birth.id)||birth.id<1||!Number.isInteger(birth.tile)||birth.tile<0||birth.tile>63||!Number.isInteger(birth.mirror)||birth.mirror<0||birth.mirror>3||![...birth.position,...birth.velocity,...birth.color,...birth.alpha,birth.size,birth.growth,birth.birth_time,birth.life,birth.floor].every(Number.isFinite)||birth.size<0)throw new Error('Invalid original fountain birth packet.');
  if(birth.id<=state.id)return -1;
  const slot=(birth.id-1)%STREAM_CAPACITY,column=birth.tile%8,row=Math.floor(birth.tile/8),u=column/8,v=row/16+.51;
  const i=slot*STREAM_STRIDE,a=state.instances;
  a[i]=birth.position[0];a[i+1]=birth.position[1];a[i+2]=birth.position[2];a[i+3]=birth.size;
  a[i+4]=birth.velocity[0];a[i+5]=birth.velocity[1];a[i+6]=birth.velocity[2];a[i+7]=birth.birth_time;
  a[i+8]=birth.life;a[i+9]=birth.growth;a[i+10]=birth.alpha[0];a[i+11]=birth.alpha[1];
  a[i+12]=u+(birth.mirror&1?.125:0);a[i+13]=v+(birth.mirror&2?.0625:0);a[i+14]=u+(birth.mirror&1?0:.125);a[i+15]=v+(birth.mirror&2?0:.0625);
  a[i+16]=birth.color[0];a[i+17]=birth.color[1];a[i+18]=birth.color[2];a[i+19]=birth.floor;
  state.wraps=Math.floor((birth.id-1)/STREAM_CAPACITY);state.id=birth.id;state.count=Math.min(STREAM_CAPACITY,Math.max(state.count,slot+1));return slot;
}
export class AmbientStreamsRenderer{
  constructor(gl,source){
    this.gl=gl;this.source=source;this.state=source.streamState??=streamState();this.dirty=new Set();this.draws=0;this.triangles=0;this.uploadCalls=0;
    this.gpuBytes=this.state.instances.byteLength+48+source.gpuTexture.rgba.length;
    const shader=(kind,text)=>{const s=gl.createShader(kind);gl.shaderSource(s,text);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){const error=gl.getShaderInfoLog(s);gl.deleteShader(s);throw new Error(error);}return s;};
    const vertex=shader(gl.VERTEX_SHADER,vertexSource),fragment=shader(gl.FRAGMENT_SHADER,fragmentSource);this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);gl.deleteShader(vertex);gl.deleteShader(fragment);if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(this.program));
    this.uniforms=Object.fromEntries(['viewProjection','right','up','eye','now','farDistance','ambientStreamAtlas','fogColor','fogPacking','fogEnabled'].map(n=>[n,gl.getUniformLocation(this.program,n)]));
    this.vao=gl.createVertexArray();gl.bindVertexArray(this.vao);this.corners=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.corners);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,1,1,-1,-1,1,1,-1,1]),gl.STATIC_DRAW);gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,2,gl.FLOAT,false,8,0);
    this.buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);gl.bufferData(gl.ARRAY_BUFFER,this.state.instances,gl.DYNAMIC_DRAW);
    for(let index=1;index<=5;index++){gl.enableVertexAttribArray(index);gl.vertexAttribPointer(index,4,gl.FLOAT,false,80,(index-1)*16);gl.vertexAttribDivisor(index,1);}
    this.texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,this.texture);const t=source.gpuTexture;gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,t.width,t.height,0,gl.RGBA,gl.UNSIGNED_BYTE,t.rgba);for(const s of[gl.TEXTURE_WRAP_S,gl.TEXTURE_WRAP_T])gl.texParameteri(gl.TEXTURE_2D,s,gl.CLAMP_TO_EDGE);for(const s of[gl.TEXTURE_MIN_FILTER,gl.TEXTURE_MAG_FILTER])gl.texParameteri(gl.TEXTURE_2D,s,gl.LINEAR);gl.bindVertexArray(null);
    this.accept(source.frame);
  }
  accept(frame){if(!frame)return;for(const birth of frame.gpu_births??[]){const slot=addStream(this.state,birth);if(slot>=0)this.dirty.add(slot);}this.frame=frame;}
  draw({camera,viewProjection,farDistance=768,fogColor=[0,0,0],fog}){
    const gl=this.gl;this.draws=0;this.triangles=0;this.uploadCalls=0;if(!this.state.count)return;
    const old={program:gl.getParameter(gl.CURRENT_PROGRAM),vao:gl.getParameter(gl.VERTEX_ARRAY_BINDING),buffer:gl.getParameter(gl.ARRAY_BUFFER_BINDING),blend:gl.isEnabled(gl.BLEND),depthMask:gl.getParameter(gl.DEPTH_WRITEMASK),src:gl.getParameter(gl.BLEND_SRC_RGB),dst:gl.getParameter(gl.BLEND_DST_RGB),srcAlpha:gl.getParameter(gl.BLEND_SRC_ALPHA),dstAlpha:gl.getParameter(gl.BLEND_DST_ALPHA),active:gl.getParameter(gl.ACTIVE_TEXTURE)};gl.activeTexture(gl.TEXTURE0);const oldTexture=gl.getParameter(gl.TEXTURE_BINDING_2D);
    gl.useProgram(this.program);gl.bindVertexArray(this.vao);gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);
    const slots=[...this.dirty].sort((a,b)=>a-b);for(let i=0;i<slots.length;){const start=slots[i];let end=start;while(++i<slots.length&&slots[i]===end+1)end=slots[i];gl.bufferSubData(gl.ARRAY_BUFFER,start*80,this.state.instances.subarray(start*20,(end+1)*20));this.uploadCalls++;}this.dirty.clear();
    gl.uniformMatrix4fv(this.uniforms.viewProjection,false,viewProjection);gl.uniform3fv(this.uniforms.eye,[camera.x,camera.y,camera.z]);gl.uniform3fv(this.uniforms.right,cameraAxes(camera.yaw,0).right);gl.uniform3fv(this.uniforms.up,[-Math.sin(camera.yaw)*Math.sin(camera.pitch),Math.cos(camera.pitch),-Math.cos(camera.yaw)*Math.sin(camera.pitch)]);gl.uniform1f(this.uniforms.now,this.frame?.time??0);gl.uniform1f(this.uniforms.farDistance,farDistance);gl.uniform3fv(this.uniforms.fogColor,fogColor);gl.uniform2f(this.uniforms.fogPacking,fog?.encoded_bias??524288,fog?.coefficient??0);gl.uniform1i(this.uniforms.fogEnabled,Boolean(fog));gl.uniform1i(this.uniforms.ambientStreamAtlas,0);gl.bindTexture(gl.TEXTURE_2D,this.texture);gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);gl.depthMask(false);gl.drawArraysInstanced(gl.TRIANGLES,0,6,this.state.count);this.draws=1;this.triangles=this.state.count*2;
    gl.depthMask(old.depthMask);gl.blendFuncSeparate(old.src,old.dst,old.srcAlpha,old.dstAlpha);if(!old.blend)gl.disable(gl.BLEND);gl.bindVertexArray(old.vao);gl.bindBuffer(gl.ARRAY_BUFFER,old.buffer);gl.bindTexture(gl.TEXTURE_2D,oldTexture);gl.activeTexture(old.active);gl.useProgram(old.program);
  }
  dispose(){const gl=this.gl;gl.deleteVertexArray(this.vao);gl.deleteBuffer(this.corners);gl.deleteBuffer(this.buffer);gl.deleteTexture(this.texture);gl.deleteProgram(this.program);}
}
