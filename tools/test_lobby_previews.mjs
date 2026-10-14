// Opt-in original-content/relay gate: source thumbnails on both lobby clients.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {ENVIRONMENTS} from '../web/launch.js';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE||'playwright');
const base=process.env.ATV_TEST_URL||'http://127.0.0.1:49199';
const browser=await chromium.launch({headless:true});
const host=await browser.newPage({viewport:{width:1280,height:720}});
const guest=await browser.newPage({viewport:{width:390,height:844},hasTouch:true,isMobile:true});
const errors=[];
for(const page of [host,guest]) {
  page.on('pageerror',error=>errors.push(error.message));
  await page.addInitScript(()=>{
    Object.defineProperty(document,'hasFocus',{value:()=>true});
    const NativeWorker=Worker;
    window.Worker=class extends NativeWorker {
      constructor(...args){
        super(...args);this.menus=new Map();
        this.addEventListener('message',({data})=>{
          const request=this.menus.get(data.id);this.menus.delete(data.id);
          if(request&&data.bytes&&!data.error) window.lobbyMenu={request,frame:JSON.parse(new TextDecoder().decode(data.bytes))};
        });
      }
      postMessage(data,...args){
        if(data.op===62){const request=JSON.parse(new TextDecoder().decode(data.bytes));if(request.page==='lobby'){request.environment=window.atvTerrainStatus?.multiplayer?.state.environment;this.menus.set(data.id,request);}}
        super.postMessage(data,...args);
      }
    };
    const clear=CanvasRenderingContext2D.prototype.clearRect,draw=CanvasRenderingContext2D.prototype.drawImage;
    CanvasRenderingContext2D.prototype.clearRect=function(...args){if(this.canvas.id==='menu-canvas')window.lobbyPaints=[];return clear.apply(this,args);};
    CanvasRenderingContext2D.prototype.drawImage=function(image,...args){
      if(this.canvas.id==='menu-canvas'&&args.length===4){
        const preview=window.lobbyMenu?.frame.draw.find(draw=>draw.kind==='preview');
        if(preview&&Math.abs(args[0]-preview.rect[0])<.001&&Math.abs(args[1]-preview.rect[1])<.001)window.lobbyPaints=[];
        (window.lobbyPaints??=[]).push({source:[image.width,image.height],rect:args});
      }
      return draw.call(this,image,...args);
    };
  });
}
const button=(page,name)=>page.getByRole('button',{name,exact:true});
try {
  await host.goto(`${base}/play.html`);await button(host,'Press START').click({timeout:90000});
  await button(host,'Multiplayer').click();await button(host,'Create lobby').click();await button(host,'Create lobby').click();
  await host.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.state.code,null,{timeout:15000});
  const code=await host.evaluate(()=>window.atvTerrainStatus.multiplayer.state.code);
  await guest.goto(`${base}/play.html?room=${code}&action=join&play=1&player=Guest&quad=2`);
  await host.waitForFunction(()=>window.atvTerrainStatus.multiplayer?.state.lobby?.players.length===2,null,{timeout:90000});
  for(const [index,environment] of ENVIRONMENTS.entries()) {
    for(const page of [host,guest]) await page.waitForFunction(environment=>{
      const state=window.atvTerrainStatus.multiplayer?.state,menu=window.lobbyMenu;
      if(state?.environment!==environment||menu?.request.environment!==environment||menu.request.page!=='lobby'||!menu.request.track_name)return false;
      const preview=menu.frame.draw.find(draw=>draw.kind==='preview');
      if(environment==='training')return preview?.image==='training-map-preview'&&(window.lobbyPaints??[]).filter(p=>Math.abs(p.rect[1]-preview.rect[1])<.001).length===1;
      return preview?.image===menu.request.track_image&&(window.lobbyPaints??[]).filter(p=>Math.abs(p.rect[1]-preview.rect[1])<.001).length===2;
    },environment,{timeout:90000});
    const menus=await Promise.all([host,guest].map(page=>page.evaluate(()=>window.lobbyMenu)));
    assert.equal(menus[0].request.track_name,menus[1].request.track_name);
    assert.equal(menus[0].request.track_image,menus[1].request.track_image);
    assert.equal(await button(guest,'Next environment').count(),0,'Only the host changes the map');
    if(environment==='sx01'&&process.env.ATV_LOBBY_SCREENSHOTS)for(const [label,page] of [['desktop',host],['portrait',guest]])await page.screenshot({path:`${process.env.ATV_LOBBY_SCREENSHOTS}-${label}.png`});
    console.log(JSON.stringify({environment,name:menus[0].request.track_name,image:menus[0].request.track_image,clients:2}));
    if(index<ENVIRONMENTS.length-1){await button(host,'Next environment').click();await host.waitForFunction(map=>window.atvTerrainStatus.multiplayer.state.lobby.map===map,index+2,{timeout:15000});}
  }
  assert.deepEqual(errors,[]);
  await button(host,'Leave lobby').click();await guest.waitForFunction(()=>!window.atvTerrainStatus.multiplayer?.state.code,null,{timeout:45000});
  console.log('PASS: all40 lobby selections synchronize names and artwork on desktop/portrait;39 original preview pairs plus the authored Training capture.');
}catch(error){console.error(JSON.stringify({errors,states:await Promise.all([host,guest].map(page=>page.evaluate(()=>({menu:window.lobbyMenu,paint:window.lobbyPaints,state:window.atvTerrainStatus.multiplayer?.state,notice:document.querySelector('#menu-notice')?.textContent})).catch(()=>null)))}));throw error;}finally{await browser.close();}
