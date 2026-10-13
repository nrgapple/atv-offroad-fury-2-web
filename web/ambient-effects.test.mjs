import test from 'node:test';import assert from 'node:assert/strict';import {visibleSprites} from './ambient-effects.js';
test('source world particles keep simulation state while portable visibility bounds draws',()=>{
 const camera={x:0,y:0,z:0,yaw:0,pitch:0};const frame={particles:[{position:[0,0,900],size:1},{position:[0,0,100],size:1},{position:[0,0,-20],size:1},{position:[1,0,100],size:1},{position:[0,0,80],size:1}]};
 const before=structuredClone(frame),scratch=[];assert.deepEqual(visibleSprites(frame,camera,768,scratch).map(p=>p.index),[1,3,4]);assert.deepEqual(frame,before);assert.equal(visibleSprites(frame,{...camera,yaw:Math.PI},768,scratch),scratch);assert.deepEqual(scratch.map(p=>p.index),[2]);
});
