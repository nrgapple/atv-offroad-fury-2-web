import test from 'node:test';
import assert from 'node:assert/strict';
import {loadPlayerSelection,savePlayerSelection,Frontend,unlockedCourses} from './frontend.js';
import {ENVIRONMENTS,courseResource} from './launch.js';
const defaults={quadId:1,riderId:0,freestyleEnvironment:'env01',playerName:'Rider'};
test('saved vehicle, rider and last-played Freestyle course survive a storage roundtrip',()=>{
 for(const quadId of [1,26,27,46]) {
  const choice={quadId,riderId:1,freestyleEnvironment:'free05',playerName:'Sam 🚀'};
  let stored;savePlayerSelection(choice,value=>stored=value);
  assert.deepEqual(loadPlayerSelection(()=>stored),choice);
 }
});
test('invalid or unavailable saved selection safely falls back per field',()=>{
 for(const playerName of ['', '   ', 'x'.repeat(25), 'bad\nname', 42])assert.equal(loadPlayerSelection(()=>JSON.stringify({playerName})).playerName,'Rider');
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
 calls.length=0;await Frontend.prototype.openLaunch.call(menu,'?room=ABCD1234');assert.deepEqual(calls,[['show','join-lobby'],['network','join',{map:'env01',start:true}]]);
});
test('direct load failures recover to a usable setup page',async()=>{
 const shown=[],menu={tracks:[{id:'env01'}],notice:{},show:async page=>shown.push(page),launchSingle:async()=>{throw Error('asset unavailable');},fail(error){this.notice.textContent=error.message;}};
 await Frontend.prototype.openLaunch.call(menu,'?map=env01');assert.deepEqual(shown,['tracks','tracks']);assert.equal(menu.notice.textContent,'asset unavailable');
});

test('all venues are selectable regardless of source access and points, with one entry per venue',()=>{
 const records=ENVIRONMENTS.map(id=>[id.startsWith('env')?'unspnat'+Number(id.slice(-2)):id.startsWith('free')?'spstunt'+Number(id.slice(-2)):id.startsWith('nat')?'spnat'+Number(id.slice(-2)):id.startsWith('sx')?'prosx'+Number(id.slice(-2)):id==='training'?'train1':'hoc1',{scn:id,name:id,image:id+'_',access:'0',points:'1000'}]);
 records.push(['lend1',{scn:'env01',name:'duplicated event'}],['freeride1',{scn:'env02',name:'debug record'}]);
 const files=ENVIRONMENTS.map(id=>({path:courseResource(id).archive}));
 const tracks=unlockedCourses(new Map(records),files);
 assert.deepEqual(tracks.map(t=>t.id),ENVIRONMENTS);assert.equal(tracks.length,40);
 assert.equal(tracks.find(t=>t.id==='training').image,'');
 assert.deepEqual(unlockedCourses(new Map(records),files.slice(0,9)).map(t=>t.id),ENVIRONMENTS.slice(0,9));
 for(const freestyleEnvironment of ENVIRONMENTS){let stored;savePlayerSelection({...defaults,freestyleEnvironment},v=>stored=v);assert.equal(loadPlayerSelection(()=>stored).freestyleEnvironment,freestyleEnvironment);}
});

test('race setup cycles available original courses and starts the selected one',async()=>{
 const calls=[],menu={busy:false,notice:{},raceCourses:['env01','nat03','sx16'],raceSettings:{laps:3,cpu:3,difficulty:1,course:'env01'},
  refresh:async()=>{},show:async()=>{},fail(error){throw error;},launchSingle:async(map,racing)=>calls.push([map,racing.course])};
 await Frontend.prototype.run.call(menu,'racetrack',-1);assert.equal(menu.raceSettings.course,'sx16');
 await Frontend.prototype.run.call(menu,'racetrack',1);await Frontend.prototype.run.call(menu,'racetrack',1);assert.equal(menu.raceSettings.course,'nat03');
 await Frontend.prototype.run.call(menu,'racestart');assert.deepEqual(calls,[['nat03','nat03']]);
});
