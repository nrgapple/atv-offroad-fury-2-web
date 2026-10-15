// Authored single-frame speed presentation. No camera velocity, history buffer,
// simulation changes or third-party shader code. Units in the port are feet/sec.
//
// One full-screen pass: a jittered radial streak toward the travel vanishing
// point, procedural wind lines and a peripheral vignette. The player's own
// ATV/rider is excluded exactly with stencil (ref 1) and its screen bounds are
// rejected as blur samples so it never smears onto the ground. A small FOV kick
// widens the periphery, the strongest cheap speed cue in racing cameras.
export function speedRushAmount(speedFeetPerSecond, intensity = 'normal') {
  if (!Number.isFinite(speedFeetPerSecond) || intensity === 'off') return 0;
  const t=Math.max(0,Math.min(1,(Math.abs(speedFeetPerSecond)*3600/5280-40)/35));
  return t*t*(3-2*t)*({low:.45,normal:.75,high:1}[intensity]??0);
}
// Multiplies the camera focal length: <1 widens the view. Bounded to ~6 degrees.
export const speedRushFocalScale = amount => 1-.09*Math.max(0,Math.min(1,amount));
// Phones and tablets get fewer taps; jitter hides the coarser sampling.
export function speedRushTaps(coarse, slowFrames=false) { return Math.max(3,(coarse?5:8)-(slowFrames?2:0)); }
// Screen-space travel focus from a clip-space projection, clamped on screen.
export function speedRushFocus(viewProjection, eye, velocity) {
  const length=Math.hypot(...velocity);
  if(!(length>1))return null;
  const point=[eye[0]+velocity[0]/length*600,eye[1]+velocity[1]/length*600,eye[2]+velocity[2]/length*600,1];
  const clip=[0,1,3].map(row=>point.reduce((sum,v,i)=>sum+v*viewProjection[i*4+row],0));
  if(!(clip[2]>1e-3))return null;
  const clamp=v=>Math.max(.2,Math.min(.8,v));
  return [clamp(clip[0]/clip[2]*.5+.5),clamp(clip[1]/clip[2]*.5+.5)];
}
// Screen-space bounds of a world box, as [centerX, centerY, halfX, halfY] in uv.
export function speedRushBounds(viewProjection, origin, extent=[5,7,5]) {
  let min=[Infinity,Infinity],max=[-Infinity,-Infinity];
  for(let i=0;i<8;i++){
    const point=[origin[0]+(i&1?1:-1)*extent[0],origin[1]+(i&2?extent[1]:-1),origin[2]+(i&4?1:-1)*extent[2],1];
    const clip=[0,1,3].map(row=>point.reduce((sum,v,j)=>sum+v*viewProjection[j*4+row],0));
    if(!(clip[2]>1e-3))return [0,0,0,0];
    for(const k of [0,1]){const v=clip[k]/clip[2]*.5+.5;min[k]=Math.min(min[k],v);max[k]=Math.max(max[k],v);}
  }
  return [(min[0]+max[0])/2,(min[1]+max[1])/2,Math.min(1,(max[0]-min[0])/2),Math.min(1,(max[1]-min[1])/2)];
}
const MAX_TAPS=8;
export class SpeedRush {
  constructor(gl,{coarse=false}={}) {
    this.gl=gl;this.coarse=coarse;this.speed=0;this.amount=0;this.draws=0;this.width=0;this.height=0;this.failed=false;
    this.time=0;this.focus=[.5,.5];this.frameTime=1/60;this.taps=speedRushTaps(coarse);this.stencil=false;
  }
  reset() { this.amount=0;this.draws=0;this.focus=[.5,.5]; }
  get focalScale() { return speedRushFocalScale(this.amount); }
  // Advance before building the camera so the FOV kick and pass share one amount.
  update({speed,intensity,active,dt}) {
    this.speed=speed;
    const desired=active?speedRushAmount(speed,intensity):0;
    // Off, pauses, crashes and low speed are truly free: no image copy/pass.
    if(!active||intensity==='off'||desired===0){this.reset();return 0;}
    const step=Math.max(0,dt);
    this.amount+=(desired-this.amount)*(1-Math.exp(-step/.25));
    this.time=(this.time+step*(1.5+2.5*this.amount))%1000;
    this.dt=step;
    this.frameTime+=(step-this.frameTime)*(1-Math.exp(-step/.5));
    // Drop taps while the device is under 45 fps; restore once it recovers.
    this.taps=speedRushTaps(this.coarse,this.frameTime>1/45);
    return this.amount;
  }
  prepare() {
    const gl=this.gl,shaders=[];
    try {
      const compile=(type,source)=>{const s=gl.createShader(type);shaders.push(s);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;};
      const vertex=compile(gl.VERTEX_SHADER,`#version 300 es
        out vec2 uv;
        void main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));uv=p;gl_Position=vec4(p*2.0-1.0,0.0,1.0);}`);
      const fragment=compile(gl.FRAGMENT_SHADER,`#version 300 es
        precision highp float;
        uniform sampler2D scene; uniform float amount; uniform vec2 focus; uniform vec4 vehicle;
        uniform vec2 pixel; uniform float aspect; uniform float time; uniform int taps;
        in vec2 uv; out vec4 color;
        float hash(float n){return fract(sin(n*127.1)*43758.5453);}
        // Samples that land on the player's own vehicle carry no weight.
        float open(vec2 p){return vehicle.z<=0.0?1.0:smoothstep(0.85,1.1,length((p-vehicle.xy)/vehicle.zw));}
        void main(){
          vec2 ray=uv-focus;
          // Unit radius reaches the nearer screen edge in landscape and portrait.
          vec2 shaped=ray*vec2(aspect,1.0)/min(aspect,1.0);
          float r=length(shaped);
          vec3 base=texture(scene,uv).rgb;
          float strength=amount*smoothstep(0.1,0.7,r);
          if(strength<0.015){color=vec4(base,1.0);return;}
          // Interleaved gradient noise turns tap banding into fine grain.
          float jitter=fract(52.9829189*fract(dot(gl_FragCoord.xy,vec2(0.06711056,0.00583715))));
          vec2 reach=ray*(0.16*strength);
          vec3 total=base; float weight=1.0;
          for(int i=1;i<=${MAX_TAPS};i++){
            if(i>taps)break;
            vec2 p=uv-reach*((float(i)-jitter)/float(taps));
            float w=open(p);
            total+=texture(scene,clamp(p,pixel*0.5,vec2(1.0)-pixel*0.5)).rgb*w; weight+=w;
          }
          vec3 c=total/weight;
          // Wind lines: polar cells around the travel focus, moving outward in
          // log-radius so they lengthen and accelerate like real perspective.
          if(r>0.24){
            float cells=84.0;
            float a=(atan(shaped.y,shaped.x)/6.2831853+0.5)*cells;
            float id=floor(a);
            float h1=hash(id),h2=hash(id+31.7),h3=hash(id+77.1);
            float across=abs(fract(a)-0.5-(h1-0.5)*0.6)*6.2831853/cells*r/max(pixel.x,pixel.y);
            float width=mix(0.7,2.4,smoothstep(0.3,1.0,r))*(0.6+0.8*h2);
            float line=1.0-smoothstep(width*0.5,width*0.5+1.0,across);
            float s=log(r)*1.7-time*(1.1+h2*1.4)+h3*9.0;
            float on=step(hash(id*1.37+floor(s)*13.1),amount*0.3);
            float f=fract(s);
            // Faint trailing tail, bright leading head.
            float body=smoothstep(0.05,0.42,f)*(1.0-smoothstep(0.42,0.47,f));
            c=mix(c,vec3(1.0),line*body*on*smoothstep(0.24,0.65,r)*(0.2+0.3*h3));
          }
          // Gentle peripheral vignette focuses the eye on the travel line.
          c*=1.0-0.2*amount*smoothstep(0.55,1.15,r);
          color=vec4(c,1.0);
        }`);
      this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);
      if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(this.program));
      this.uniforms=Object.fromEntries(['scene','amount','focus','vehicle','pixel','aspect','time','taps'].map(n=>[n,gl.getUniformLocation(this.program,n)]));
      this.texture=gl.createTexture();this.vao=gl.createVertexArray();
      gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,this.texture);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
      this.stencil=Boolean(gl.getContextAttributes?.()?.stencil);
    }catch(error){this.dispose();this.failed=true;this.error=String(error);}
    finally{for(const shader of shaders)gl.deleteShader(shader);}
  }
  // Callers that skip update() (tests, tools) still get the original contract.
  draw({speed,intensity,active,dt,width,height,focus,vehicle=[0,0,0,0]}) {
    this.draws=0;
    if(speed!==undefined)this.update({speed,intensity,active,dt});
    if(!active||intensity==='off'||this.amount<.001||this.failed)return;
    if(!this.program)this.prepare();if(this.failed)return;
    if(focus){const k=1-Math.exp(-(this.dt??1/60)/.12);this.focus=this.focus.map((v,i)=>v+(focus[i]-v)*k);}
    const gl=this.gl;
    gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,this.texture);
    if(width!==this.width||height!==this.height){
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGB8,width,height,0,gl.RGB,gl.UNSIGNED_BYTE,null);this.width=width;this.height=height;
    }
    gl.copyTexSubImage2D(gl.TEXTURE_2D,0,0,0,0,0,width,height);
    gl.disable(gl.DEPTH_TEST);gl.disable(gl.BLEND);gl.depthMask(false);
    // Stencil ref 1 marks the player's own ATV and rider: leave them crisp.
    if(this.stencil){gl.enable(gl.STENCIL_TEST);gl.stencilFunc(gl.NOTEQUAL,1,0xff);gl.stencilOp(gl.KEEP,gl.KEEP,gl.KEEP);}
    gl.useProgram(this.program);gl.bindVertexArray(this.vao);
    gl.uniform1i(this.uniforms.scene,0);gl.uniform1f(this.uniforms.amount,this.amount);
    gl.uniform2f(this.uniforms.focus,...this.focus);gl.uniform4f(this.uniforms.vehicle,...vehicle);
    gl.uniform2f(this.uniforms.pixel,1/width,1/height);gl.uniform1f(this.uniforms.aspect,width/height);
    gl.uniform1f(this.uniforms.time,this.time);gl.uniform1i(this.uniforms.taps,this.taps);
    gl.drawArrays(gl.TRIANGLES,0,3);this.draws=1;
    if(this.stencil)gl.disable(gl.STENCIL_TEST);
    gl.bindVertexArray(null);gl.depthMask(true);gl.enable(gl.DEPTH_TEST);
  }
  snapshot(){return {speed:this.speed,amount:this.amount,draws:this.draws,taps:this.taps,focus:this.focus,focalScale:this.focalScale,stencil:this.stencil,textureBytes:this.width*this.height*3,failed:this.failed,error:this.error??null};}
  dispose(){const g=this.gl;if(this.texture)g.deleteTexture(this.texture);if(this.vao)g.deleteVertexArray(this.vao);if(this.program)g.deleteProgram(this.program);this.texture=this.vao=this.program=null;this.width=this.height=0;this.reset();}
}
