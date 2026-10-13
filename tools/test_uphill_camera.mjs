// Ordinary source-course driving; observes authored view adaptation separately
// from the untouched Rust camera. No injected vehicle state or forces.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const {chromium} = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({headless:true});
try {
 const page = await browser.newPage({viewport:{width:390,height:844}}), errors=[];
 page.on('pageerror',e=>errors.push(e.message));
 await page.goto(`${process.env.ATV_TEST_URL || 'http://127.0.0.1:4338'}/play.html?drive=1&environment=env01`);
 await page.waitForFunction(()=>window.atvTerrainStatus?.simulationSteps>60,null,{timeout:120000});
 await page.keyboard.down('KeyW');
 await page.waitForFunction(()=>window.atvTerrainStatus.cameraPresentation.uphillLift>.22,null,{timeout:45000});
 const hill=await page.evaluate(()=>window.atvTerrainStatus);
 const native=Math.atan2(hill.followCamera.target[1]-hill.followCamera.position[1],Math.hypot(hill.followCamera.target[0]-hill.followCamera.position[0],hill.followCamera.target[2]-hill.followCamera.position[2]));
 assert(Math.abs(hill.camera[4]-native-hill.cameraPresentation.pitchLift-hill.cameraPresentation.uphillLift)<1e-6);
 if(process.env.ATV_HILL_SCREENSHOT)await page.screenshot({path:process.env.ATV_HILL_SCREENSHOT});
 await page.keyboard.up('KeyW');
 await page.getByRole('button',{name:'Camera',exact:true}).click();
 await page.waitForFunction(()=>window.atvTerrainStatus.followCamera.preset!==0&&window.atvTerrainStatus.cameraPresentation.uphillLift===0);
 assert.deepEqual(errors,[]);
 console.log(JSON.stringify({lift:hill.cameraPresentation.uphillLift,position:hill.frame.origin,grounded:hill.frame.grounded,nativePitch:native,renderedPitch:hill.camera[4]}));
 console.log('PASS: ordinary Caribou climb raises rendered default chase sightline; camera cycling restores the alternate recovered view.');
} finally {await browser.close();}
