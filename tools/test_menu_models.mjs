// Real-WASM original-content UI gate. Reports/screenshots remain private.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createRequire} from 'node:module';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base=process.env.ATV_TEST_URL || 'http://127.0.0.1:4173';
const output=process.env.ATV_MENU_SCREENSHOTS;
const browser=await chromium.launch({headless:true,args:['--use-angle=metal']});
const page=await browser.newPage({viewport:{width:390,height:844},hasTouch:true,isMobile:true});
const errors=[];page.on('pageerror',e=>errors.push(e.message));page.on('response',r=>{if(r.status()>=400)errors.push(`${r.status()} ${r.url()}`);});
const button=name=>page.getByRole('button',{name,exact:true});
const ready=key=>page.waitForFunction(key=>[...document.querySelectorAll('.model-view canvas')].length===2&&[...document.querySelectorAll('.model-view canvas')].every(c=>c.dataset.ready==='true'&&c.dataset.selection===key),key,{timeout:90000});
const shot=async name=>{if(output)await page.screenshot({path:`${output}-${name}.png`});};
const report={viewports:[],models:[],errors};
try {
  await page.goto(new URL('/play.html',base).href);
  await button('Press START').click({timeout:120000});
  await page.waitForFunction(()=>window.atvTerrainStatus.audio.menuMusic.voices>0);
  assert.equal(await page.evaluate(()=>window.atvTerrainStatus.simulation),false);
  const music=await page.evaluate(()=>window.atvTerrainStatus.audio);
  assert.equal(music.music.songs,21);assert.equal(music.voices,0);assert.equal(music.commands,0);assert.equal(music.music.failed,false);
  assert.equal(music.menuMusic.stereo,true);assert.equal(music.menuMusic.track,"UI/uitune1");
  report.menuMusic=music.menuMusic;
  await button('Single Player').click();await button('Quad and Rider').click();await ready('1:0');await page.waitForFunction(()=>document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.startsWith('mrider'));
  for(const viewport of [{width:390,height:844},{width:320,height:568},{width:844,height:390},{width:740,height:360},{width:1280,height:720},{width:768,height:1024}]){
    await page.setViewportSize(viewport);
    await page.waitForFunction(({width,height})=>{const c=document.querySelector('#menu-canvas');return c.width/c.height>width/height-.01&&c.width/c.height<width/height+.01;},viewport);
    const bounds=await page.evaluate(()=>({views:[...document.querySelectorAll('.model-view canvas')].map(c=>({...c.getBoundingClientRect().toJSON(),triangles:Number(c.dataset.triangles),gl:c.dataset.glError})),buttons:[...document.querySelectorAll('#menu-buttons button')].map(b=>({...b.getBoundingClientRect().toJSON(),label:b.getAttribute('aria-label')})),overflow:document.documentElement.scrollWidth>innerWidth}));
    assert.equal(bounds.overflow,false);
    for(const b of [...bounds.views,...bounds.buttons]){assert.ok(b.x>=0&&b.y>=0&&b.right<=viewport.width+1&&b.bottom<=viewport.height+1,JSON.stringify(b));}
    for(const b of bounds.buttons){assert.ok(b.height>=44&&b.width>=44,`${viewport.width}: ${b.label}`);for(const v of bounds.views)assert.ok(b.right<=v.x||b.x>=v.right||b.bottom<=v.y||b.y>=v.bottom,'Controls must not cover models');}
    for(const v of bounds.views){assert.ok(v.width>70&&v.height>90);assert.ok(v.triangles>1000);assert.equal(v.gl,'0');}
    report.viewports.push({viewport,bounds});await shot(`${viewport.width}x${viewport.height}`);
  }
  await page.waitForFunction(()=>document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.includes('mrider_body'));
  const rider=page.locator('.model-view canvas').last();const idleStart=await rider.screenshot();await page.waitForTimeout(650);assert.notDeepEqual(await rider.screenshot(),idleStart,'Original rider motion must change rendered pixels');
  // Touch/pointer orbit and keyboard orbit redraw actual framebuffer pixels.
  await page.setViewportSize({width:390,height:844});await page.waitForTimeout(150);
  const canvas=page.locator('.model-view canvas').first();const before=await canvas.screenshot();const box=await canvas.boundingBox();
  await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.mouse.move(box.x+box.width/2+55,box.y+box.height/2);await page.mouse.up();
  assert.notDeepEqual(await canvas.screenshot(),before);
  await canvas.focus();const rotated=await canvas.screenshot();await page.keyboard.press('ArrowRight');assert.notDeepEqual(await canvas.screenshot(),rotated);
  // Keyboard menu navigation selects the rider row; controller navigation works
  // with the same Rust choices (release after every simulated button press).
  await page.mouse.click(10,80);
  await page.keyboard.press('ArrowDown');await page.keyboard.press('ArrowDown');await page.keyboard.press('Enter');await ready('1:1');await page.waitForFunction(()=>document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.startsWith('frider'));await shot('female-portrait');
  const pad=async index=>{await page.evaluate(index=>{window.__menuPad={connected:true,axes:[0,0,0,0],buttons:Array.from({length:17},(_,i)=>({pressed:i===index,value:i===index?1:0}))};Object.defineProperty(navigator,'getGamepads',{configurable:true,value:()=>[window.__menuPad]});},index);await page.waitForTimeout(120);await page.evaluate(()=>window.__menuPad.buttons.forEach(b=>{b.pressed=false;b.value=0;}));await page.waitForTimeout(100);};
  await pad(15);await ready('2:1');await pad(14);await ready('1:1');
  // All original quads and both original bind rigs are GPU drawn via real UI.
  for(let quad=1;quad<=26;quad++){
    if(quad>1)await button('Next quad').click();
    await ready(`${quad}:1`);
    const views=await page.locator('.model-view canvas').evaluateAll(cs=>cs.map(c=>({key:c.dataset.selection,triangles:Number(c.dataset.triangles),gl:c.dataset.glError})));
    assert.ok(views.every(v=>v.triangles>1000&&v.gl==='0'));report.models.push({quad,views});
  }
  await page.waitForFunction(()=>document.querySelectorAll('.model-view canvas')[1]?.dataset.clip?.startsWith('frider'));await shot('quad26-female');for(let i=0;i<21;i++)await button('Next quad').click();await ready('1:1');
  // Rapid changes cannot show a late response for an old selected model.
  for(let i=0;i<5;i++)await button('Next quad').click();await ready('6:1');
  const view=page.locator('.model-view canvas').first();
  await view.evaluate(c=>{c.__loss=c.getContext('webgl2').getExtension('WEBGL_lose_context');c.__loss.loseContext();});await page.waitForTimeout(100);
  await view.evaluate(c=>c.__loss.restoreContext());await ready('6:1');
  await button('Done').click();assert.equal(await page.locator('.model-view').count(),0);await button('Freestyle').click();
  await button('Ride Caribou Peak').click();await page.waitForFunction(()=>document.querySelector('#frontend').dataset.page==='loading');
  assert.equal(await page.locator('#menu-buttons button').count(),0);await shot('loading-portrait');
  await page.setViewportSize({width:844,height:390});await page.waitForTimeout(100);await shot('loading-landscape');
  await page.waitForFunction(()=>document.querySelector('#frontend').hidden&&window.atvTerrainStatus.simulation,null,{timeout:120000});
  const driven=await page.evaluate(()=>window.atvTerrainStatus);assert.equal(driven.playerModel.quad_id,6);assert.equal(driven.playerModel.rider_id,1);assert.equal(driven.glError,0);
  await page.keyboard.press('Escape');await button('Resume').waitFor();
  const paused=await page.evaluate(()=>window.atvTerrainStatus.audio);assert.equal(paused.musicPreview,false);assert.equal(paused.context,'suspended');
  await button('Quit to Main Menu').click();await page.waitForFunction(()=>window.atvTerrainStatus.audio.musicPreview&&window.atvTerrainStatus.audio.context==='running');
  assert.deepEqual(errors,[]);report.passed=true;console.log(JSON.stringify(report));
  if(process.env.ATV_MENU_REPORT)fs.writeFileSync(process.env.ATV_MENU_REPORT,JSON.stringify(report,null,2));
}finally{await browser.close();}
