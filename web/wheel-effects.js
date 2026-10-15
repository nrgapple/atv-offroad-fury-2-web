import { cameraAxes } from './render-support.js';
// Original map particle atlases and Rust tire-effect state. JavaScript only draws.
const vertexSource = `#version 300 es
precision highp float;
layout(location=0) in vec2 corner;
layout(location=1) in vec4 pointSize;
layout(location=2) in vec4 rectangle;
layout(location=3) in float particleAlpha;
uniform mat4 viewProjection;
uniform vec3 right, up;
uniform bool wet;
uniform float now;
layout(location=4) in vec4 velocityBirth;
layout(location=5) in float lifetime;
out vec2 texcoord;
out float alpha;
void main() {
  vec3 center=pointSize.xyz;
  float age=now-velocityBirth.w;
  if(wet){center+=velocityBirth.xyz*age+vec3(0.0,-16.1,0.0)*age*age;center.y=max(center.y,particleAlpha);}
  vec3 point=center + (right*corner.x+up*corner.y)*pointSize.w;
  gl_Position=viewProjection*vec4(point,1.0);
  texcoord=mix(rectangle.xy,rectangle.zw,corner*0.5+0.5);
  alpha=wet?(64.0+63.0*clamp(1.0-age/lifetime,0.0,1.0))/128.0:particleAlpha;
  if(wet&&age>lifetime)gl_Position=vec4(2.0,2.0,2.0,1.0);
}`;
const fragmentSource = `#version 300 es
precision highp float;
in vec2 texcoord;
in float alpha;
uniform sampler2D atlas;
uniform vec3 lightColor;
uniform bool wet;
out vec4 color;
void main() {
  vec4 sampled=texture(atlas,texcoord);
  if(!wet&&sampled.a*255.0<3.0)discard;
  //001451a0 packet alpha64, GS128 unity; recovered PST remains in source units.
  color=vec4(sampled.rgb*lightColor,clamp(sampled.a*(255.0/128.0)*alpha,0.0,1.0));
}`;
const normalize=name=>name.replaceAll('\\','/').split('/').at(-1).toLowerCase();
export async function loadWheelEffects({resource,entries,quadArchive,quadEntries,codec,json,scene,mode}) {
  const named=(list,name)=>{const index=list.findIndex(entry=>normalize(entry.name)===name.toLowerCase());if(index<0)throw new Error(`Original wheel effect resource is missing: ${name}`);return index;};
  const prtIndex=entries.findIndex(entry=>/particles\.prt$/i.test(entry.name));
  if(prtIndex<0)return null;
  const decoder=new TextDecoder();
  const [prt,dust,chunk,spray]=await Promise.all([
    codec(3,resource,prtIndex),codec(3,quadArchive,named(quadEntries,'quaddust.emt')),
    codec(3,quadArchive,named(quadEntries,'quaddirtchunk.emt')),codec(3,quadArchive,named(quadEntries,'quaddirtspray.emt')),
  ]);
  const source={scene,mode,prt:decoder.decode(prt),dust:decoder.decode(dust),chunk:decoder.decode(chunk),spray:decoder.decode(spray)};
  const config=await json(74,new TextEncoder().encode(JSON.stringify(source)));
  const stem=config.atlas.texture.replace(/\.tga$/i,'');
  const bytes=await codec(3,resource,named(entries,`${stem}.pst`));
  const texture={...await json(7,bytes),rgba:await codec(8,bytes)};
  for(const frame of config.atlas.frames)if(frame.rect[2]>texture.width||frame.rect[3]>texture.height)throw new Error('Original particle rectangle exceeds atlas.');
  const gpuMaterial=entries.findIndex(entry=>/(?:^|[/\\])(?:particle_gpu|env\d+particles_gpu)\.psm$/i.test(entry.name));
  if(gpuMaterial<0)throw new Error('Original wet particle material is missing.');
  const gpuDefinition=decoder.decode(await codec(3,resource,gpuMaterial));
  const gpuBinding=gpuDefinition.match(/^\s*TextureMap_1\s*=\s*([A-Za-z0-9_]+)\s*$/mi)?.[1];
  if(!gpuBinding)throw new Error('Original wet particle texture binding is missing.');
  const gpuBytes=await codec(3,resource,named(entries,`${gpuBinding}.pst`));
  const gpuTexture={...await json(7,gpuBytes),rgba:await codec(8,gpuBytes)};
  if(gpuTexture.width!==128||![64,256].includes(gpuTexture.height))throw new Error('Unsupported original wet particle atlas.');
  return{source,config,texture,gpuTexture,wetBirthHistory:[]};
}
export async function loadMotorcycleEffects({resource,entries,config,codec,json}) {
  const name = config.atlas.texture.replace(/\.tga$/i,'.pst');
  const index = entries.findIndex(entry => normalize(entry.name) === name.toLowerCase());
  if (index < 0) throw new Error('Motorcycle particle atlas is missing.');
  const bytes = await codec(3,resource,index);
  const texture = {...await json(7,bytes),rgba:await codec(8,bytes)};
  for (const frame of config.atlas.frames) if(frame.rect[2]>texture.width||frame.rect[3]>texture.height) throw new Error('Motorcycle particle rectangle exceeds atlas.');
  return {config,texture,gpuTexture:texture,wetBirthHistory:[]};
}
export class WheelEffectsRenderer {
  constructor(gl,source) {
    this.gl=gl;this.source=source;this.draws=0;this.triangles=0;this.gpuBytes=(600+10800)*14*4+12*4+source.texture.rgba.length+source.gpuTexture.rgba.length;
    this.frame=null;this.pending=true;this.instances=new Float32Array(600*14);this.wetInstances=new Float32Array(10800*14);this.wetCount=0;this.wetId=0;this.wetPending=[];
    const shader=(kind,text)=>{const s=gl.createShader(kind);gl.shaderSource(s,text);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){const error=gl.getShaderInfoLog(s);gl.deleteShader(s);throw new Error(error);}return s;};
    const vertex=shader(gl.VERTEX_SHADER,vertexSource),fragment=shader(gl.FRAGMENT_SHADER,fragmentSource);
    this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);gl.deleteShader(vertex);gl.deleteShader(fragment);
    if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(this.program));
    this.uniforms=Object.fromEntries(['viewProjection','right','up','atlas','now','wet','lightColor'].map(name=>[name,gl.getUniformLocation(this.program,name)]));
    this.vao=gl.createVertexArray();gl.bindVertexArray(this.vao);
    this.corners=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.corners);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,1,1,-1,-1,1,1,-1,1]),gl.STATIC_DRAW);gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,2,gl.FLOAT,false,8,0);
    this.buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);gl.bufferData(gl.ARRAY_BUFFER,this.instances.byteLength,gl.DYNAMIC_DRAW);
    for(const[index,size,offset]of[[1,4,0],[2,4,16],[3,1,32],[4,4,36],[5,1,52]]){gl.enableVertexAttribArray(index);gl.vertexAttribPointer(index,size,gl.FLOAT,false,56,offset);gl.vertexAttribDivisor(index,1);}
    this.wetBuffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.wetBuffer);gl.bufferData(gl.ARRAY_BUFFER,this.wetInstances.byteLength,gl.DYNAMIC_DRAW);
    this.texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,this.texture);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,source.texture.width,source.texture.height,0,gl.RGBA,gl.UNSIGNED_BYTE,source.texture.rgba);
    for(const setting of[gl.TEXTURE_WRAP_S,gl.TEXTURE_WRAP_T])gl.texParameteri(gl.TEXTURE_2D,setting,gl.CLAMP_TO_EDGE);
    for(const setting of[gl.TEXTURE_MIN_FILTER,gl.TEXTURE_MAG_FILTER])gl.texParameteri(gl.TEXTURE_2D,setting,gl.LINEAR);
    this.wetTexture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,this.wetTexture);
    gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,source.gpuTexture.width,source.gpuTexture.height,0,gl.RGBA,gl.UNSIGNED_BYTE,source.gpuTexture.rgba);
    for(const setting of[gl.TEXTURE_WRAP_S,gl.TEXTURE_WRAP_T])gl.texParameteri(gl.TEXTURE_2D,setting,gl.CLAMP_TO_EDGE);
    for(const setting of[gl.TEXTURE_MIN_FILTER,gl.TEXTURE_MAG_FILTER])gl.texParameteri(gl.TEXTURE_2D,setting,gl.LINEAR);
    for(const birth of source.wetBirthHistory)this.addWet(birth);
    gl.bindVertexArray(null);
  }
  addWet(birth){
    if(!Number.isSafeInteger(birth.id)||!Number.isInteger(birth.tile)||birth.tile<0||birth.tile>15||![...birth.position,...birth.velocity,birth.size,birth.birth_time,birth.life,birth.floor].every(Number.isFinite)||birth.life<=0)throw new Error('Invalid Rust wet particle birth.');
    if(birth.id<=this.wetId)return false;this.wetId=birth.id;
    const slot=(birth.id-1)%10800,column=Math.floor(birth.tile/4),row=birth.tile%4;
    this.wetInstances.set([...birth.position,birth.size,column/4,row/8,(column+1)/4,(row+1)/8,birth.floor,...birth.velocity,birth.birth_time,birth.life],slot*14);
    this.wetPending.push(slot);this.wetCount=Math.min(10800,Math.max(this.wetCount,slot+1));return true;
  }
  accept(frame) {
    if(!frame)return;
    if(!Number.isFinite(frame.time)||!Array.isArray(frame.particles)||frame.particles.length>600)throw new Error('Invalid Rust wheel particle frame.');
    if(this.frame?.time===frame.time&&this.frame?.placement_epoch===frame.placement_epoch)return;
    this.frame=frame;this.pending=true;
    for(const birth of frame.wet_births??[])if(this.addWet(birth))this.source.wetBirthHistory.push(birth);
    this.source.wetBirthHistory=this.source.wetBirthHistory.filter(birth=>frame.time-birth.birth_time<=birth.life);
  }
  draw({camera,viewProjection}) {
    const started=performance.now(),gl=this.gl,particles=this.frame?.particles??[];this.draws=0;this.triangles=0;
    if(!particles.length&&!this.wetCount)return;
    const program=gl.getParameter(gl.CURRENT_PROGRAM);
    if(this.pending){
      particles.forEach((p,index)=>{
        const frame=this.source.config.atlas.frames[p.frame];if(!frame||![...p.position,p.size,p.alpha].every(Number.isFinite))throw new Error('Invalid Rust wheel particle.');
        const rect=frame.rect,w=this.source.texture.width,h=this.source.texture.height;
        this.instances.set([...p.position,p.size,(rect[0]+0.5)/w,(rect[1]+0.5)/h,(rect[2]+0.5)/w,(rect[3]+0.5)/h,p.alpha,0,0,0,0,0],index*14);
      });
      gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);gl.bufferSubData(gl.ARRAY_BUFFER,0,this.instances.subarray(0,particles.length*14));this.pending=false;
    }
    gl.useProgram(this.program);gl.uniformMatrix4fv(this.uniforms.viewProjection,false,viewProjection);
    const yaw=camera.yaw,pitch=camera.pitch;
    gl.uniform3fv(this.uniforms.right,cameraAxes(yaw,0).right);
    gl.uniform3fv(this.uniforms.up,[-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch),-Math.cos(yaw)*Math.sin(pitch)]);
    gl.activeTexture(gl.TEXTURE0);gl.uniform1i(this.uniforms.atlas,0);gl.uniform1f(this.uniforms.now,this.frame?.time??0);
    gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);gl.depthMask(false);
    gl.bindVertexArray(this.vao);
    const bindInstances=buffer=>{gl.bindBuffer(gl.ARRAY_BUFFER,buffer);for(const[index,size,offset]of[[1,4,0],[2,4,16],[3,1,32],[4,4,36],[5,1,52]])gl.vertexAttribPointer(index,size,gl.FLOAT,false,56,offset);};
    if(particles.length){bindInstances(this.buffer);gl.uniform1i(this.uniforms.wet,false);gl.uniform3fv(this.uniforms.lightColor,[1,1,1]);gl.bindTexture(gl.TEXTURE_2D,this.texture);gl.drawArraysInstanced(gl.TRIANGLES,0,6,particles.length);this.draws++;}
    if(this.wetCount&&this.source.wetBirthHistory.length){
      bindInstances(this.wetBuffer);for(const slot of this.wetPending)gl.bufferSubData(gl.ARRAY_BUFFER,slot*56,this.wetInstances.subarray(slot*14,slot*14+14));this.wetPending=[];
      gl.uniform1i(this.uniforms.wet,true);gl.uniform3fv(this.uniforms.lightColor,this.source.config.light_color.map(value=>value/128));gl.bindTexture(gl.TEXTURE_2D,this.wetTexture);gl.drawArraysInstanced(gl.TRIANGLES,0,6,this.wetCount);this.draws++;
    }
    gl.depthMask(true);gl.disable(gl.BLEND);gl.bindVertexArray(null);gl.useProgram(program);
    this.triangles=(particles.length+(this.source.wetBirthHistory.length?this.wetCount:0))*2;this.wetLive=this.source.wetBirthHistory.length;this.cpuMilliseconds=performance.now()-started;
  }
  dispose(){const gl=this.gl;gl.deleteVertexArray(this.vao);gl.deleteBuffer(this.corners);gl.deleteBuffer(this.buffer);gl.deleteBuffer(this.wetBuffer);gl.deleteTexture(this.texture);gl.deleteTexture(this.wetTexture);gl.deleteProgram(this.program);}
}

// Gorn blood: ground splats, stump caps and droplets packed for one
// instanced draw. 12 floats per instance: position+kind, axis+seed, extra.
export const BLOOD_STRIDE = 12, BLOOD_CAPACITY = 120 + 64 + 32;
const seedAt = (x, z) => { const s = Math.sin(x * 12.9898 + z * 78.233) * 43758.5453; return s - Math.floor(s); };
export function packBlood(blood, out) {
  let n = 0;
  const put = (x, y, z, kind, ax, ay, az, seed, e0, e1, e2, e3) => {
    if (n >= BLOOD_CAPACITY) return;
    out.set([x, y, z, kind, ax, ay, az, seed, e0, e1, e2, e3], n++ * BLOOD_STRIDE);
  };
  const splats = blood?.splats ?? [], stumps = blood?.stumps ?? [], drops = blood?.drops ?? [];
  // Ground stains first, then stump caps, then droplets in flight on top.
  for (let i = 0; i + 9 < splats.length; i += 10) {
    const s = splats.slice(i, i + 10);
    put(s[0], s[1], s[2], 1, s[3], s[4], s[5], seedAt(s[0], s[2]), s[6], s[7], s[8], s[9]);
  }
  for (let i = 0; i + 6 < stumps.length; i += 7) {
    const s = stumps.slice(i, i + 7);
    put(s[0], s[1], s[2], 2, s[3], s[4], s[5], seedAt(s[0], s[1]), 0, 0, s[6], s[6]);
  }
  for (let i = 0; i + 6 < drops.length; i += 7) {
    const d = drops.slice(i, i + 7), speed = Math.hypot(d[3], d[4], d[5]);
    // Motion streak: ~30 ms of travel, never longer than a foot.
    put(d[0], d[1], d[2], 0, d[3], d[4], d[5], 0, 0, 0, d[6], Math.min(d[6] + speed * 0.03, 1));
  }
  return n;
}
export function bloodCount(blood) {
  return Math.floor((blood?.drops?.length ?? 0) / 7) + Math.floor((blood?.splats?.length ?? 0) / 10);
}
export class BloodRenderer {
  constructor(gl) {
    this.gl=gl; this.draws=0; this.instances=0; this.data=new Float32Array(BLOOD_CAPACITY*BLOOD_STRIDE);
    const shader=(type,source)=>{const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw new Error(gl.getShaderInfoLog(s));return s;};
    const vertex=shader(gl.VERTEX_SHADER,`#version 300 es
    precision highp float;
    layout(location=0) in vec2 corner;
    layout(location=1) in vec4 a0;
    layout(location=2) in vec4 a1;
    layout(location=3) in vec4 a2;
    uniform mat4 viewProjection;
    uniform vec3 eye;
    out vec2 uv; flat out float kind; flat out float seed; out float shade;
    void main(){
      uv=corner; kind=a0.w; seed=a1.w;
      vec3 p=a0.xyz, X, Y;
      if(kind<.5){
        // Droplet: a camera-facing streak along its velocity, tail behind.
        float speed=length(a1.xyz);
        vec3 along=speed>.01?a1.xyz/speed:vec3(0,1,0);
        vec3 view=normalize(eye-p);
        vec3 side=cross(along,view);
        side=length(side)>1e-3?normalize(side):normalize(cross(vec3(0,1,0),view)+vec3(1e-4,0,0));
        vec3 up=normalize(cross(view,side));
        p-=along*(a2.w-a2.z)*.5;
        X=side*a2.z; Y=up*a2.w*sign(dot(up,along)+1e-4);
        shade=1.;
      } else {
        vec3 N=normalize(a1.xyz), T;
        if(kind<1.5){vec3 d=vec3(a2.x,0.,a2.y);d.y=-(N.x*d.x+N.z*d.z)/max(N.y,.05);T=normalize(d);}
        else T=normalize(abs(N.y)<.9?cross(N,vec3(0,1,0)):cross(N,vec3(1,0,0)));
        vec3 B=cross(N,T);
        X=T*(kind<1.5?a2.w:a2.z); Y=B*a2.z;
        p+=N*(kind<1.5?.03:.015);
        shade=.55+.45*max(dot(N,normalize(vec3(.35,.85,.4))),0.);
      }
      gl_Position=viewProjection*vec4(p+X*corner.x+Y*corner.y,1);
    }`);
    const fragment=shader(gl.FRAGMENT_SHADER,`#version 300 es
    precision highp float;
    in vec2 uv; flat in float kind; flat in float seed; in float shade; out vec4 color;
    float hash(float n){return fract(sin(n)*43758.5453);}
    void main(){
      float r=length(uv);
      if(kind<.5){
        if(r>1.)discard;
        vec3 c=mix(vec3(.30,.004,.01),vec3(.62,.02,.03),1.-r);
        c+=vec3(.3,.15,.15)*(1.-smoothstep(0.,.35,length(uv-vec2(-.3,.4))));
        color=vec4(c,(1.-smoothstep(.55,1.,r))*.95);
      } else if(kind<1.5){
        // Irregular lobed stain with satellite droplets beyond the rim.
        float a=atan(uv.y,uv.x);
        float edge=.68+.10*sin(a*5.+seed*6.283)+.07*sin(a*9.+seed*17.)+.05*sin(a*17.+seed*41.);
        float body=1.-smoothstep(edge-.06,edge,r);
        vec2 cell=floor(uv*5.), f=fract(uv*5.)-.5;
        float h=hash(dot(cell,vec2(12.9898,78.233))+seed*91.);
        float satellite=h>.8&&r>edge&&r<1.?1.-smoothstep(.12,.28,length(f)):0.;
        float alpha=max(body,satellite);
        if(alpha<.02)discard;
        vec3 wet=mix(vec3(.15,0.,.008),vec3(.40,.012,.02),smoothstep(edge,0.,r));
        wet+=vec3(.16,.07,.07)*(1.-smoothstep(0.,.18,abs(r-.4*edge)));
        color=vec4(wet*shade,alpha*.92);
      } else {
        // Stump cap: torn muscle around a bone and marrow.
        if(r>1.)discard;
        vec3 meat=mix(vec3(.56,.07,.07),vec3(.30,.01,.02),smoothstep(.35,1.,r));
        vec2 b=uv-vec2(.08,-.05);
        vec3 c=mix(meat,vec3(.86,.82,.72),1.-smoothstep(.26,.32,length(b)));
        c=mix(c,vec3(.45,.12,.08),1.-smoothstep(.09,.13,length(b)));
        color=vec4(c*shade,1.-smoothstep(.88,1.,r));
      }
    }`);
    this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);gl.deleteShader(vertex);gl.deleteShader(fragment);
    if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(this.program));
    this.uniforms=Object.fromEntries(['viewProjection','eye'].map(n=>[n,gl.getUniformLocation(this.program,n)]));
    this.vao=gl.createVertexArray();gl.bindVertexArray(this.vao);
    this.corners=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.corners);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,1,1,-1,-1,1,1,-1,1]),gl.STATIC_DRAW);gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,2,gl.FLOAT,false,8,0);
    this.buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);gl.bufferData(gl.ARRAY_BUFFER,this.data.byteLength,gl.DYNAMIC_DRAW);
    for(const index of [1,2,3]) {gl.enableVertexAttribArray(index);gl.vertexAttribPointer(index,4,gl.FLOAT,false,BLOOD_STRIDE*4,(index-1)*16);gl.vertexAttribDivisor(index,1);}
    gl.bindVertexArray(null);
  }
  draw(blood,{camera,viewProjection,program}) {
    const gl=this.gl;this.draws=0;
    const changed=this.blood!==blood;this.blood=blood;
    if(changed)this.instances=packBlood(blood,this.data);
    if(!this.instances)return;
    gl.useProgram(this.program);gl.uniformMatrix4fv(this.uniforms.viewProjection,false,viewProjection);
    gl.uniform3fv(this.uniforms.eye,[camera.x,camera.y,camera.z]);
    gl.bindVertexArray(this.vao);gl.bindBuffer(gl.ARRAY_BUFFER,this.buffer);if(changed)gl.bufferSubData(gl.ARRAY_BUFFER,0,this.data.subarray(0,this.instances*BLOOD_STRIDE));
    gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);gl.depthMask(false);
    gl.enable(gl.POLYGON_OFFSET_FILL);gl.polygonOffset(-2,-2);
    gl.drawArraysInstanced(gl.TRIANGLES,0,6,this.instances);
    gl.disable(gl.POLYGON_OFFSET_FILL);gl.depthMask(true);gl.disable(gl.BLEND);gl.bindVertexArray(null);gl.useProgram(program);this.draws=1;
  }
  dispose(){const gl=this.gl;gl.deleteVertexArray(this.vao);gl.deleteBuffer(this.corners);gl.deleteBuffer(this.buffer);gl.deleteProgram(this.program);}
}

