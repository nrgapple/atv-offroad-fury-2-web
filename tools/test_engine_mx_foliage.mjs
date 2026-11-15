// Host visual regression for the fully covered near-plant alpha boundary on
// The Glades. The prior 720x405 Original capture had 1,821 green pixels in
// this palm crop. The threshold allows GPU variation while rejecting the
// sparse canopy that prompted this fix. It is not a native/phone comparison.
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const browser = await launch();
try {
  for (const [graphics, minGreen] of [['original', 2100], ['remastered', 2500]]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 720, height: 405 }, hasTouch: true }, errors);
    await page.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([
      { frames: 120, input: {} }, { frames: 240, input: { throttle: 0.4 } },
    ])));
    await page.goto(url(`/play.html?venue=mx_sm03&quad=29&graphics=${graphics}&weather=off&uiDiagnostics=1&traceAll=1&script=storage`));
    await page.waitForSelector('#trace', { state: 'attached', timeout: 240000 });
    const state = JSON.parse(await page.locator('#ui-state').textContent());
    assert.equal(state.ride?.environment, 'mx_sm03');
    const png = await page.screenshot();
    const green = await page.evaluate(async bytes => {
      const binary = atob(bytes);
      const image = await createImageBitmap(new Blob([Uint8Array.from(binary, c => c.charCodeAt(0))], { type: 'image/png' }));
      const canvas = new OffscreenCanvas(image.width, image.height);
      const ctx = canvas.getContext('2d');
      ctx.drawImage(image, 0, 0);
      const pixels = ctx.getImageData(0, 0, image.width, image.height).data;
      let count = 0;
      for (let y = 8; y < 120; y++) for (let x = 200; x < 380; x++) {
        const i = (y * image.width + x) * 4;
        if (pixels[i+1] > pixels[i] * 1.12 && pixels[i+1] > pixels[i+2] * 0.95 && pixels[i+1] > 63) count++;
      }
      return count;
    }, png.toString('base64'));
    assert.ok(green > minGreen, `${graphics}: palm crop has ${green} green pixels, expected >${minGreen}`);
    if (process.env.ATV_MX_FOLIAGE_SCREENSHOT) await writeFile(`${process.env.ATV_MX_FOLIAGE_SCREENSHOT}-${graphics}.png`, png);
    assertNoErrors(errors);
    console.log(`${graphics}: ${green} green palm pixels`);
    await page.close();
  }
  console.log('PASS: The Glades near foliage stays visible in Original and Remastered on the host viewport');
} finally { await browser.close(); }
