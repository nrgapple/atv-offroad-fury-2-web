// Inspect the actual GPU index/vertex streams, not just the authored bone poses.
export async function captureSeveredMesh(page) {
 await page.addInitScript(()=>{
  const p=WebGL2RenderingContext.prototype, buffers=new WeakMap(),vaos=new WeakMap(),names=new WeakMap();
  let vao=null;const bindings=new Map();let matrices=null;
  const wrap=(name,inspect)=>{const original=p[name];p[name]=function(...args){inspect.call(this,...args);return original.apply(this,args);};};
  wrap('bindVertexArray',v=>{vao=v;if(v&&!vaos.has(v))vaos.set(v,{});});
  wrap('bindBuffer',function(target,b){bindings.set(target,b);if(target===this.ELEMENT_ARRAY_BUFFER&&vao)vaos.get(vao).indices=b;});
  wrap('bufferData',function(target,data){if(ArrayBuffer.isView(data))buffers.set(bindings.get(target),data.slice());});
  wrap('vertexAttribPointer',function(index,size,type,normalized,stride,offset){if(vao&&index===0)Object.assign(vaos.get(vao),{vertices:bindings.get(this.ARRAY_BUFFER),stride:stride/4,offset:offset/4});});
  const get=p.getUniformLocation;p.getUniformLocation=function(program,name){const loc=get.call(this,program,name);if(loc)names.set(loc,name);return loc;};
  wrap('uniformMatrix4fv',(loc,transpose,v)=>{if(names.get(loc)?.startsWith('quadBones'))matrices=v;});
  window.severedMesh={draws:0,maxEdge:0,triangles:0};
  wrap('drawElements',function(mode,count){
   if(!vao||!matrices||window.severedMesh.draws>=300)return;
   const fx=window.atvTerrainStatus?.frame?.funny_crash;
   if(!fx?.severed_nodes.length||fx.rider_node_matrices.length*16!==matrices.length||!fx.rider_node_matrices.every((m,i)=>m.every((x,j)=>Math.fround(x)===matrices[i*16+j])))return;
   const state=vaos.get(vao),vertices=buffers.get(state.vertices),indices=buffers.get(state.indices);
   if(!vertices||!indices||state.stride!==9)return;
   const point=i=>{const k=indices[i]*9,node=vertices[k+8]*16;return [0,1,2].map(a=>matrices[node+a]*vertices[k]+matrices[node+4+a]*vertices[k+1]+matrices[node+8+a]*vertices[k+2]+matrices[node+12+a]);};
   for(let i=0;i<count;i+=3){const pts=[point(i),point(i+1),point(i+2)];for(let a=0;a<3;a++)window.severedMesh.maxEdge=Math.max(window.severedMesh.maxEdge,Math.hypot(...pts[a].map((v,j)=>v-pts[(a+1)%3][j])));}
   window.severedMesh.triangles+=count/3;window.severedMesh.draws++;
  });
 });
}
