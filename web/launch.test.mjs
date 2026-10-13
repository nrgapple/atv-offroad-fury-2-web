import test from 'node:test';
import assert from 'node:assert/strict';
import {parseLaunch,buildRideUrl,copyRideUrl,ENVIRONMENTS} from './launch.js';
const parse = search=>parseLaunch(search);
test('ordinary and cache/marketing-only visits retain the title',()=>{
 for(const q of ['', '?v=123&utm_source=test','#room=ABCD1234']) assert.equal(parse(q).requested,false);
});
test('direct links are deterministic across every supported map and vehicle',()=>{
 for(const map of ENVIRONMENTS)for(const quadId of [1,26,27,46])for(const riderId of [0,1]){
  const url=buildRideUrl('https://furyremastered.com',{map,quadId,riderId,playerName:'A & B 🏁'}),s=parse(new URL(url).search);
  assert.deepEqual(s.errors,[]);assert.equal(s.mode,'single');assert.equal(s.map,map);assert.equal(s.quadId,quadId);assert.equal(s.riderId,riderId);assert.equal(s.playerName,'A & B 🏁');assert.equal(s.play,true);
 }
});
test('map names, compatible aliases and setup-only launch are supported',()=>{
 const s=parseLaunch('?mode=sp&environment=Tioga%20Valley&vehicle=14&rider=female&name=Sam&play=false',[{id:'env02',name:'Tioga Valley'}]);
 assert.deepEqual(s.errors,[]);assert.equal(s.map,'env02');assert.equal(s.quadId,14);assert.equal(s.riderId,1);assert.equal(s.play,false);
 const old=parse('?drive=1&environment=ENV08');assert.deepEqual(old.errors,[]);assert.equal(old.map,'env08');
});
test('old room links autojoin and create links use explicit intent',()=>{
 const s=parse('?room=abcd1234');assert.deepEqual(s.errors,[]);assert.equal(s.action,'join');assert.equal(s.room,'ABCD1234');assert.equal(s.mode,'multiplayer');
 assert.deepEqual(parse('?mode=mp&action=create&map=free05&quad=26').errors,[]);
 assert.deepEqual(parse('?action=host&room=FURY1234&play=0').errors,[]);
 assert(parse('?mode=mp').errors.length);assert(parse('?mode=mp&action=join').errors.length);
});
test('race settings survive roundtrip; incomplete modes are rejected',()=>{
 const s=parse(new URL(buildRideUrl('https://example.test',{activity:'race',quadId:34,raceSettings:{laps:5,cpu:4}})).search);
 assert.deepEqual(s.errors,[]);assert.deepEqual(s.raceSettings,{laps:5,cpu:4,difficulty:1});
 for(const q of ['?activity=race&map=env02','?mode=mp&room=ABCD1234&activity=race','?room=ABCD1234&quad=27','?laps=2','?activity=race&cpu=5'])assert(parse(q).errors.length,q);
});
test('malformed, conflicting and duplicate values never silently launch',()=>{
 for(const q of ['?map=nope','?map=','?map=env01&map=env02','?map=env01&environment=env01','?quad=1&atv=2','?quad=0','?quad=47','?quad=1.5','?quad=0x10','?rider=2','?player=','?player=%00','?player='+encodeURIComponent('🏁'.repeat(9)),'?mode=single&room=ABCD1234','?room=ABCD','?play=maybe','?activity=supercross','?action=delete','?activity=race&laps=6'])assert(parse(q).errors.length,q);
});
test('invite serialization removes name, owner secrets and unrelated location state',()=>{
 const url=new URL(buildRideUrl('https://example.test/play.html?host=SECRET#token',{mode:'multiplayer',room:'ABCD1234',map:'env08',quadId:14,riderId:1,playerName:'Host'},{includePlayer:false}));
 assert.equal(url.pathname,'/play.html');assert.equal(url.hash,'');assert.equal(url.searchParams.has('host'),false);assert.equal(url.searchParams.has('player'),false);assert.equal(parse(url.search).action,'join');
 assert.throws(()=>buildRideUrl('https://example.test',{mode:'multiplayer',room:'bad'}),/Room/);
});
test('blocked clipboard displays the whole usable URL',async()=>{
 const notice={};await copyRideUrl('https://example.test/play.html?map=env02',notice);assert.equal(notice.textContent,'https://example.test/play.html?map=env02');
});
