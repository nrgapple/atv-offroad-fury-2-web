// Regenerate authored icon text sources from SVG. PNG build outputs stay ignored.
import {createRequire} from 'node:module';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const root=new URL('../',import.meta.url), browser=await chromium.launch({headless:true});
const icons={};
mkdirSync(new URL('work/verification/',root),{recursive:true});
try {
 for(const size of [180,512]) {
  const page=await browser.newPage({viewport:{width:size,height:size},deviceScaleFactor:1});
  const svg=readFileSync(new URL('web/app-icon.svg',root),'utf8').replace('width="512" height="512"',`width="${size}" height="${size}"`);
  await page.setContent(`<html><body style="margin:0">${svg}</body></html>`);await page.evaluate(()=>document.fonts.ready);
  const png=await page.screenshot();icons[size===180?'apple-touch-icon.png':'app-icon-512.png']=png.toString('base64');
  writeFileSync(new URL(`work/verification/app-icon-${size}.png`,root),png);await page.close();
 }
 writeFileSync(new URL('web/app-icons.json',root),JSON.stringify(icons,null,2)+'\n');
} finally {await browser.close();}
