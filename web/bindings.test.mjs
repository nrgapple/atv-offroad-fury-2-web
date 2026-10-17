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
 i=b.read(new Set(['KeyS']),pad([0,2])).input;assert.equal(i.brake,0);assert.deepEqual(i.keyboard_drive,{generation:0,held:true,axis:-1},'Keyboard S goes to the Rust brake-then-reverse adapter');
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
 const restored=new GameplayBindings(storage);assert(restored.rows('keyboard').every(r=>r.labelBinding==='Z'));assert(restored.rows('controller').every(r=>r.labelBinding==='L3'));assert(restored.rows('controller',false).every(r=>r.labelBinding==='Button 10'));
 restored.setBinding('keyboard','throttle',[]);assert(!restored.usesKey('KeyW'));assert.equal(restored.read(new Set(['KeyW']),null).input.keyboard_drive.held,false);
 restored.reset('keyboard');assert(restored.usesKey('KeyW'));assert.equal(restored.read(new Set(['KeyW']),null).input.keyboard_drive.axis,1);
 for(const binding of [{button:-1},{axis:0,sign:0},{button:Infinity},{chord:[{button:1},{button:1}]}])assert.throws(()=>b.setBinding('controller','throttle',binding));
 assert.throws(()=>b.setBinding('keyboard','unknown',{key:'KeyZ'}));
 memory.set(STORAGE_KEY,'{"keyboard":{}}');assert.equal(new GameplayBindings(storage).read(new Set(['KeyW']),null).input.keyboard_drive.axis,1);
 const denied=new GameplayBindings({getItem(){throw Error()},setItem(){throw Error()}});denied.setBinding('keyboard','throttle',{key:'KeyZ'});assert(denied.storageError);assert.equal(denied.read(new Set(['KeyZ']),null).input.keyboard_drive.axis,1);
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
test('keyboard gas and brake/reverse are one drive axis; brake wins and arrows mirror WASD',()=>{
 const b=new GameplayBindings(null),drive=keys=>b.read(new Set(keys),null).input;
 for(const [keys,held,axis] of [[[],false,0],[['KeyW'],true,1],[['ArrowUp'],true,1],[['KeyS'],true,-1],[['ArrowDown'],true,-1],[['KeyW','KeyS'],true,-1]]){
  const i=drive(keys);assert.deepEqual(i.keyboard_drive,{generation:b.generation,held,axis});assert.equal(i.throttle,0);assert.equal(i.brake,0);assert.equal(i.reverse,false);
 }
 assert.equal(drive(['ArrowLeft']).steer,-1);assert.equal(drive(['ArrowUp']).lean,0,'Arrow Up drives instead of leaning');
 assert.equal(drive(['ShiftLeft']).lean,1);assert.equal(drive(['KeyQ']).lean,-1);
 b.setBinding('keyboard','reverse',{key:'KeyB'});assert.equal(drive(['KeyB']).keyboard_drive.axis,-1,'An extra reverse key behaves like brake/reverse');
 const c=new GameplayBindings(null);c.cancel();assert.equal(c.read(empty,null).input.keyboard_drive.generation,c.generation);
});
test('keyboard tricks: hold a trick key, then the freshest direction picks the trick',()=>{
 let now=1000;const b=new GameplayBindings(null);b.clock=()=>now;
 const press=(keys,code)=>{b.observeKeys(new Set(keys),code);return new Set(keys);};
 // Gas held from before the jump does not force Up while a direction is chosen.
 press(['KeyW'],'KeyW');now+=500;let keys=press(['KeyW','KeyJ'],'KeyJ');
 let i=b.read(keys,null).input;assert.equal(i.stunt_modifiers,1);assert.deepEqual([i.stunt_x,i.stunt_y],[0,0]);
 now+=60;keys=press(['KeyW','KeyJ','KeyA'],'KeyA');i=b.read(keys,null).input;assert.deepEqual([i.stunt_x,i.stunt_y],[-1,0]);assert.equal(i.keyboard_drive.axis,1,'Gas stays on during the trick');
 // With no fresh direction the held gas key becomes Up after the grace window.
 now+=1000;keys=press(['KeyW'],null);b.read(keys,null);keys=press(['KeyW','KeyK'],'KeyK');
 assert.deepEqual([b.read(keys,null).input.stunt_x,b.read(keys,null).input.stunt_y],[0,0]);
 now+=200;i=b.read(keys,null).input;assert.equal(i.stunt_modifiers,2);assert.deepEqual([i.stunt_x,i.stunt_y],[0,-1]);
 // Brake/reverse is Down, steering is Left/Right, combos keep their native mask.
 now+=1000;keys=press(['KeyJ','KeyL'],'KeyJ');keys=press(['KeyJ','KeyL','KeyS'],'KeyS');i=b.read(keys,null).input;assert.equal(i.stunt_modifiers,5);assert.deepEqual([i.stunt_x,i.stunt_y],[0,1]);
 keys=press(['KeyJ','KeyL','KeyS','KeyD'],'KeyD');i=b.read(keys,null).input;assert.deepEqual([i.stunt_x,i.stunt_y],[1,0]);
 // Without a trick key the stunt axes still follow steer/lean (controller stick).
 i=b.read(empty,pad([1],0,-1)).input;assert.equal(i.stunt_modifiers,1);assert.deepEqual([i.stunt_x,i.stunt_y],[0,-1]);
});
test('saved v1 keyboard profiles adopt new defaults but keep customized rows',()=>{
 const v1={keyboard:{throttle:[{key:'KeyW'}],brake:[{key:'KeyS'}],steerLeft:[{key:'KeyA'},{key:'ArrowLeft'}],steerRight:[{key:'KeyD'},{key:'ArrowRight'}],leanForward:[{key:'KeyQ'},{key:'ArrowUp'}],leanBack:[{key:'KeyE'},{key:'ArrowDown'}],reverse:[{key:'KeyR'}],stuntCircle:[{key:'KeyJ'}],stuntTriangle:[{key:'KeyK'}],stuntL1:[{key:'KeyL'}],clutch:[{key:'KeyV'}],preloadHold:[{key:'KeyP'}],camera:[{key:'KeyC'}],cameraLeft:[],cameraRight:[],reset:[{key:'Backspace'}],pause:[{key:'Escape'}]},controller:{throttle:[{button:5}]}};
 const memory=new Map([[STORAGE_KEY,JSON.stringify(v1)]]),storage={getItem:k=>memory.get(k),setItem:(k,v)=>memory.set(k,v)};
 const b=new GameplayBindings(storage),row=id=>b.rows('keyboard').find(r=>r.id===id).labelBinding;
 assert.equal(row('throttle'),'W / ArrowUp');assert.equal(row('reverse'),'Unbound');assert.equal(row('reset'),'R / Backspace');
 assert.equal(row('preloadHold'),'P','Customized row is kept');assert.equal(row('pause'),'Escape','New default drops a key a custom row already uses');
 assert.equal(b.rows('controller').find(r=>r.id==='throttle').labelBinding,'R1');
 assert.equal(JSON.parse(memory.get(STORAGE_KEY)).version,2);
 b.setBinding('keyboard','leanForward',{key:'ArrowUp'});assert.equal(new GameplayBindings(storage).rows('keyboard').find(r=>r.id==='leanForward').labelBinding,'ArrowUp','v2 profiles are not migrated again');
 assert.equal(b.rows('keyboard').find(r=>r.id==='brake').label,'Brake / reverse');assert.equal(b.rows('controller').find(r=>r.id==='brake').label,'Brake');
});
