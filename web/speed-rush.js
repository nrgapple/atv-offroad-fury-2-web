// Authored single-frame speed presentation. No camera velocity, history buffer,
// simulation changes or third-party shader code. Units in the port are feet/sec.
export function speedRushAmount(speedFeetPerSecond, intensity = 'normal') {
  if (!Number.isFinite(speedFeetPerSecond) || intensity === 'off') return 0;
  const t=Math.max(0,Math.min(1,(Math.abs(speedFeetPerSecond)*3600/5280-40)/35));
  return t*t*(3-2*t)*({low:.45,normal:.75,high:1}[intensity]??0);
}
export class SpeedRush {
  constructor(gl) { this.gl=gl;this.speed=0;this.amount=0;this.draws=0;this.width=0;this.height=0;this.failed=false; }
  reset() { this.amount=0;this.draws=0; }
  prepare() {
    const gl=this.gl,shaders=[];
    try {
      const compile=(type,source)=>{const s=gl.createShader(type);shaders.push(s);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;};
      const vertex=compile(gl.VERTEX_SHADER,`#version 300 es
        out vec2 uv;
        void main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));uv=p;gl_Position=vec4(p*2.0-1.0,0.0,1.0);}`);
      const fragment=compile(gl.FRAGMENT_SHADER,`#version 300 es
        precision highp float;
        uniform sampler2D scene; uniform float amount; uniform vec2 focus; uniform vec2 pixel;
        in vec2 uv; out vec4 color;
        void main(){
          vec2 ray=uv-vec2(0.5,0.56);
          float edge=smoothstep(0.18,0.62,length(uv-0.5));
          float rider=smoothstep(1.0,1.55,length((uv-focus)/vec2(0.17,0.26)));
          float strength=amount*edge*rider;
          vec2 warped=uv-ray*(0.012*strength);
          vec3 total=vec3(0.0);
          // Inward samples avoid pulling undefined pixels from beyond the frame.
          for(int i=0;i<7;i++){
            vec2 p=warped-ray*(float(i)/6.0)*0.10*strength;
            total+=texture(scene,clamp(p,pixel*0.5,vec2(1.0)-pixel*0.5)).rgb;
          }
          color=vec4(total/7.0,1.0);
        }`);
      this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);
      if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(this.program));
      this.uniforms=Object.fromEntries(['scene','amount','focus','pixel'].map(n=>[n,gl.getUniformLocation(this.program,n)]));
      this.texture=gl.createTexture();this.vao=gl.createVertexArray();
      gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,this.texture);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
    }catch(error){this.dispose();this.failed=true;this.error=String(error);}
    finally{for(const shader of shaders)gl.deleteShader(shader);}
  }
  draw({speed,intensity,active,dt,width,height,focus=[.5,.35]}) {
    this.draws=0;this.speed=speed;
    const desired=active?speedRushAmount(speed,intensity):0;
    // Off, pauses, crashes and low speed are truly free: no image copy/pass.
    if(!active||intensity==='off'||desired===0){this.reset();return;}
    this.amount+=(desired-this.amount)*(1-Math.exp(-Math.max(0,dt)/.25));
    if(this.amount<.001||this.failed)return;
    if(!this.program)this.prepare();if(this.failed)return;
    const gl=this.gl;
    gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,this.texture);
    if(width!==this.width||height!==this.height){
      gl.texImage2D(gl.TEXTURE_2D,0,gl.RGB8,width,height,0,gl.RGB,gl.UNSIGNED_BYTE,null);this.width=width;this.height=height;
    }
    gl.copyTexSubImage2D(gl.TEXTURE_2D,0,0,0,0,0,width,height);
    gl.disable(gl.DEPTH_TEST);gl.disable(gl.BLEND);gl.depthMask(false);
    gl.useProgram(this.program);gl.bindVertexArray(this.vao);
    gl.uniform1i(this.uniforms.scene,0);gl.uniform1f(this.uniforms.amount,this.amount);
    gl.uniform2f(this.uniforms.focus,...focus);gl.uniform2f(this.uniforms.pixel,1/width,1/height);
    gl.drawArrays(gl.TRIANGLES,0,3);this.draws=1;
    gl.bindVertexArray(null);gl.depthMask(true);gl.enable(gl.DEPTH_TEST);
  }
  snapshot(){return {speed:this.speed,amount:this.amount,draws:this.draws,textureBytes:this.width*this.height*3,failed:this.failed,error:this.error??null};}
  dispose(){const g=this.gl;if(this.texture)g.deleteTexture(this.texture);if(this.vao)g.deleteVertexArray(this.vao);if(this.program)g.deleteProgram(this.program);this.texture=this.vao=this.program=null;this.width=this.height=0;this.reset();}
}
