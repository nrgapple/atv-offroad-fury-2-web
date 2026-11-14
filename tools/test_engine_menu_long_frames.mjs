// Host-only Chromium x4 measurements for #511. Fresh contexts, 390x844 DPR3 touch.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, unlockStore, url } from './browser/harness.mjs';
const errors = [];
const browser = await launch({ engine: 'chromium' });
try {
  for (let run = 0; run < 3; run++) {
    const page = await newPage(browser, { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true, isMobile: true }, errors);
    await unlockStore(page);
    await page.addInitScript(() => {
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
      window.samples = []; window.tasks = [];
      new PerformanceObserver(l => window.tasks.push(...l.getEntries().map(t => [t.startTime, t.duration]))).observe({entryTypes:['longtask']});
      const watch = () => {
        const el = document.querySelector('#ui-state');
        if (!el) return setTimeout(watch, 10);
        new MutationObserver(() => { const f = JSON.parse(el.textContent).front; if (f) window.samples.push({t:performance.now(), track:f.track, split:f.splitMs, layouts:f.pageLayouts}); }).observe(el,{childList:true,subtree:true,characterData:true});
      }; watch();
    });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Emulation.setCPUThrottlingRate', {rate:4});
    await page.goto(url('/play.html?uiDiagnostics=1&front=tracks'));
    await page.waitForFunction(() => { const f = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front; return f?.page === 'tracks' && !f.motion && f.photos > 0; }, null, {timeout:120000});
    if (process.env.MENU_PROFILE) { await cdp.send('Profiler.enable'); await cdp.send('Profiler.start'); }
    const from = await page.evaluate(() => performance.now());
    for (let i=0;i<10;i++) { await page.keyboard.press('ArrowDown'); await new Promise(r=>setTimeout(r,100)); }
    await page.waitForTimeout(1000);
    if (process.env.MENU_PROFILE) { const {profile} = await cdp.send('Profiler.stop'); await mkdir('work/verification/menu-long-frames', {recursive:true}); await writeFile(`work/verification/menu-long-frames/${process.env.MENU_PROFILE}-${run}.cpuprofile`, JSON.stringify(profile)); }
    const log = await page.evaluate(from => ({tasks:window.tasks.filter(t=>t[0]>=from), samples:window.samples.filter(s=>s.t>=from)}), from);
    const changed = log.samples.filter((s,i,a)=>i && s.track!==a[i-1].track);
    assert.ok(changed.length >= 8, `keys change courses: ${changed.length}`);
    console.log(JSON.stringify({run, tasks:log.tasks, changes:changed.map(s=>s.split), layouts:log.samples.at(-1).layouts-log.samples[0].layouts}));
    await page.context().close();
  }
} finally { await browser.close(); }

assertNoErrors(errors);
