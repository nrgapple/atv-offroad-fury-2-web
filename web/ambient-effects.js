// Original placed TOY effects are simulated in Rust; this bounded renderer
// adapts their sprite packets into two depth-tested, atlas-instanced GL draws.
import {fogVertexShader, fogFragmentShader} from './fog.js';
const normalize = name => name.replaceAll('\\','/').split('/').at(-1).toLowerCase();
export async function loadAmbientEffects({resource,entries,codec,json,scene,common,mode,wheelEffects}) {
  const source={scene,common,mode};
  const plan=await json(130,new TextEncoder().encode(JSON.stringify(source)));
  if (!plan.declared) return null;
  const read=async name=>{
    const index=entries.findIndex(entry=>normalize(entry.name)===name.toLowerCase());
    if(index<0)throw new Error(`Original ambient resource is missing: ${name}`);
    return codec(3,resource,index);
  };
  source.emts={};
  for(const name of new Set(plan.placements.filter(p=>[2,8].includes(p.type_id)).map(p=>p.filename))) source.emts[name]=new TextDecoder().decode(await read(name));
  const prt=entries.findIndex(entry=>/particles\.prt$/i.test(entry.name));
  source.prt=prt<0?'':new TextDecoder().decode(await codec(3,resource,prt));
  const config=await json(131,new TextEncoder().encode(JSON.stringify(source)));
  let texture=null;
  if (config.atlas) {
    if(wheelEffects?.config.atlas.texture.toLowerCase()===config.atlas.texture.toLowerCase()) texture=wheelEffects.texture;
    else {
      const bytes=await read(config.atlas.texture.replace(/\.tga$/i,'.pst'));
      texture={...await json(7,bytes),rgba:await codec(8,bytes)};
    }
    for(const frame of config.atlas.frames)if(frame.rect[2]>texture.width||frame.rect[3]>texture.height)throw new Error('Original ambient rectangle exceeds atlas.');
  }
  return {source,config,texture,frame:{time:0,particles:[],total_emitted:0,dropped:0}};
}
const vertexSource=`#version 300 es
precision highp float;
layout(location=0) in vec2 corner;
layout(location=1) in vec4 pointSize;
layout(location=2) in vec4 rectangle;
layout(location=3) in vec2 alphaLit;
uniform mat4 viewProjection;
uniform vec3 right,up,eye;
out vec2 texcoord;
out vec2 sprite;
${fogVertexShader}
void main(){
  vec3 point=pointSize.xyz+(right*corner.x+up*corner.y)*pointSize.w;
  gl_Position=viewProjection*vec4(point,1.0);
  texcoord=mix(rectangle.xy,rectangle.zw,corner*.5+.5);sprite=alphaLit;
  prepareFog(length(point-eye),gl_Position);
}`;
const fragmentSource=`#version 300 es
precision highp float;
in vec2 texcoord;
in vec2 sprite;
uniform sampler2D ambientAtlas;
uniform vec3 lightColor,fogColor;
out vec4 color;
${fogFragmentShader}
void main(){
  vec4 sampled=texture(ambientAtlas,texcoord);
  if(sampled.a*255.0<3.0)discard;
  color=vec4(mix(fogColor,sampled.rgb*mix(vec3(1.0),lightColor,sprite.y),fogCoverage()),clamp(sampled.a*(255.0/128.0)*sprite.x,0.0,1.0));
}`;
export function visibleSprites(frame,camera,farDistance=768,workspace=[]) {
  workspace.length=0;if(!frame)return workspace;
  const far=Math.max(0,farDistance),fx=Math.sin(camera.yaw)*Math.cos(camera.pitch),fy=Math.sin(camera.pitch),fz=Math.cos(camera.yaw)*Math.cos(camera.pitch);
  for(let index=0;index<frame.particles.length;index++){
    const p=frame.particles[index],dx=p.position[0]-camera.x,dy=p.position[1]-camera.y,dz=p.position[2]-camera.z,depth=dx*fx+dy*fy+dz*fz;
    if(dx*dx+dy*dy+dz*dz<=(far+p.size)**2&&depth>=-p.size)workspace.push({p,index,depth});
  }
  return workspace.sort((a,b)=>b.depth-a.depth||a.index-b.index);
}
export class AmbientEffectsRenderer {
  constructor(gl,source){
    this.gl=gl;this.source=source;this.frame=source.frame;this.draws=0;this.triangles=0;this.visible=0;
    this.visibleScratch=[];this.instances=new Float32Array(source.config.capacity*10);this.gpuBytes=this.instances.byteLength+48+source.texture.rgba.length;
    const shader=(kind,text)=>{const s=gl.createShader(kind);gl.shaderSource(s,text);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){const error=gl.getShaderInfoLog(s);gl.deleteShader(s);throw new Error(error);}return s;};
    const vertex=shader(gl.VERTEX_SHADER,vertexSource),fragment=shader(gl.FRAGMENT_SHADER,fragmentSource);
    this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);gl.deleteShader(vertex);gl.deleteShader(fragment);
    if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(this.program));
    this.uniforms=Object.fromEntries(['viewProjection','right','up','eye','ambientAtlas','lightColor','fogColor','fogPacking','fogEnabled'].map(name=>[name,gl.getUniformLocation(this.program,name)]));
    this.vao=gl.createVertexArray();gl.bindVertexArray(this.vao);this.corners=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.corners);
    gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,1,1,-1,-1,1,1,-1,1]),gl.STATIC_DRAW);gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,2,gl.FLOAT,false,8,0);
    this.buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);gl.bufferData(gl.ARRAY_BUFFER,this.instances.byteLength,gl.DYNAMIC_DRAW);
    for(const [index,size,offset]of[[1,4,0],[2,4,16],[3,2,32]]){gl.enableVertexAttribArray(index);gl.vertexAttribPointer(index,size,gl.FLOAT,false,40,offset);gl.vertexAttribDivisor(index,1);}
    this.texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,this.texture);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,source.texture.width,source.texture.height,0,gl.RGBA,gl.UNSIGNED_BYTE,source.texture.rgba);
    for(const setting of[gl.TEXTURE_WRAP_S,gl.TEXTURE_WRAP_T])gl.texParameteri(gl.TEXTURE_2D,setting,gl.CLAMP_TO_EDGE);
    for(const setting of[gl.TEXTURE_MIN_FILTER,gl.TEXTURE_MAG_FILTER])gl.texParameteri(gl.TEXTURE_2D,setting,gl.LINEAR);gl.bindVertexArray(null);
  }
  accept(frame){
    if(!frame||!Number.isFinite(frame.time)||!Array.isArray(frame.particles)||frame.particles.length>this.source.config.capacity)throw new Error('Invalid Rust ambient particle frame.');
    for(const p of frame.particles)if(!this.source.config.atlas.frames[p.frame]||![...p.position,p.size,p.alpha].every(Number.isFinite)||p.size<0)throw new Error('Invalid original ambient sprite.');
    this.frame=frame;this.source.frame=frame;
  }
  draw({camera,viewProjection,farDistance=768,fogColor=[0,0,0],fog,lightColor=[1,1,1]}){
    const started=performance.now(),gl=this.gl;this.draws=0;this.triangles=0;
    const sprites=visibleSprites(this.frame,camera,farDistance,this.visibleScratch);this.visible=sprites.length;
    if(!sprites.length){this.cpuMilliseconds=performance.now()-started;return;}
    const old={program:gl.getParameter(gl.CURRENT_PROGRAM),vao:gl.getParameter(gl.VERTEX_ARRAY_BINDING),buffer:gl.getParameter(gl.ARRAY_BUFFER_BINDING),blend:gl.isEnabled(gl.BLEND),depthMask:gl.getParameter(gl.DEPTH_WRITEMASK),src:gl.getParameter(gl.BLEND_SRC_RGB),dst:gl.getParameter(gl.BLEND_DST_RGB),srcAlpha:gl.getParameter(gl.BLEND_SRC_ALPHA),dstAlpha:gl.getParameter(gl.BLEND_DST_ALPHA),active:gl.getParameter(gl.ACTIVE_TEXTURE)};
    gl.activeTexture(gl.TEXTURE0);const oldTexture=gl.getParameter(gl.TEXTURE_BINDING_2D);
    gl.useProgram(this.program);gl.bindVertexArray(this.vao);gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);
    gl.uniformMatrix4fv(this.uniforms.viewProjection,false,viewProjection);
    gl.uniform3fv(this.uniforms.eye,[camera.x,camera.y,camera.z]);gl.uniform3fv(this.uniforms.right,[-Math.cos(camera.yaw),0,Math.sin(camera.yaw)]);
    gl.uniform3fv(this.uniforms.up,[-Math.sin(camera.yaw)*Math.sin(camera.pitch),Math.cos(camera.pitch),-Math.cos(camera.yaw)*Math.sin(camera.pitch)]);
    gl.uniform3fv(this.uniforms.lightColor,lightColor);gl.uniform3fv(this.uniforms.fogColor,fogColor);gl.uniform2f(this.uniforms.fogPacking,fog?.encoded_bias??524288,fog?.coefficient??0);gl.uniform1i(this.uniforms.fogEnabled,Boolean(fog));
    gl.uniform1i(this.uniforms.ambientAtlas,0);gl.bindTexture(gl.TEXTURE_2D,this.texture);gl.enable(gl.BLEND);gl.depthMask(false);
    // Preserve back-to-front ordering inside each blend family. No per-particle draws.
    for(const additive of [false,true]){
      let count=0;
      for(const {p}of sprites){if(p.additive!==additive)continue;const rect=this.source.config.atlas.frames[p.frame].rect,w=this.source.texture.width,h=this.source.texture.height;
        const u=[(rect[0]+.5)/w,(rect[2]+.5)/w],v=[(rect[1]+.5)/h,(rect[3]+.5)/h];if(p.mirror[0])u.reverse();if(p.mirror[1])v.reverse();
        this.instances.set([...p.position,p.size,u[0],v[0],u[1],v[1],p.alpha,Number(p.lit)],count++*10);
      }
      if(!count)continue;gl.bufferSubData(gl.ARRAY_BUFFER,0,this.instances.subarray(0,count*10));gl.blendFunc(gl.SRC_ALPHA,additive?gl.ONE:gl.ONE_MINUS_SRC_ALPHA);gl.drawArraysInstanced(gl.TRIANGLES,0,6,count);this.draws++;this.triangles+=count*2;
    }
    gl.depthMask(old.depthMask);gl.blendFuncSeparate(old.src,old.dst,old.srcAlpha,old.dstAlpha);if(!old.blend)gl.disable(gl.BLEND);gl.bindVertexArray(old.vao);gl.bindBuffer(gl.ARRAY_BUFFER,old.buffer);gl.bindTexture(gl.TEXTURE_2D,oldTexture);gl.activeTexture(old.active);gl.useProgram(old.program);
    this.cpuMilliseconds=performance.now()-started;
  }
  dispose(){const gl=this.gl;gl.deleteVertexArray(this.vao);gl.deleteBuffer(this.corners);gl.deleteBuffer(this.buffer);gl.deleteTexture(this.texture);gl.deleteProgram(this.program);}
}
