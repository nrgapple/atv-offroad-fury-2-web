// Rust owns camera-ring selection and source SnowEmitter births; GLSL ages them.
const vertexSource=`#version 300 es
precision highp float;
layout(location=0)in vec2 corner;
layout(location=1)in vec4 pointSize;
layout(location=2)in vec4 velocityBirth;
layout(location=3)in vec3 lifeMirrorHalf;
uniform mat4 viewProjection;
uniform vec3 right,up;
uniform float now,minAlpha,maxAlpha;
out vec2 uv;out float alpha;
void main(){
 float age=now-velocityBirth.w,life=lifeMirrorHalf.x;
 vec3 center=pointSize.xyz+velocityBirth.xyz*age;
 //SourceSnowEmitter clears GPU gravity; its mode2 minimum plane is-10000.
 center.y=max(center.y,-10000.0);
 gl_Position=viewProjection*vec4(center+(right*corner.x+up*corner.y)*pointSize.w,1.0);
 vec2 mirrored=corner*0.5+0.5;
 int bits=int(lifeMirrorHalf.y);
 if((bits&1)!=0)mirrored.x=1.0-mirrored.x;
 if((bits&2)!=0)mirrored.y=1.0-mirrored.y;
 uv=vec2((mirrored.x+lifeMirrorHalf.z)*0.5,mirrored.y);
 alpha=(minAlpha+(maxAlpha-minAlpha)*clamp(1.0-age/max(life,0.000001),0.0,1.0))/128.0;
 if(life<=0.0||age<0.0||age>life)gl_Position=vec4(2.0,2.0,2.0,1.0);
}`;
const fragmentSource=`#version 300 es
precision highp float;
in vec2 uv;in float alpha;
uniform sampler2D atlas;uniform vec3 particleColor;
out vec4 color;
void main(){vec4 sampled=texture(atlas,uv);color=vec4(sampled.rgb*particleColor,clamp(sampled.a*(255.0/128.0)*alpha,0.0,1.0));}`;
export async function loadSnow({selection,resource,entries,codec,json,gpuTexture}){
 if(!selection?.active||selection.definition?.kind!=='snow'||!selection.definition.ring_emitters)return null;
 const index=entries.findIndex(entry=>entry.name.replaceAll('\\','/').split('/').at(-1).toLowerCase()==='snow.emt');
 if(index<0)throw new Error('Original Snow emitter is missing.');
 const emitter=new TextDecoder().decode(await codec(3,resource,index));
 if(!gpuTexture)throw new Error('Original Snow GPU particle texture is missing.');
 //Configuration is installed after scene initialization, which replaces Rust state.
 return {selection,emitter,texture:gpuTexture,history:[],frame:null,config:null};
}
export class SnowRenderer{
 constructor(gl,source){
  this.gl=gl;this.source=source;this.capacity=10800;this.instances=new Float32Array(this.capacity*11);this.count=0;this.sequence=0;this.pending=[];this.draws=0;this.triangles=0;this.uploadBytes=0;
  const shader=(kind,text)=>{const s=gl.createShader(kind);gl.shaderSource(s,text);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw new Error(gl.getShaderInfoLog(s));return s;};
  const vertex=shader(gl.VERTEX_SHADER,vertexSource),fragment=shader(gl.FRAGMENT_SHADER,fragmentSource);this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);gl.deleteShader(vertex);gl.deleteShader(fragment);if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(this.program));
  this.uniforms=Object.fromEntries(['viewProjection','right','up','now','minAlpha','maxAlpha','particleColor','atlas'].map(name=>[name,gl.getUniformLocation(this.program,name)]));
  this.vao=gl.createVertexArray();gl.bindVertexArray(this.vao);this.corners=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.corners);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,1,1,-1,-1,1,1,-1,1]),gl.STATIC_DRAW);gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,2,gl.FLOAT,false,8,0);
  this.buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);gl.bufferData(gl.ARRAY_BUFFER,this.instances.byteLength,gl.DYNAMIC_DRAW);
  for(const[index,size,offset]of[[1,4,0],[2,4,16],[3,3,32]]){gl.enableVertexAttribArray(index);gl.vertexAttribPointer(index,size,gl.FLOAT,false,44,offset);gl.vertexAttribDivisor(index,1);}
  this.texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,this.texture);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,source.texture.width,source.texture.height,0,gl.RGBA,gl.UNSIGNED_BYTE,source.texture.rgba);for(const setting of[gl.TEXTURE_WRAP_S,gl.TEXTURE_WRAP_T])gl.texParameteri(gl.TEXTURE_2D,setting,gl.CLAMP_TO_EDGE);for(const setting of[gl.TEXTURE_MIN_FILTER,gl.TEXTURE_MAG_FILTER])gl.texParameteri(gl.TEXTURE_2D,setting,gl.LINEAR);
  this.gpuBytes=this.instances.byteLength+48+source.texture.rgba.length;for(const birth of source.history)this.add(birth);gl.bindVertexArray(null);
 }
 add(birth){
  if(!Number.isSafeInteger(birth.id)||birth.id<=0||![...birth.position,...birth.velocity,birth.time,birth.life,birth.size].every(Number.isFinite)||birth.life<=0)throw new Error('Invalid Rust Snow birth.');
  if(birth.id<=this.sequence)return false;this.sequence=birth.id;
  const slot=(birth.id-1)%this.capacity;this.instances.set([...birth.position,birth.size,...birth.velocity,birth.time,birth.life,birth.mirror,birth.half],slot*11);this.pending.push(slot);this.count=Math.max(this.count,slot+1);return true;
 }
 accept(frame){
  if(!frame||frame.time===this.source.frame?.time)return;
  this.source.frame=frame;for(const birth of frame.births)if(this.add(birth))this.source.history.push(birth);
  //Retain every resident slot, including shader-expired births. A CPU lifetime
  //test rounds differently from GLSL and could remove a boundary flake on restore.
  if(this.source.history.length>this.capacity)this.source.history=this.source.history.filter(birth=>birth.id>this.sequence-this.capacity);
 }
 draw({camera,viewProjection}){
  this.draws=0;this.triangles=0;this.uploadBytes=0;const frame=this.source.frame;if(!frame||!this.source.history.length)return;
  const gl=this.gl,program=gl.getParameter(gl.CURRENT_PROGRAM);gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);
  //Coalesce adjacent new slots; ordinary particle aging has no buffer uploads.
  this.pending.sort((a,b)=>a-b);for(let i=0;i<this.pending.length;){let first=this.pending[i],last=first;while(++i<this.pending.length&&this.pending[i]<=last+1)last=this.pending[i];const values=this.instances.subarray(first*11,(last+1)*11);gl.bufferSubData(gl.ARRAY_BUFFER,first*44,values);this.uploadBytes+=values.byteLength;}this.pending=[];
  gl.useProgram(this.program);gl.uniformMatrix4fv(this.uniforms.viewProjection,false,viewProjection);gl.uniform3fv(this.uniforms.right,[-Math.cos(camera.yaw),0,Math.sin(camera.yaw)]);gl.uniform3fv(this.uniforms.up,[-Math.sin(camera.yaw)*Math.sin(camera.pitch),Math.cos(camera.pitch),-Math.cos(camera.yaw)*Math.sin(camera.pitch)]);gl.uniform1f(this.uniforms.now,frame.time);gl.uniform1f(this.uniforms.minAlpha,this.source.config.min_alpha);gl.uniform1f(this.uniforms.maxAlpha,this.source.config.max_alpha);gl.uniform3fv(this.uniforms.particleColor,this.source.config.color.map(value=>value/128));gl.uniform1i(this.uniforms.atlas,0);gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,this.texture);gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);gl.depthMask(false);gl.bindVertexArray(this.vao);gl.drawArraysInstanced(gl.TRIANGLES,0,6,this.count);this.draws=1;this.triangles=this.count*2;gl.depthMask(true);gl.disable(gl.BLEND);gl.bindVertexArray(null);gl.useProgram(program);
 }
 dispose(){const gl=this.gl;gl.deleteVertexArray(this.vao);gl.deleteBuffer(this.corners);gl.deleteBuffer(this.buffer);gl.deleteTexture(this.texture);gl.deleteProgram(this.program);}
}
