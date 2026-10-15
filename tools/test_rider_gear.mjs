// Host/public integration with original resources. Never runs in private-free CI.
// Rider gear page, persistence and previews, ride appearance, and (with
// ATV_TEST_MULTIPLAYER=1 against a relay-backed site) gear seen by other riders.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdirSync} from 'node:fs';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:4212';
const output=process.env.ATV_GEAR_SCREENSHOTS;if(output)mkdirSync(output,{recursive:true});
const browser=await chromium.launch({headless:true});
const errors=[];
const button=(p,name)=>p.getByRole('button',{name,exact:true});
const state=p=>p.evaluate(()=>window.atvTerrainStatus);
const screen=(p,name)=>p.waitForFunction(n=>document.querySelector('#frontend')?.dataset.page===n,name,{timeout:90000});
async function open(viewport,selection) {
 const p=await browser.newPage({viewport,hasTouch:true});p.on('pageerror',e=>errors.push(e.message));
 await p.addInitScript(()=>Object.defineProperty(document,'hasFocus',{value:()=>true}));
 if(selection)await p.addInitScript(value=>localStorage.setItem('atv-player-selection',value),JSON.stringify(selection));
 return p;
}
try {
 // Garage → Rider gear: every row cycles, choices persist, previews redraw.
 for(const [name,viewport] of [['desktop',{width:1280,height:720}],['portrait',{width:390,height:844}],['landscape',{width:844,height:390}]]){
  const p=await open(viewport);
  await p.goto(base+'/play.html');await screen(p,'title');
  await button(p,'Press START').click();await screen(p,'main');
  await button(p,'Single Player').click();await screen(p,'single');
  await button(p,'Quad and Rider').click();await screen(p,'garage');
  await button(p,'Rider gear').click();await screen(p,'gear');
  const label=prefix=>p.locator(`#menu-buttons button[aria-label^="${prefix}:"]`).getAttribute('aria-label');
  for(const prefix of ['Helmet','Jersey','Pants','Gloves','Boots','Goggles','Quad colour']){
   const before=await label(prefix);assert.equal(before,`${prefix}: Original`);
   await p.locator(`#menu-buttons button[aria-label^="${prefix}:"]`).click();
   await p.waitForFunction(([x,v])=>document.querySelector(`#menu-buttons button[aria-label^="${x}:"]`)?.getAttribute('aria-label')!==v,[prefix,before]);
  }
  assert.match(await label('Jersey'),/Jersey: (?!Original)/);
  const hits=await p.locator('#menu-buttons button').evaluateAll(b=>b.map(x=>({label:x.getAttribute('aria-label'),rect:x.getBoundingClientRect().toJSON()})));
  for(const hit of hits)assert(hit.rect.x>=0&&hit.rect.y>=0&&hit.rect.right<=viewport.width+1&&hit.rect.bottom<=viewport.height+1&&hit.rect.height>=34,JSON.stringify(hit));
  const saved=JSON.parse(await p.evaluate(()=>localStorage.getItem('atv-player-selection')));
  assert.deepEqual(saved.appearance,{gear:[1,1,1,1,1,1,1],paint:1});
  await p.waitForFunction(()=>[...document.querySelectorAll('.model-view canvas')].length===2&&[...document.querySelectorAll('.model-view canvas')].every(c=>c.dataset.ready==='true'),null,{timeout:60000});
  if(output)await p.screenshot({path:`${output}/gear-${name}.png`});
  await p.close();
 }
 // Ride with saved gear: the loaded ride carries the appearance.
 const appearance={gear:[3,0,0,5,5,5,0],paint:4};
 const ride=await open({width:1280,height:720},{quadId:14,riderId:0,freestyleEnvironment:'env01',playerName:'Rider',appearance});
 await ride.goto(base+'/play.html?map=env05&quad=14');
 await ride.waitForFunction(()=>window.atvTerrainStatus?.simulation,null,{timeout:180000});
 assert.deepEqual((await state(ride)).rideAppearance,appearance);assert.equal((await state(ride)).glError,0);
 if(output){await ride.keyboard.press('KeyH');await ride.waitForTimeout(1500);await ride.screenshot({path:`${output}/gear-ride.png`});}
 await ride.close();
 if(process.env.ATV_TEST_MULTIPLAYER==='1'){
  // Two riders in one room each see the other's gear and paint.
  const looks=[{gear:[3,0,0,5,5,5,0],paint:4},{gear:[10,2,3,20,20,20,4],paint:9}];
  const pages=[];
  for(const [i,role] of ['Host','Guest'].entries()){
   const p=await open({width:1280,height:720},{quadId:14,riderId:0,freestyleEnvironment:'env01',playerName:role,appearance:looks[i]});pages.push(p);
   await p.goto(base+'/play.html');await button(p,'Press START').click({timeout:90000});await button(p,'Multiplayer').click();
   await button(p,role==='Host'?'Host Game':'Join Game').click();await p.getByRole('textbox',{name:'Rider name',exact:true}).fill(role);
  }
  const [a,b]=pages,code='G'+Date.now().toString(36).toUpperCase().slice(-7);
  await a.getByRole('textbox',{name:'Lobby code (8 letters/numbers, optional)',exact:true}).fill(code);await button(a,'Select Track').click();
  await a.locator('.ms-course[data-track-id=env05]').click({timeout:60000});await a.locator('button.ms-ride').click();
  await a.waitForFunction(()=>window.atvTerrainStatus.simulation&&window.atvTerrainStatus.multiplayer?.state?.code,null,{timeout:120000});
  await b.getByRole('textbox',{name:'Lobby code',exact:true}).fill(code);await button(b,'Join Game').click();
  for(const p of pages)await p.waitForFunction(()=>window.atvTerrainStatus.simulation&&window.atvTerrainStatus.multiplayer?.remote.length===1&&window.atvTerrainStatus.multiplayerDraws>0,null,{timeout:120000});
  for(const [i,p] of pages.entries()){
   const other=looks[1-i],players=(await state(p)).multiplayer.state.lobby.players;
   assert(players.some(q=>JSON.stringify(q.appearance)===JSON.stringify([...other.gear,other.paint])),JSON.stringify(players));
   await p.waitForFunction(()=>window.atvTerrainStatus.remoteGear?.gear===1&&window.atvTerrainStatus.remoteGear?.paint===1,null,{timeout:30000});
  }
  if(output){await b.waitForTimeout(1500);await b.screenshot({path:`${output}/gear-multiplayer.png`});}
 }
 assert.deepEqual(errors,[]);
 console.log(`PASS: rider gear page cycles all original categories and quad colour in three layouts, persists and redraws previews; rides load the saved gear${process.env.ATV_TEST_MULTIPLAYER==='1'?'; both multiplayer riders see each other\'s gear and paint':''}; no page or GL errors.`);
} finally { await browser.close(); }
