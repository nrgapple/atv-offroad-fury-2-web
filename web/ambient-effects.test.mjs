import test from 'node:test';import assert from 'node:assert/strict';import {visibleSprites,spriteAlpha} from './ambient-effects.js';
test('source world particles keep simulation state while portable visibility bounds draws',()=>{
 const camera={x:0,y:0,z:0,yaw:0,pitch:0};const frame={particles:[{position:[0,0,900],size:1},{position:[0,0,100],size:1},{position:[0,0,-20],size:1},{position:[1,0,100],size:1},{position:[0,0,80],size:1}]};
 const before=structuredClone(frame),scratch=[];assert.deepEqual(visibleSprites(frame,camera,768,scratch).map(p=>p.index),[1,3,4]);assert.deepEqual(frame,before);assert.equal(visibleSprites(frame,{...camera,yaw:Math.PI},768,scratch),scratch);assert.deepEqual(scratch.map(p=>p.index),[2]);
});

test('persistent fog fades with squared XYZ distance and retains ordinary sprite alpha',()=>{
 const fog={position:[0,0,0],alpha:.25,fade_distance:20};
 assert.equal(spriteAlpha(fog,{x:0,y:0,z:0}),0);assert.equal(spriteAlpha(fog,{x:0,y:10,z:0}),.0625);assert.equal(spriteAlpha(fog,{x:20,y:0,z:0}),.25);assert.equal(spriteAlpha(fog,{x:0,y:0,z:30}),.25);assert.equal(spriteAlpha({...fog,fade_distance:undefined},{x:0,y:0,z:0}),.25);
});
test('private original001451a0 near-alpha instruction cases',{skip:!process.env.ATV_AMBIENT_FOG_UPDATE_CASES},async()=>{
 const {readFile}=await import('node:fs/promises');const {alphas}=JSON.parse(await readFile(process.env.ATV_AMBIENT_FOG_UPDATE_CASES,'utf8'));const words=new Uint32Array(1),floats=new Float32Array(words.buffer),f=Math.fround;
 for(const c of alphas){const alpha=f(f(1-f(c.age*c.inverse))*.5);floats[0]=spriteAlpha({position:c.position,alpha,fade_distance:c.distance},{x:c.eye[0],y:c.eye[1],z:c.eye[2]});const actual=words[0];words[0]=c.alpha;floats[0]/=128;assert.equal(actual,words[0]);}
});
