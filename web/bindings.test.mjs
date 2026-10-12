import test from 'node:test';
import assert from 'node:assert/strict';
import {GameplayBindings,ACTIONS,STORAGE_KEY} from './bindings.js';
const empty=new Set();
function pad(buttons=[],x=0,y=0){return {connected:true,axes:[x,y,0],buttons:Array.from({length:16},(_,i)=>({pressed:buttons.includes(i),value:Number(buttons.includes(i))}))};}
test('native face drive/brake/reverse and independent trigger brake',()=>{
 const b=new GameplayBindings(null);
 assert.equal(b.read(empty,pad([0])).input.throttle,1);
 assert.equal(b.read(empty,pad([2])).input.brake,1);
 let i=b.read(empty,pad([0,2])).input;assert(i.reverse);assert.equal(i.throttle,1);assert.equal(i.brake,0);assert.equal(i.reverse_chord_brake,1,'Rust must receive the chord brake to apply the native speed gate');
 i=b.read(empty,pad([0,2,6])).input;assert(i.reverse);assert.equal(i.brake,1);
 i=b.read(new Set(['KeyS']),pad([0,2])).input;assert.equal(i.brake,1);
 assert.equal(b.read(empty,pad([0],0,1)).input.lean,1,'Down+Cross retains wheelie lean');
 assert.equal(b.read(empty,pad([13])).input.lean,1);assert.equal(b.read(empty,pad([12])).input.lean,-1);
});
test('all 24 original modifier/direction selections use remappable axes',()=>{
 const b=new GameplayBindings(null),seen=new Set();
 for(const mask of [1,2,3,5,6,7])for(const [x,y]of [[0,-1],[0,1],[-1,0],[1,0]]){
  const buttons=[1,3,4].filter((_,i)=>mask&(1<<i));const input=b.read(empty,pad(buttons,x,y)).input;
  assert.equal(input.stunt_modifiers,mask);assert.equal(input.stunt_x,x);assert.equal(input.stunt_y,y);seen.add(`${mask}/${x}/${y}`);
 }
 assert.equal(seen.size,24);
 b.setBinding('keyboard','leanForward',{key:'KeyI'});b.setBinding('keyboard','steerLeft',{key:'KeyU'});
 const i=b.read(new Set(['KeyI','KeyJ']),null).input;assert.equal(i.stunt_y,-1);assert.equal(i.lean,-1);assert.equal(i.stunt_modifiers,1);
 assert.equal(b.read(new Set(['ArrowUp']),null).input.lean,0,'Old stunt direction is not hardcoded');
});
test('signed axis remapping preserves analog range and inversion',()=>{
 const b=new GameplayBindings(null);b.setBinding('controller','throttle',{axis:3,sign:-1});
 const p=pad();p.axes[3]=-.575;assert(Math.abs(b.read(empty,p).input.throttle-.5)<1e-12);
 p.axes[3]=.8;assert.equal(b.read(empty,p).input.throttle,0);
 b.setBinding('controller','leanForward',{axis:1,sign:1});b.setBinding('controller','leanBack',{axis:1,sign:-1});assert.equal(b.read(empty,pad([],0,1)).input.lean,-1);
});
test('reset chord emits once and cancellation cannot synthesize reset or preload release',()=>{
 const b=new GameplayBindings(null);
 assert(!b.read(empty,pad([4])).actions.reset);assert(b.read(empty,pad([4,5])).actions.reset);assert(!b.read(empty,pad([4,5])).actions.reset);
 b.cancel();assert(!b.read(empty,pad([4,5])).actions.reset);b.read(empty,pad([4]));assert(b.read(empty,pad([4,5])).actions.reset);
 b.read(empty,null);assert.equal(b.read(new Set(['Space']),null).input.preload_axis,1);assert.equal(b.read(empty,null).input.preload_axis,-1);assert(!('preload_axis'in b.read(empty,null).input));
 b.read(new Set(['Space']),null);b.cancel();assert(!('preload_axis'in b.read(empty,null).input));
 b.cancel();assert(!('preload_axis'in b.read(new Set(['Space']),null).input));b.read(empty,null);assert.equal(b.read(new Set(['Space']),null).input.preload_axis,1);
 b.cancel();
 assert(!('preload_axis'in b.read(empty,pad([],0,1)).input)&&!('preload_axis'in b.read(empty,pad([],0,-1)).input),'Default controller lean-flick stays native');
});
test('every gameplay action can persist, reload, clear, and restore defaults',()=>{
 const memory=new Map(),storage={getItem:k=>memory.get(k),setItem:(k,v)=>memory.set(k,v)};const b=new GameplayBindings(storage);
 for(const {id}of ACTIONS){b.setBinding('keyboard',id,{key:'KeyZ'});b.setBinding('controller',id,{button:10});}
 const restored=new GameplayBindings(storage);assert(restored.rows('keyboard').every(r=>r.labelBinding==='Z'));assert(restored.rows('controller').every(r=>r.labelBinding==='Button 10'));
 restored.setBinding('keyboard','throttle',[]);assert(!restored.usesKey('KeyW'));assert.equal(restored.read(new Set(['KeyW']),null).input.throttle,0);
 restored.reset('keyboard');assert(restored.usesKey('KeyW'));assert.equal(restored.read(new Set(['KeyW']),null).input.throttle,1);
 for(const binding of [{button:-1},{axis:0,sign:0},{button:Infinity},{chord:[{button:1},{button:1}]}])assert.throws(()=>b.setBinding('controller','throttle',binding));
 assert.throws(()=>b.setBinding('keyboard','unknown',{key:'KeyZ'}));
 memory.set(STORAGE_KEY,'{"keyboard":{}}');assert.equal(new GameplayBindings(storage).read(new Set(['KeyW']),null).input.throttle,1);
 const denied=new GameplayBindings({getItem(){throw Error()},setItem(){throw Error()}});denied.setBinding('keyboard','throttle',{key:'KeyZ'});assert(denied.storageError);assert.equal(denied.read(new Set(['KeyZ']),null).input.throttle,1);
});
test('camera pause edges and disconnected controller are neutral',()=>{
 const b=new GameplayBindings(null);assert(b.read(new Set(['Escape','KeyC']),null).actions.pause);assert(!b.read(new Set(['Escape','KeyC']),null).actions.camera);
 b.cancel();assert(!b.read(new Set(['Escape','KeyC']),null).actions.pause);b.read(empty,null);assert(b.read(new Set(['KeyC']),null).actions.camera);
 const p=pad([0,9]);p.connected=false;assert.equal(b.read(empty,p).input.throttle,0);assert(!b.read(empty,p).actions.pause);
});
test('controller loss and replacement cancel preload without a release edge',()=>{
 const b=new GameplayBindings(null);b.setBinding('controller','preloadHold',{button:5});
 const first=pad();first.id='first';first.index=0;b.read(empty,first);
 first.buttons[5]={value:1,pressed:true};assert.equal(b.read(empty,first).input.preload_axis,1);
 assert(!('preload_axis'in b.read(empty,null).input));
 const second=pad([5]);second.id='second';second.index=1;assert(!('preload_axis'in b.read(empty,second).input));
 second.buttons[5]={value:0,pressed:false};b.read(empty,second);second.buttons[5]={value:1,pressed:true};assert.equal(b.read(empty,second).input.preload_axis,1);
 second.connected=false;assert(!('preload_axis'in b.read(empty,second).input));
});
test('binding updates notify subscribers and cancel a held keyboard preload',()=>{
 const b=new GameplayBindings(null);let updates=0;const unsubscribe=b.subscribe(()=>updates++);
 b.read(new Set(['Space']),null);b.setBinding('keyboard','throttle',{key:'KeyT'});assert.equal(updates,1);assert(!('preload_axis'in b.read(empty,null).input));
 b.reset('keyboard');assert.equal(updates,2);unsubscribe();b.reset('controller');assert.equal(updates,2);
});
test('short keyboard action taps survive between physics batches without consuming preload',()=>{
 const b=new GameplayBindings(null);
 for(const [code,action]of [['KeyC','camera'],['Backspace','reset'],['Escape','pause']]){
  b.observeKeys(new Set([code]),code);b.observeKeys(empty);
  assert(b.read(empty,null).actions[action]);assert(!b.read(empty,null).actions[action]);
 }
 b.observeKeys(new Set(['KeyC']),'KeyC');b.observeKeys(new Set(['KeyC']));
 assert(b.read(new Set(['KeyC']),null).actions.camera);assert(!b.read(new Set(['KeyC']),null).actions.camera);
 b.observeKeys(empty);b.read(empty,null);b.observeKeys(new Set(['KeyC']),'KeyC');b.cancel();assert(!b.read(empty,null).actions.camera);
 b.read(new Set(['Space']),null);b.observeKeys(new Set(['KeyC']),'KeyC');b.observeKeys(empty);
 assert.equal(b.read(empty,null).input.preload_axis,-1,'Action events do not consume the preload release');
});
test('fresh keyboard press immediately after resume does not require a neutral physics sample',()=>{
 const b=new GameplayBindings(null);b.cancel();
 b.observeKeys(new Set(['Escape']),'Escape');b.observeKeys(empty);
 assert(b.read(empty,null).actions.pause);
 b.cancel();b.observeKeys(new Set(['KeyC']));assert(!b.read(new Set(['KeyC']),null).actions.camera,'Repeat is not a new event edge');
 b.cancel();b.observeKeys(new Set(['KeyC']));b.observeKeys(new Set(['KeyC','KeyW']),'KeyW');assert(!b.read(new Set(['KeyC','KeyW']),null).actions.camera,'Unrelated key does not revive a cancelled action');
 b.cancel();b.observeKeys(new Set(['Space']),'Space');assert.equal(b.read(new Set(['Space']),null).input.preload_axis,1);
});
