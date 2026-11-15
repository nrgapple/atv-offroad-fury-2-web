// Host visual regression for MX scene objects marked Hide=True. Keweni Falls'
// source scene places a hidden Type_National start-zone marker at this camera's
// center; before the fix its yellow beam filled 17,637 pixels in the crop.
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const browser = await launch();
try {
  for (const [name, camera] of [
    ['hidden-start', '2304,430,3360,0,0.3,1.7'],
    ['finish-sign', '2304,430,3100,0,0.16,1.7'],
  ]) {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 960, height: 540 }, hasTouch: true }, errors);
    await page.goto(url(`/play.html?venue=mx_nat17&quad=29&graphics=original&weather=off&uiDiagnostics=1&camera=${camera}`));
    await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').step > 5, null, { timeout: 120000 });
    await nextFrames(page, 6);
    const state = JSON.parse(await page.locator('#ui-state').textContent());
    assert.equal(state.ride?.environment, 'mx_nat17');
    const png = await page.screenshot();
    const counts = await page.evaluate(async bytes => {
      const image = await createImageBitmap(new Blob([Uint8Array.from(atob(bytes), c => c.charCodeAt(0))], { type: 'image/png' }));
      const canvas = new OffscreenCanvas(image.width, image.height);
      const ctx = canvas.getContext('2d');
      ctx.drawImage(image, 0, 0);
      const data = ctx.getImageData(0, 0, image.width, image.height).data;
      const count = (bounds, predicate) => {
        let total = 0;
        for (let y = bounds[1]; y < bounds[3]; y++) for (let x = bounds[0]; x < bounds[2]; x++) {
          const i = (y * image.width + x) * 4;
          if (predicate(data[i], data[i + 1], data[i + 2])) total++;
        }
        return total;
      };
      return {
        yellow: count([380, 0, 560, 450], (r, g, b) => r > 179 && g > 153 && b < 158),
        red: count([620, 220, 880, 400], (r, g, b) => r > 89 && r > g * 1.35 && r > b * 1.25),
      };
    }, png.toString('base64'));
    if (name === 'hidden-start') assert.ok(counts.yellow < 5000, `hidden National beam still has ${counts.yellow} yellow pixels`);
    if (name === 'finish-sign') assert.ok(counts.red > 600, `visible finish sign has only ${counts.red} red pixels`);
    if (process.env.ATV_MX_HIDDEN_SCREENSHOT) await writeFile(`${process.env.ATV_MX_HIDDEN_SCREENSHOT}-${name}.png`, png);
    assertNoErrors(errors);
    console.log(`${name}: ${counts.yellow} yellow, ${counts.red} red pixels`);
    await page.close();
  }
  console.log('PASS: hidden Keweni start marker is absent while the finish sign remains visible');
} finally { await browser.close(); }
