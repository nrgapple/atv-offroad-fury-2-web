import test from 'node:test';
import assert from 'node:assert/strict';
import {parseLaunch,buildRideUrl,copyRideUrl,ENVIRONMENTS,RACE_COURSES,courseResource,loadingShotsResource} from './launch.js';
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
 assert.deepEqual(s.errors,[]);assert.deepEqual(s.raceSettings,{laps:5,cpu:4,difficulty:1,course:'env01'});
 assert.deepEqual(parse('?activity=race&map=sx07').raceSettings,{laps:3,cpu:3,difficulty:1,course:'sx07'});
 for(const q of ['?activity=race&map=free05','?activity=race&map=training','?mode=mp&room=ABCD1234&activity=race','?room=ABCD1234&quad=27','?laps=2','?activity=race&cpu=5'])assert(parse(q).errors.length,q);
});
test('race difficulty and the original AI cheats are race-only link settings',()=>{
 assert.deepEqual(parse('?activity=race&map=nat01&difficulty=expert').raceSettings,{laps:3,cpu:3,difficulty:2,course:'nat01'});
 assert.equal(parse('?activity=race&difficulty=Normal').raceSettings.difficulty,1);
 assert.deepEqual(parse('?activity=race&cheats=EATDIRT,bringit').raceSettings,{laps:3,cpu:3,difficulty:1,course:'env01',aggressive:true,all_out:true});
 const settings={laps:2,cpu:4,difficulty:2,course:'sx07',aggressive:true};
 const url=new URL(buildRideUrl('https://example.test',{activity:'race',map:'sx07',raceSettings:settings}));
 assert.equal(url.searchParams.get('difficulty'),'expert');assert.equal(url.searchParams.get('cheats'),'eatdirt');
 assert.deepEqual(parse(url.search).raceSettings,settings);
 assert.equal(new URL(buildRideUrl('https://example.test',{activity:'race',raceSettings:{laps:3,cpu:3,difficulty:1}})).searchParams.has('difficulty'),false);
 for(const q of ['?difficulty=expert','?cheats=eatdirt','?activity=race&difficulty=hard','?activity=race&difficulty=0','?activity=race&cheats=nofog','?activity=race&cheats='])assert(parse(q).errors.length,q);
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

test('all venue resource paths match family naming and reject unsupported race routes',()=>{
 assert.equal(new Set(ENVIRONMENTS).size,40);
 for(const [id,path,scene,mode] of [
  ['env02','/TRACKS/TRACKSA/ENV02/E02_FR.RES','env02fr','env02fr'],
  ['free01','/TRACKS/TRACKSA/FREE01/F01_FR.RES','free01','free01'],
  ['nat08','/TRACKS/TRACKSA/NAT08/N08_FR.RES','nat08','nat08'],
  ['sx16','/TRACKS/TRACKSB/SX16/S16_FR.RES','sx16','sx16'],
  ['training','/TRACKS/TRACKSB/TRAINING/T00_TR.RES','training','training'],
  ['game01','/TRACKS/TRACKSA/GAME01/G01_SO.RES','game01','game01']]) {
   const r=courseResource(id);assert.equal(r.archive,path);assert.equal(r.sceneStem,scene);assert.equal(r.modeStem,mode);
  }
 assert.equal(RACE_COURSES.length,32);
 for(const [id,path,mode] of [['env01','/TRACKS/TRACKSA/ENV01/E01_NAT.RES','env01nat'],['nat01','/TRACKS/TRACKSA/NAT01/N01_NAT.RES','nat01nat'],['sx16','/TRACKS/TRACKSB/SX16/S16_NAT.RES','sx16nat']]) {
  const r=courseResource(id,true);assert.equal(r.archive,path);assert.equal(r.sceneStem,id);assert.equal(r.modeStem,mode);
 }
 for(const id of ['free01','training','game01','unknown'])assert.throws(()=>courseResource(id,true));
});
test('every Freestyle venue except Training maps to its original loading shots', () => {
  assert.equal(loadingShotsResource('env04'), '/UI/LOADICON/ENVB/E04_OB.LSI');
  assert.equal(loadingShotsResource('env08'), '/UI/LOADICON/ENVC/E08_OB.LSI');
  assert.equal(loadingShotsResource('free05'), '/UI/LOADICON/FREE/F05_OB.LSI');
  assert.equal(loadingShotsResource('nat03'), '/UI/LOADICON/NAT/N03_NAT.LSI');
  assert.equal(loadingShotsResource('sx16'), '/UI/LOADICON/SX/S16_NAT.LSI');
  assert.equal(loadingShotsResource('game01'), '/UI/LOADICON/GAME/G01_SO.LSI');
  assert.equal(loadingShotsResource('training'), null);
  assert.equal(new Set(ENVIRONMENTS.map(loadingShotsResource).filter(Boolean)).size, ENVIRONMENTS.length - 1);
});
