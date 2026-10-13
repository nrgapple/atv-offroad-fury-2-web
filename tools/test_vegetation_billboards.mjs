// Original atlas rendering and bounded browser-selection gate; private native
// probes are optional inputs and never required or published by CI.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import fs from 'node:fs/promises';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const native=process.env.ATV_BILLBOARD_NATIVE?JSON.parse(await fs.readFile(process.env.ATV_BILLBOARD_NATIVE,'utf8')):null;
const geometry=process.env.ATV_BILLBOARD_GEOMETRY?JSON.parse(await fs.readFile(process.env.ATV_BILLBOARD_GEOMETRY,'utf8')):null;
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4290',browser=await chromium.launch({headless:true});
try {for(const environment of (process.env.ATV_ECO_MAPS||'env01,env02,env03,env04,env05,env06,env07,env08,free05').split(',')) {
 const page=await browser.newPage({viewport:{width:960,height:600}}),errors=[];
 page.on('pageerror',e=>errors.push(e.message));
 await page.route('**/game.js',async route=>{
  const response=await route.fetch();
  const oracle=`\nconst sourceDrawEcosystem=drawEcosystem;drawEcosystem=()=>{window.billboardSelectionView={...camera};return sourceDrawEcosystem();};\nwindow.billboardSelectionCheck=()=>{
   const camera=window.billboardSelectionView,eco=terrain.ecosystem,fx=Math.sin(camera.yaw),fz=Math.cos(camera.yaw),far=terrain.world.metadata.fog?.end??768;
   const start=performance.now(),brute=[];
   for(const p of eco.renderPlacements){const dx=p.position[0]-camera.x,dz=p.position[2]-camera.z,depth=dx*fx+dz*fz;
    if(dx*dx+dz*dz<=(far+p.radius)**2&&depth>-p.radius)brute.push({index:p.index,depth});}
   brute.sort((a,b)=>a.depth-b.depth);const fullScanMs=performance.now()-start;
   const selected=eco.selection.slice().sort((a,b)=>a.depth-b.depth);
   if(selected.length!==Math.min(1500,brute.length))throw Error('Spatial index lost visible candidates');
   for(let i=0;i<selected.length;i++)if(Math.abs(selected[i].depth-brute[i].depth)>1e-6)throw Error('Bounded selection differs from full scan '+JSON.stringify({i,selected:{index:selected[i].index,depth:selected[i].depth},brute:brute[i],camera}));
   return{candidates:brute.length,selected:selected.length,fullScanMs};};`;
  await route.fulfill({response,body:(await response.text())+oracle});
 });
 await page.addInitScript(()=>{
  window.billboardGate={draws:0,pixels:0,transitions:0,selected:[]};const N=Worker;
  window.Worker=class extends N{constructor(...a){super(...a);this.ops=new Map();this.addEventListener('message',({data})=>{const op=this.ops.get(data.id);this.ops.delete(data.id);if(op===58&&data.bytes&&!data.error)billboardGate.eco=JSON.parse(new TextDecoder().decode(data.bytes));});}postMessage(d,...a){this.ops.set(d.id,d.op);super.postMessage(d,...a);}};
  const p=WebGL2RenderingContext.prototype,draw=p.drawElementsInstanced,shader=p.shaderSource;
  p.shaderSource=function(s,text){if(text.includes('ecoSunDirection'))billboardGate.vertex=text;return shader.call(this,s,text);};
  p.drawElementsInstanced=function(...args){
   const program=this.getParameter(this.CURRENT_PROGRAM),u=name=>{const l=this.getUniformLocation(program,name);return l&&this.getUniform(program,l);};
   if(!u('useEcosystem')||!u('ecoBillboard'))return draw.apply(this,args);
   assertState(this,u,args);
   if(!billboardGate.capture)return draw.apply(this,args);
   const before=new Uint8Array(this.canvas.width*this.canvas.height*4),after=new Uint8Array(before.length);
   this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,before);draw.apply(this,args);this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,after);
   let pixels=0;for(let i=0;i<before.length;i+=4)if(before[i]!==after[i]||before[i+1]!==after[i+1]||before[i+2]!==after[i+2])pixels++;billboardGate.pixels=Math.max(billboardGate.pixels,pixels);
  };
  function assertState(gl,u,args){
   if(args[1]!==12||!u('ecoPerInstance')||!u('useVertexColor')||!u('unlit')||!gl.isEnabled(gl.BLEND))throw Error('Original billboard strip/material state missing');
   if(!gl.getParameter(gl.DEPTH_WRITEMASK)){if(Math.abs(u('alphaCutoff')-1/255)>1e-8)throw Error('Transition TEST reference differs');billboardGate.transitions++;}
   const previous=gl.getParameter(gl.ARRAY_BUFFER_BINDING),instances=new Float32Array(args[4]*16),rotation=u('ecoRotation'),eye=u('eye');
   gl.bindBuffer(gl.ARRAY_BUFFER,gl.getVertexAttrib(4,gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING));gl.getBufferSubData(gl.ARRAY_BUFFER,0,instances);gl.bindBuffer(gl.ARRAY_BUFFER,previous);
   let last=Infinity;for(let i=0;i<args[4];i++){const at=i*16,depth=(instances[at]-eye[0])*(-rotation[1])+(instances[at+2]-eye[2])*rotation[0];if(depth>last+.002)throw Error('Billboard transparency order differs');last=depth;}
   billboardGate.draws++;
  }
 });
 await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
 await page.waitForFunction(()=>atvTerrainStatus?.ready&&atvTerrainStatus?.simulation,null,{timeout:120000});
 assert.match(await page.evaluate(()=>atvTerrainStatus.source),new RegExp(`${environment}\\.tr2$`));
 await page.evaluate(()=>billboardGate.capture=true);await page.waitForFunction(()=>billboardGate.pixels>0,null,{timeout:15000});await page.evaluate(()=>billboardGate.capture=false);
 const result=await page.evaluate(()=>({environment:atvTerrainStatus.source,placements:atvTerrainStatus.vegetationPlacements,scanned:atvTerrainStatus.vegetationScanned,near:atvTerrainStatus.vegetationNear,far:atvTerrainStatus.vegetationFar,transition:atvTerrainStatus.vegetationTransition,triangles:atvTerrainStatus.vegetationTriangles,draws:billboardGate.draws,batches:atvTerrainStatus.vegetationBillboardDraws,pixels:billboardGate.pixels,gl:atvTerrainStatus.glError,indexCount:billboardGate.eco.visibility_index.placements.length}));
 assert(result.draws>0&&result.far>0&&result.pixels>0);assert.equal(result.indexCount,result.placements);assert(result.scanned<result.placements);assert.equal(result.gl,0);assert.deepEqual(errors,[]);
 assert(result.batches>0&&result.batches<=2,'All source atlas species share ordered distant/transition batches');
 await page.keyboard.press('Escape');await page.waitForTimeout(250);
 result.selection=await page.evaluate(()=>billboardSelectionCheck());
 if(native&&environment==='env01')result.native=await page.evaluate(({cases,geometry})=>{
  const gl=document.querySelector('#game').getContext('webgl2'),prior=gl.getParameter(gl.CURRENT_PROGRAM),priorVao=gl.getParameter(gl.VERTEX_ARRAY_BINDING);
  const compile=(type,text)=>{const s=gl.createShader(type);gl.shaderSource(s,text);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;};
  const vertex=compile(gl.VERTEX_SHADER,billboardGate.vertex),fragment=compile(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1.0);}');
  const p=gl.createProgram();gl.attachShader(p,vertex);gl.attachShader(p,fragment);gl.transformFeedbackVaryings(p,['authoredColor','worldXZ','worldHeight','texcoord','gl_Position'],gl.INTERLEAVED_ATTRIBS);gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));
  const vao=gl.createVertexArray(),buffer=gl.createBuffer(),feedback=gl.createTransformFeedback(),identity=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],out=new Float32Array(13);
  gl.bindVertexArray(vao);gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK,feedback);gl.bindBuffer(gl.TRANSFORM_FEEDBACK_BUFFER,buffer);gl.bufferData(gl.TRANSFORM_FEEDBACK_BUFFER,52,gl.DYNAMIC_READ);gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER,0,buffer);gl.useProgram(p);
  const u=name=>gl.getUniformLocation(p,name);gl.uniformMatrix4fv(u('model'),false,identity);gl.uniformMatrix4fv(u('viewProjection'),false,identity);
  for(const name of ['useEcosystem','ecoBillboard','ecoPerInstance','ecoSunPresent'])gl.uniform1i(u(name),true);
  gl.vertexAttrib3f(0,0,0,0);gl.vertexAttrib3f(4,0,0,0);gl.vertexAttrib2f(5,1,1);gl.vertexAttrib3f(9,1,0,64);gl.vertexAttrib4f(10,-1,1,0,1);gl.vertexAttrib4f(11,.5,0,1,0);gl.enable(gl.RASTERIZER_DISCARD);let maxError=0,differingBytes=0;
  for(const item of cases){gl.uniform2fv(u('ecoRotation'),[item.look[2],-item.look[0]]);gl.uniform3fv(u('ecoAmbient'),item.ambient);gl.uniform3fv(u('ecoSunlight'),item.sunlight);gl.uniform3fv(u('ecoSunDirection'),item.sunDirection);
   for(let column=0;column<3;column++){gl.vertexAttrib3f(1,column,0,0);gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,1);gl.endTransformFeedback();gl.getBufferSubData(gl.TRANSFORM_FEEDBACK_BUFFER,0,out);
    for(let i=0;i<4;i++){const error=Math.abs(out[i]-item.colors[column][i]);if(!Number.isFinite(error)||error>1)throw Error(`Native billboard color mismatch ${JSON.stringify({item,column,out:[...out]})}`);maxError=Math.max(maxError,error);if(error)differingBytes++;}
   }
  }
  let maxPositionError=0,maxUvError=0;
  for(const item of geometry??[]){const [bx,,bz]=item.basis;gl.uniformMatrix4fv(u('viewProjection'),false,[bx,0,bz,0,0,1,0,0,-bz,0,bx,0,0,0,0,1]);gl.uniform2fv(u('ecoRotation'),[item.basis[0],-item.basis[2]]);gl.vertexAttrib3fv(4,item.position);gl.vertexAttrib2f(5,item.width,item.height);
   const extent=Math.fround(2/Math.fround(item.u[2]-item.u[0])),horizontal=[-Math.fround(extent*Math.fround(item.u[1]-item.u[0])),0,Math.fround(extent*Math.fround(item.u[2]-item.u[1]))];
   for(const [i,[column,row]]of [[1,1],[0,1],[0,0],[1,0],[2,0],[2,1]].entries()){
    gl.vertexAttrib3f(0,0,1-row,0);gl.vertexAttrib3f(1,column,1,0);gl.vertexAttrib4f(10,horizontal[0],horizontal[2],item.u[0],item.u[2]);gl.vertexAttrib4f(11,item.u[1],item.v[0],item.v[1],item.offset);
    gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,1);gl.endTransformFeedback();gl.getBufferSubData(gl.TRANSFORM_FEEDBACK_BUFFER,0,out);
    const position=[out[9],out[10],out[11]];for(let j=0;j<3;j++){const error=Math.abs(position[j]-item.points[i][j]);if(!Number.isFinite(error)||error>.003)throw Error('Original billboard vertex differs '+JSON.stringify({item,i,j,position,error}));maxPositionError=Math.max(maxPositionError,error);}
    for(let j=0;j<2;j++){const error=Math.abs(out[7+j]-item.uvs[i][j]);if(!Number.isFinite(error)||error>1e-6)throw Error('Original billboard atlas UV differs');maxUvError=Math.max(maxUvError,error);}
   }
  }
  gl.disable(gl.RASTERIZER_DISCARD);gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK,null);gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER,0,null);gl.bindVertexArray(priorVao);gl.useProgram(prior);gl.deleteTransformFeedback(feedback);gl.deleteBuffer(buffer);gl.deleteVertexArray(vao);gl.deleteProgram(p);gl.deleteShader(vertex);gl.deleteShader(fragment);return{cases:cases.length,columns:cases.length*3,maxError,differingBytes,geometryCases:geometry?.length??0,maxPositionError,maxUvError};
 },{cases:native.cases,geometry:geometry?.cases});
 if(process.env.ATV_ECO_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_ECO_SCREENSHOTS}-${environment}.png`});
 console.log(JSON.stringify(result));await page.close();
}console.log('PASS: original billboard resources, six-vertex atlas strip, lighting, transition depth state, sparse Rust index and visible pixels. Native visibility, clipping and GS hardware precision remain open.');
}finally{await browser.close();}
