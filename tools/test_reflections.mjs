// Original-content reflection gate. Native arithmetic observations stay private.
import assert from 'node:assert/strict';
import {assertNoErrors,base,launch,nativeData,newPage,waitForGame} from './browser/harness.mjs';
const native=await nativeData('ATV_REFLECTION_NATIVE','native reflection UV arithmetic (GPU/native comparison)');
const browser=await launch();
try {for(const environment of ['env01','free05']) {
  const errors=[], page=await newPage(browser,{viewport:{width:960,height:600}},errors);
  await page.addInitScript(()=>{
    window.reflectionMeshes=[]; window.reflectionModels=[]; window.reflectionDraws=0; window.reflectionPixels=0;
    const NativeWorker=Worker;
    window.Worker=class extends NativeWorker {
      constructor(...args) {super(...args);this.ops=new Map();this.addEventListener('message',({data})=>{
        const op=this.ops.get(data.id);this.ops.delete(data.id);if(!data.bytes||data.error)return;
        if(op===11) {const mesh=JSON.parse(new TextDecoder().decode(data.bytes));if(mesh.materials.some(m=>/^(env01|free05)ice$/i.test(m)))window.reflectionMeshes.push(mesh);}
        if(op===60)window.reflectionModels=JSON.parse(new TextDecoder().decode(data.bytes)).models.filter(m=>/ice/i.test(m.name));
      });}
      postMessage(data,...args){this.ops.set(data.id,data.op);super.postMessage(data,...args);}
    };
    const source=WebGL2RenderingContext.prototype.shaderSource;
    WebGL2RenderingContext.prototype.shaderSource=function(shader,text){if(text.includes('uniform int sceneryReflectionPlane'))window.reflectionVertexSource=text;return source.call(this,shader,text);};
    const names=new WeakMap(),loc=WebGL2RenderingContext.prototype.getUniformLocation;
    WebGL2RenderingContext.prototype.getUniformLocation=function(...args){const l=loc.apply(this,args);if(l)names.set(l,args[1]);return l;};
    const matrix=WebGL2RenderingContext.prototype.uniformMatrix4fv;
    WebGL2RenderingContext.prototype.uniformMatrix4fv=function(location,transpose,value){
      if(window.reflectionView&&names.get(location)==='viewProjection')value=new Float32Array(window.reflectionView);
      return matrix.call(this,location,transpose,value);
    };
    const eye=WebGL2RenderingContext.prototype.uniform3f;
    WebGL2RenderingContext.prototype.uniform3f=function(location,...value){if(window.reflectionEye&&names.get(location)==='eye')value=window.reflectionEye;return eye.call(this,location,...value);};
    const draw=WebGL2RenderingContext.prototype.drawElements;
    WebGL2RenderingContext.prototype.drawElements=function(...args){
      const p=this.getParameter(this.CURRENT_PROGRAM),plane=this.getUniformLocation(p,'sceneryReflectionPlane');
      if(!plane||!this.getUniform(p,plane))return draw.apply(this,args);
      window.reflectionDraws++;
      const stride=this.getVertexAttrib(7,this.VERTEX_ATTRIB_ARRAY_STRIDE);
      if(stride!==76||!this.getVertexAttrib(8,this.VERTEX_ATTRIB_ARRAY_ENABLED))throw Error('Original local reflection attributes missing');
      const previous=this.getParameter(this.ARRAY_BUFFER_BINDING);this.bindBuffer(this.ARRAY_BUFFER,this.getVertexAttrib(7,this.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING));
      const vertex=new Float32Array(19);this.getBufferSubData(this.ARRAY_BUFFER,0,vertex);this.bindBuffer(this.ARRAY_BUFFER,previous);
      const source=window.reflectionMeshes.some(mesh=>mesh.lods[0].groups.some(g=>g.vertices.some(v=>[...v.local_position,...v.local_normal,v.node].every((x,i)=>Math.fround(x)===vertex[12+i]))));
      if(!source)throw Error('Reflection position/normal/node differs from original decoded PSG');
      if(!window.captureReflection)return draw.apply(this,args);
      const count=this.canvas.width*this.canvas.height,before=new Uint8Array(count*4),after=new Uint8Array(count*4);
      this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,before);draw.apply(this,args);this.readPixels(0,0,this.canvas.width,this.canvas.height,this.RGBA,this.UNSIGNED_BYTE,after);
      let changed=0;for(let i=0;i<count;i++)if(after.subarray(i*4,i*4+3).some((v,j)=>v!==before[i*4+j]))changed++;
      window.reflectionPixels=Math.max(window.reflectionPixels,changed);
    };
  });
  // The inspection camera must reach both CPU visibility and GPU projection.
  // Intercepting only the GL uniform would leave culling on a different camera.
  await page.route('**/game.js*', async route => {
    const response = await route.fetch();
    const source = (await response.text()).replace('const viewProjection = matrix();', 'const viewProjection = window.reflectionView ?? matrix();');
    await route.fulfill({ response, body: source });
  });
  await page.goto(`${base}/play.html?drive=1&environment=${environment}`);
  await waitForGame(page);
  await page.locator('#game').focus();
  const state=await page.evaluate(()=>{
    const status=window.atvTerrainStatus;
    if(!reflectionMeshes.length||!reflectionModels.length)throw Error('Original ice assets were not decoded');
    const mesh=reflectionMeshes[0],placement=reflectionModels.find(m=>m.name.toLowerCase()===mesh.nodes[0].name.toLowerCase())||reflectionModels[0];
    const vertices=mesh.lods[0].groups.flatMap(g=>g.vertices),low=[0,1,2].map(i=>Math.min(...vertices.map(v=>v.position[i]))),high=[0,1,2].map(i=>Math.max(...vertices.map(v=>v.position[i])));
    const center=low.map((v,i)=>(v+high[i])*.5),m=placement.world_matrix;
    const point=[0,1,2].map(i=>m[12+i]+m[i]*center[0]+m[4+i]*center[1]+m[8+i]*center[2]);
    const span=Math.max(high[0]-low[0],high[2]-low[2],50),eye=[point[0],point[1]+Math.min(span,status.fog?.end*.2||span),point[2]];
    const half=span*.65,aspect=960/600;
    // Asset-derived top-down inspection view; this camera is test-only.
    window.reflectionView=[1/half,0,0,0,0,0,-1/(span*4),0,0,-aspect/half,0,0,-point[0]/half,point[2]*aspect/half,eye[1]/(span*4),1];
    window.reflectionEye=eye;window.captureReflection=true;
    return {groups:status.sceneryReflectionGroups,instances:status.sceneInstances,skipped:status.skippedSceneGroups,models:reflectionModels.length};
  });
  assert(state.groups>0);assert.equal(state.skipped,0);if(environment==='free05')assert.equal(state.instances,78);
  await page.waitForFunction(()=>window.reflectionPixels>20,null,{timeout:15000});
  await page.evaluate(()=>{window.captureReflection=false;});
  const first=await page.locator('#game').screenshot();
  const previousDraws=await page.evaluate(()=>{window.reflectionEye[0]+=80;return reflectionDraws;});
  await page.waitForFunction(previous=>reflectionDraws>previous+4,previousDraws);
  const second=await page.locator('#game').screenshot();assert.notDeepEqual(second,first,'Reflection must respond to camera direction');
  let maximumError=null;
  if(native&&environment==='env01') maximumError=await page.evaluate(cases=>{
    const gl=document.querySelector('#game').getContext('webgl2'),priorProgram=gl.getParameter(gl.CURRENT_PROGRAM),priorVao=gl.getParameter(gl.VERTEX_ARRAY_BINDING);
    const compile=(type,text)=>{const s=gl.createShader(type);gl.shaderSource(s,text);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;};
    const vertex=compile(gl.VERTEX_SHADER,reflectionVertexSource),fragment=compile(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1.0);}');
    const p=gl.createProgram();gl.attachShader(p,vertex);gl.attachShader(p,fragment);gl.transformFeedbackVaryings(p,['texcoord'],gl.INTERLEAVED_ATTRIBS);gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));
    const identity=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],vao=gl.createVertexArray(),buffer=gl.createBuffer(),feedback=gl.createTransformFeedback();
    gl.bindVertexArray(vao);gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK,feedback);gl.bindBuffer(gl.TRANSFORM_FEEDBACK_BUFFER,buffer);gl.bufferData(gl.TRANSFORM_FEEDBACK_BUFFER,8,gl.DYNAMIC_READ);gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER,0,buffer);gl.useProgram(p);
    gl.uniformMatrix4fv(gl.getUniformLocation(p,'viewProjection'),false,identity);gl.uniformMatrix4fv(gl.getUniformLocation(p,'model'),false,identity);gl.uniformMatrix4fv(gl.getUniformLocation(p,'quadBones[0]'),false,identity);gl.vertexAttrib3f(0,0,0,0);gl.vertexAttrib1f(3,0);gl.vertexAttrib3f(7,0,0,0);
    const result=new Float32Array(2);let maximum=0;
    gl.enable(gl.RASTERIZER_DISCARD);
    const rotated=[0,0,-1,0,0,1,0,0,1,0,0,0,80,-30,120,1],c=Math.SQRT1_2,child=[c,0,-c,0,0,1,0,0,c,0,c,0,3,4,5,1];
    const transform=(m,v)=>[0,1,2].map(i=>m[12+i]+m[i]*v[0]+m[4+i]*v[1]+m[8+i]*v[2]);
    for(const [root,node] of [[identity,identity],[rotated,identity],[rotated,child]]) {
      gl.uniformMatrix4fv(gl.getUniformLocation(p,'model'),false,root);gl.uniformMatrix4fv(gl.getUniformLocation(p,'quadBones[0]'),false,node);
      const localPosition=[3,-5,7];gl.vertexAttrib3fv(7,localPosition);
      for(const item of cases){
        const localEye=localPosition.map((v,i)=>v+item.direction[i]*100),eye=transform(root,transform(node,localEye));
        gl.uniform1i(gl.getUniformLocation(p,'sceneryReflectionPlane'),item.plane);gl.uniform3fv(gl.getUniformLocation(p,'eye'),eye);gl.vertexAttrib3fv(8,item.normal);
        gl.beginTransformFeedback(gl.POINTS);gl.drawArrays(gl.POINTS,0,1);gl.endTransformFeedback();gl.getBufferSubData(gl.TRANSFORM_FEEDBACK_BUFFER,0,result);
        for(let i=0;i<2;i++){const error=Math.abs(result[i]-item.uv[i]);if(!Number.isFinite(error)||error>0.00001)throw Error(`GPU/native reflection mismatch: ${JSON.stringify({item,result:[...result],error})}`);maximum=Math.max(maximum,error);}
      }
    }
    gl.disable(gl.RASTERIZER_DISCARD);gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK,null);gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER,0,null);gl.bindVertexArray(priorVao);gl.useProgram(priorProgram);gl.deleteTransformFeedback(feedback);gl.deleteBuffer(buffer);gl.deleteVertexArray(vao);gl.deleteProgram(p);gl.deleteShader(vertex);gl.deleteShader(fragment);return maximum;
  },native.cases);
  assertNoErrors(errors);assert.equal(await page.evaluate(()=>document.querySelector('#game').getContext('webgl2').getError()),0);
  if(process.env.ATV_REFLECTION_SCREENSHOTS)await page.screenshot({path:`${process.env.ATV_REFLECTION_SCREENSHOTS}-${environment}.png`});
  console.log(JSON.stringify({environment,...state,pixels:await page.evaluate(()=>reflectionPixels),nativeCases:native&&environment==='env01'?native.cases.length:0,gpuComparisons:native&&environment==='env01'?native.cases.length*3:0,maximumError}));
  await page.close();
}console.log('PASS: original ice overlays, local PSG attributes, camera-dependent reflection UVs and optional native arithmetic comparison. Full GS/VU hardware equivalence remains open.');}finally{await browser.close();}
