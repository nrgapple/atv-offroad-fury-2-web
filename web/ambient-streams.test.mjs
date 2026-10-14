import test from 'node:test';
import assert from 'node:assert/strict';
import {streamState,addStream,STREAM_CAPACITY,STREAM_STRIDE} from './ambient-streams.js';
const birth=id=>({id,position:[id,2,3],velocity:[4,5,6],size:.4,growth:2,birth_time:7,life:0,tile:3,mirror:0,color:[170,170,170],alpha:[0,120],floor:119});
test('expired stream birth clears its ring slot and repeated frames preserve it',()=>{
  const state=streamState();addStream(state,{...birth(1),life:1});
  for(let id=2;id<=STREAM_CAPACITY+1;id++)addStream(state,birth(id));assert.equal(state.count,STREAM_CAPACITY);
  assert.equal(state.instances[0],STREAM_CAPACITY+1);assert.equal(state.instances[8],0);
  const buffer=state.instances.slice();assert.equal(addStream(state,birth(1)),-1);assert.deepEqual(state.instances,buffer);
  assert.equal(state.instances.length,STREAM_CAPACITY*STREAM_STRIDE);assert.equal(state.wraps,1);
});
test('malformed birth fails before modifying retained graphics restoration state',()=>{
  const state=streamState(),before=state.instances.slice();assert.throws(()=>addStream(state,{...birth(1),velocity:[NaN,0,0]}));assert.deepEqual(state.instances,before);assert.equal(state.id,0);
});
