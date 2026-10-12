// Runtime encounters use decoded original route poses and original COL bodies.
// The setup moves the test ATV only; it never injects velocity, impulses or wrecks.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4192';
const browser=await chromium.launch({headless:true});
try {
 const page=await browser.newPage({viewport:{width:960,height:600}}),errors=[];
 page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  const NativeWorker=Worker;let id=-1000;
  window.Worker=class extends NativeWorker {
   constructor(...args){super(...args);window.encounterWorker=this;this.pending=new Map();this.addEventListener('message',({data})=>{const p=this.pending.get(data.id);if(!p)return;this.pending.delete(data.id);data.error?p.reject(Error(data.error)):p.resolve(JSON.parse(new TextDecoder().decode(data.bytes)));});}
   postMessage(data,...args){if(data.op===67&&data.bytes.length)window.encounterResource={bytes:data.bytes.slice(),index:data.index};super.postMessage(data,...args);}
  };
  window.encounterBinary=(op,bytes,index=0)=>new Promise((resolve,reject)=>{const key=id--;encounterWorker.pending.set(key,{resolve,reject});encounterWorker.postMessage({id:key,op,index,bytes:bytes.slice()});});
  window.encounterCommand=(op,value)=>encounterBinary(op,new TextEncoder().encode(value==null?'':JSON.stringify(value)));
 });
 await page.goto(`${base}/play.html?drive=1&environment=env08`);
 await page.waitForFunction(()=>window.atvTerrainStatus?.ready||document.querySelector('#status.error'),null,{timeout:120000});
 assert((await page.evaluate(()=>window.atvTerrainStatus)).ready,await page.locator('#status').textContent());
 await page.keyboard.press('Escape');await page.getByRole('button',{name:'Resume',exact:true}).waitFor();await page.waitForTimeout(100);
 const cases=[
  {name:'front-land',local:[0,.5,60],direction:1,throttle:0,routeSteps:900},
  {name:'front-launch',local:[0,.5,140],direction:-1,throttle:1,routeSteps:900},
  {name:'side-wreck',local:[17,7,-20],direction:0,throttle:1,routeSteps:30},
  {name:'top-drop',local:[0,25,-10],direction:1,throttle:0,routeSteps:30},
 ];
 for(const test of cases){
  const frames=await page.evaluate(async test=>{
   const startup=await encounterBinary(67,encounterResource.bytes,encounterResource.index);
   await encounterCommand(23,{origin:startup.wheel_matrices[0].slice(12,15),yaw:0});
   const installed=await encounterCommand(69,{});if(installed.bodies!==11)throw Error('Original train bodies missing');
   for(let i=0;i<test.routeSteps;i++)await encounterCommand(68,{dt:1/60});
   const train=await encounterCommand(68,null),m=train.cars[0].matrix;
   const origin=[0,1,2].map(i=>m[12+i]+m[i]*test.local[0]+m[4+i]*test.local[1]+m[8+i]*test.local[2]);
   const forward=test.direction?[m[8]*test.direction,m[10]*test.direction]:[-m[0],-m[2]];
   //This reset must retain the moving bodies in the newly created scene.
   await encounterCommand(23,{origin,yaw:Math.atan2(forward[0],forward[1])});
   const frames=[];
   for(let i=0;i<180;i++)frames.push(...(await encounterCommand(50,{count:4,input:{throttle:test.throttle,brake:0,steer:0,lean:0,reverse:false},orbit:0})).map(s=>s.frame));
   return frames;
  },test);
  assert(frames.every(f=>f.origin.every(Number.isFinite)&&f.linear_velocity.every(Number.isFinite)));
  const contact=frames.findIndex(f=>f.dynamic_contact_count>0);
  assert(contact>=0,`${test.name}: no dynamic body response`);
  assert(frames.some(f=>f.scene_query.selected_dynamic>0));
  if(test.name==='front-land'){
   const air=frames.findIndex((f,i)=>i>contact&&!f.grounded.some(Boolean));
   assert(air>contact);assert(frames.slice(air+1).some(f=>f.grounded.some(Boolean)));
   assert(frames.every(f=>f.wreck.phase==='Riding'));
   assert(Math.max(...frames.map(f=>f.origin[1]))>frames[contact].origin[1]+10);
  } else if(test.name==='front-launch'){
   assert(frames.some((f,i)=>i>contact&&f.linear_velocity[1]>40&&!f.grounded.some(Boolean)&&f.wreck.phase==='Riding'));
   assert(Math.max(...frames.map(f=>f.origin[1]))>frames[contact].origin[1]+50);
  } else {
   assert(frames.some(f=>f.wreck.phase==='Falling'));
   assert(frames.some(f=>f.wreck.phase==='Recovering'));
   assert.equal(frames.at(-1).wreck.phase,'Riding');
   if(test.name==='top-drop')assert(frames.some(f=>f.scene_query.dynamic_wheel_callbacks>0));
  }
  console.log(JSON.stringify({name:test.name,contact,upwardSpeed:Math.max(...frames.map(f=>f.linear_velocity[1])),dynamicContacts:Math.max(...frames.map(f=>f.dynamic_contact_count)),secondaryContacts:Math.max(...frames.map(f=>f.dynamic_secondary_contacts)),phases:[...new Set(frames.map(f=>f.wreck.phase))]}));
 }
 assert.deepEqual(errors,[]);
 console.log('PASS: original front ramp supports a rideable lift/landing and a high relative-speed launch; side/top encounters run body contacts, wreck/recovery and wheel support. Whole-game/hardware parity remains unverified.');
}finally{await browser.close();}
