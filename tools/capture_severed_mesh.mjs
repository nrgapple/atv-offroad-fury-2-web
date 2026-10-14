// Inspect the actual GPU index/vertex streams, not just the authored bone poses.
export async function captureSeveredMesh(page) {
 await page.addInitScript(()=>{
  const p=WebGL2RenderingContext.prototype, buffers=new WeakMap(),vaos=new WeakMap(),names=new WeakMap();
  let vao=null;const bindings=new Map();let matrices=null,model=null,projection=null;
  const wrap=(name,inspect)=>{const original=p[name];p[name]=function(...args){inspect.call(this,...args);return original.apply(this,args);};};
  wrap('bindVertexArray',v=>{vao=v;if(v&&!vaos.has(v))vaos.set(v,{});});
  wrap('bindBuffer',function(target,b){bindings.set(target,b);if(target===this.ELEMENT_ARRAY_BUFFER&&vao)vaos.get(vao).indices=b;});
  wrap('bufferData',function(target,data){if(ArrayBuffer.isView(data))buffers.set(bindings.get(target),data.slice());});
  wrap('vertexAttribPointer',function(index,size,type,normalized,stride,offset){if(vao&&index===0)Object.assign(vaos.get(vao),{vertices:bindings.get(this.ARRAY_BUFFER),stride:stride/4,offset:offset/4});});
  const get=p.getUniformLocation;p.getUniformLocation=function(program,name){const loc=get.call(this,program,name);if(loc)names.set(loc,name);return loc;};
  wrap('uniformMatrix4fv',(loc,transpose,v)=>{const name=names.get(loc);if(name?.startsWith('quadBones'))matrices=v;if(name==='model')model=v;if(name==='viewProjection')projection=v;});
  window.severedMesh={draws:0,maxEdge:0,triangles:0,actorDraws:0,maxNdc:0,minW:Infinity};
  wrap('drawElements',function(mode,count){
   if(!vao||!matrices||window.severedMesh.draws>=300)return;
   const fx=window.atvTerrainStatus?.frame?.funny_crash;
   if(!fx?.severed_nodes.length||!model||!projection)return;
   const matches=pose=>pose.length*16===matrices.length && pose.every((m,i)=>m.every((x,j)=>Math.fround(x)===matrices[i*16+j]));
   const rider=matches(fx.rider_node_matrices);if(!rider&&!matches(fx.quad_node_matrices))return;
   const state=vaos.get(vao),vertices=buffers.get(state.vertices),indices=buffers.get(state.indices);
   if(!vertices||!indices||state.stride!==9)return;
   const transform=(m,p)=>[0,1,2,3].map(a=>m[a]*p[0]+m[4+a]*p[1]+m[8+a]*p[2]+m[12+a]);
   const point=i=>{const k=indices[i]*9,node=vertices[k+8]*16;return transform(model,transform(matrices.subarray(node,node+16),vertices.subarray(k,k+3)));};
   for(let i=0;i<count;i+=3){const pts=[point(i),point(i+1),point(i+2)];for(let a=0;a<3;a++){
    if(rider)window.severedMesh.maxEdge=Math.max(window.severedMesh.maxEdge,Math.hypot(...pts[a].slice(0,3).map((v,j)=>v-pts[(a+1)%3][j])));
    const clip=transform(projection,pts[a]);window.severedMesh.minW=Math.min(window.severedMesh.minW,clip[3]);window.severedMesh.maxNdc=Math.max(window.severedMesh.maxNdc,Math.abs(clip[0]/clip[3]),Math.abs(clip[1]/clip[3]));
   }}
   window.severedMesh.actorDraws++;if(rider){window.severedMesh.triangles+=count/3;window.severedMesh.draws++;}

  });
 });
}
