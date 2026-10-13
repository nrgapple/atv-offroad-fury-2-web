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
  start:async()=>{},launchSingle:Frontend.prototype.launchSingle,quadId:1,riderId:0,saveSelection(){calls.push(this.lastFreestyleEnvironment);},resume(){},show:async()=>{},fail(){},refresh:async()=>{}};
 await Frontend.prototype.action.call(menu,'accept');assert.deepEqual(calls,['env07']);assert.equal(menu.raceActive,false);
 menu.start=async()=>{throw Error('load failure');};menu.tracks=[{id:'env02'}];await Frontend.prototype.action.call(menu,'accept');assert.deepEqual(calls,['env07']);assert.equal(menu.lastFreestyleEnvironment,'env07');
});

test('URL launch applies identity before loading, and invalid links do no gameplay/network work',async()=>{
 const calls=[];
 const menu={tracks:[{id:'env01'},{id:'env02'}],notice:{},raceAvailable:true,
  show:async page=>calls.push(['show',page]),launchSingle:async function(map,racing){calls.push(['start',map,racing,this.quadId,this.riderId,this.playerName]);},
  multiplayer:{action:async(...args)=>calls.push(['network',args[0],args[2]]),disconnect:async()=>calls.push(['disconnect'])},fail:error=>calls.push(['error',error.message])};
 await Frontend.prototype.openLaunch.call(menu,'?map=env02&quad=14&rider=female&player=Jane');
 assert.deepEqual(calls,[['show','tracks'],['start','env02',null,14,1,'Jane']]);
 calls.length=0;await Frontend.prototype.openLaunch.call(menu,'?room=ABCD1234&quad=27');assert.deepEqual(calls,[['show','main']]);assert.match(menu.notice.textContent,/ATVs/);
 calls.length=0;await Frontend.prototype.openLaunch.call(menu,'?action=create&map=env02&play=0');assert.deepEqual(calls,[['show','create-lobby']]);
 calls.length=0;await Frontend.prototype.openLaunch.call(menu,'?action=create&map=env02');assert.deepEqual(calls,[['show','create-lobby'],['network','host',{map:'env02',start:true}]]);
 calls.length=0;await Frontend.prototype.openLaunch.call(menu,'?room=ABCD1234');assert.deepEqual(calls,[['show','join-lobby'],['network','join',undefined]]);
});
test('direct load failures recover to a usable setup page',async()=>{
 const shown=[],menu={tracks:[{id:'env01'}],notice:{},show:async page=>shown.push(page),launchSingle:async()=>{throw Error('asset unavailable');},fail(error){this.notice.textContent=error.message;}};
 await Frontend.prototype.openLaunch.call(menu,'?map=env01');assert.deepEqual(shown,['tracks','tracks']);assert.equal(menu.notice.textContent,'asset unavailable');
});
