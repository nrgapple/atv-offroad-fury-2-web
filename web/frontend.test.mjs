import test from 'node:test';
import assert from 'node:assert/strict';
import {loadPlayerSelection,savePlayerSelection,Frontend} from './frontend.js';
const defaults={quadId:1,riderId:0,freestyleEnvironment:'env01'};
test('saved vehicle, rider and last-played Freestyle course survive a storage roundtrip',()=>{
 for(const quadId of [1,26,27,46]) {
  const choice={quadId,riderId:1,freestyleEnvironment:'free05'};
  let stored;savePlayerSelection(choice,value=>stored=value);
  assert.deepEqual(loadPlayerSelection(()=>stored),choice);
 }
});
test('invalid or unavailable saved selection safely falls back per field',()=>{
 for(const stored of [null,'bad','null','[]','42','{"freestyleEnvironment":["env02"]}','{"quadId":47,"riderId":7,"freestyleEnvironment":"private"}'])assert.deepEqual(loadPlayerSelection(()=>stored),defaults);
 assert.deepEqual(loadPlayerSelection(()=>{throw Error('denied');}),defaults);
 assert.doesNotThrow(()=>savePlayerSelection(defaults,()=>{throw Error('quota');}));
 assert.deepEqual(loadPlayerSelection(()=>'{"quadId":34,"riderId":1,"freestyleEnvironment":"gone"}'),{...defaults,quadId:34,riderId:1});
});
test('only a successfully played course replaces the remembered Freestyle map',async()=>{
 const calls=[];
 const menu={busy:false,root:{hidden:false},notice:{},page:'tracks',tracks:[{id:'env07'}],track:0,lastFreestyleEnvironment:'free05',raceActive:true,
  start:async()=>{},saveSelection(){calls.push(this.lastFreestyleEnvironment);},resume(){},show:async()=>{},fail(){},refresh:async()=>{}};
 await Frontend.prototype.action.call(menu,'accept');assert.deepEqual(calls,['env07']);assert.equal(menu.raceActive,false);
 menu.start=async()=>{throw Error('load failure');};menu.tracks=[{id:'env02'}];await Frontend.prototype.action.call(menu,'accept');assert.deepEqual(calls,['env07']);assert.equal(menu.lastFreestyleEnvironment,'env07');
});
