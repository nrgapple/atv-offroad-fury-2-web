// Real title/menu and DOM, synthetic platform/install signals. This does not
// perform an OS installation or establish physical iOS behavior.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { assertNoErrors, base, launch, nextFrames, waitForScreen, watchErrors } from './browser/harness.mjs';
const engine = process.env.ATV_INSTALL_BROWSER || 'chromium';
assert.ok(['chromium', 'webkit'].includes(engine));
const output = process.env.ATV_INSTALL_OUTPUT;
if (output) await mkdir(output, { recursive: true });
const browser = await launch({ engine });
const checks = [],
  errors = [];
const contexts = [];
async function title({
  apple = false,
  ipad = false,
  standalone = false,
  display = false,
  blocked = false,
  width = 390,
  height = 844
} = {}) {
  const context = await browser.newContext({ viewport: { width, height }, hasTouch: true });
  contexts.push(context);
  await context.addInitScript(
    ({ apple, ipad, standalone, display, blocked }) => {
      if (apple || ipad)
        Object.defineProperty(navigator, 'userAgent', {
          value: ipad ? 'Mozilla/5.0 (Macintosh) AppleWebKit Safari' : 'Mozilla/5.0 (iPhone) AppleWebKit Safari'
        });
      if (ipad) {
        Object.defineProperty(navigator, 'platform', { value: 'MacIntel' });
        Object.defineProperty(navigator, 'maxTouchPoints', { value: 5 });
      }
      Object.defineProperty(navigator, 'standalone', { value: standalone });
      if (display) {
        const original = window.matchMedia;
        window.matchMedia = query =>
          query === '(display-mode: standalone)'
            ? { matches: true, addEventListener() {} }
            : original.call(window, query);
      }
      if (blocked) {
        const get = Storage.prototype.getItem,
          set = Storage.prototype.setItem;
        Storage.prototype.getItem = function (key) {
          if (key === 'fury-app-install-dismissed-until') throw Error('blocked');
          return get.call(this, key);
        };
        Storage.prototype.setItem = function (key, value) {
          if (key === 'fury-app-install-dismissed-until') throw Error('blocked');
          return set.call(this, key, value);
        };
      }
    },
    { apple, ipad, standalone, display, blocked }
  );
  const page = await context.newPage();
  watchErrors(page, errors);
  await page.goto(base + '/play.html');
  await waitForScreen(page, 'title');
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.transition !== 'entering');
  await page.getByRole('button', { name: 'Press START', exact: true }).waitFor();
  return page;
}
const shot = async (page, name) => {
  if (output) await page.screenshot({ path: `${output}/${name}.png` });
};
async function native(page, outcome = 'accepted', failure = false) {
  await page.evaluate(
    ({ outcome, failure }) => {
      window.installCalls = 0;
      const event = new Event('beforeinstallprompt', { cancelable: true });
      event.prompt = () => {
        window.installCalls++;
        return failure ? Promise.reject(Error('unavailable')) : Promise.resolve();
      };
      event.userChoice = Promise.resolve({ outcome });
      window.dispatchEvent(event);
      window.installPrevented = event.defaultPrevented;
    },
    { outcome, failure }
  );
}
try {
  const page = await title({ apple: true });
  const banner = page.locator('#app-install-banner');
  await banner.waitFor({ state: 'visible' });
  const metadata = await page.evaluate(async () => {
    const manifest = await (await fetch(document.querySelector('link[rel=manifest]').href)).json();
    const apple = document.querySelector('link[rel="apple-touch-icon"]').href;
    const records = [
      { url: apple, size: 180 },
      ...manifest.icons.map(icon => ({ url: icon.src, size: Number(icon.sizes.split('x')[0]) }))
    ];
    for (const { url, size } of records) {
      const response = await fetch(url);
      if (!response.ok || response.headers.get('content-type').split(';')[0] !== 'image/png')
        throw Error('Icon response failed');
      const bytes = await response.arrayBuffer(),
        view = new DataView(bytes);
      if (view.getUint32(16) !== size || view.getUint32(20) !== size) throw Error('Icon dimensions failed');
      const image = new Image();
      image.src = url;
      await image.decode();
      if (image.naturalWidth !== size) throw Error('Icon decode failed');
    }
    return {
      name: manifest.name,
      display: manifest.display,
      start: manifest.start_url,
      scope: manifest.scope,
      apple,
      sizes: records.map(r => r.size)
    };
  });
  assert.equal(metadata.name, 'Fury Remastered');
  assert.equal(metadata.display, 'standalone');
  assert.equal(metadata.start, '/play.html');
  assert.equal(metadata.scope, '/');
  assert.deepEqual(metadata.sizes, [180, 192, 512]);
  assert.equal(new URL(metadata.apple).search, '');
  for (const path of ['/apple-touch-icon.png', '/apple-touch-icon-precomposed.png']) {
    const response = await page.request.get(base + path);
    assert.equal(response.status(), 200);
    assert.match(response.headers()['content-type'], /image\/png/);
  }
  checks.push(
    'manifest identity/start/scope, all three Fury Remastered PNG responses and decoded sizes, conventional Safari root URLs'
  );
  const rects = await page.evaluate(() => {
    const r = id => {
      const x = document.getElementById(id).getBoundingClientRect();
      return { x: x.x, y: x.y, width: x.width, height: x.height, bottom: x.bottom };
    };
    return {
      banner: r('app-install-banner'),
      button: document.querySelector('#menu-buttons button').getBoundingClientRect().toJSON(),
      add: r('app-install-open'),
      dismiss: r('app-install-dismiss')
    };
  });
  assert.ok(rects.banner.bottom < rects.button.y, 'promotion must not cover Press START');
  assert.ok(rects.add.height >= 44 && rects.dismiss.height >= 44, 'touch targets >=44px');
  await shot(page, 'iphone-banner');
  await page.getByRole('button', { name: 'Add app', exact: true }).focus();
  await page.keyboard.press('Enter');
  await page.locator('#app-install-help').waitFor({ state: 'visible' });
  assert.match(await page.locator('#app-install-steps').textContent(), /Share.*Add to Home Screen.*Open as Web App/s);
  assert.equal(
    await page.locator('#frontend').getAttribute('data-page'),
    'title',
    'Enter on Add app must not start game'
  );
  await shot(page, 'iphone-instructions');
  await page.evaluate(() => {
    window.testPad = {
      connected: true,
      axes: [0, 0, 0, 0],
      buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 }))
    };
    Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => [window.testPad] });
    window.testPad.buttons[0] = { pressed: true, value: 1 };
  });
  await nextFrames(page, 21);
  assert.equal(await page.locator('#frontend').getAttribute('data-page'), 'title', 'gamepad blocked behind modal');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('#app-install-help').evaluate(e => e.open), false);
  await nextFrames(page, 9);
  assert.equal(
    await page.locator('#frontend').getAttribute('data-page'),
    'title',
    'held controller stays blocked after modal closes'
  );
  await page.evaluate(() => {
    window.testPad.buttons[0] = { pressed: false, value: 0 };
  });
  await page.getByRole('button', { name: 'Press START', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#frontend').dataset.page === 'main');
  assert.equal(await banner.isVisible(), false);
  await page.keyboard.press('Escape');
  await banner.waitFor({ state: 'visible' });
  await page.getByRole('button', { name: 'Dismiss app banner', exact: true }).click();
  assert.equal(await banner.isVisible(), false);
  await page.reload();
  await page.getByRole('button', { name: 'Press START', exact: true }).waitFor({ timeout: 90000 });
  assert.equal(await banner.isVisible(), false);
  checks.push(
    'iPhone instructions, >=44px targets, no start overlap, keyboard/gamepad isolation, menu-only visibility, dismissal survives reload'
  );
  for (const options of [
    { apple: true, standalone: true },
    { apple: true, display: true }
  ]) {
    const p = await title(options);
    assert.equal(await p.locator('#app-install-banner').isVisible(), false);
    await p.evaluate(() => {
      document.documentElement.style.setProperty('--app-safe-top', '54px');
      document.documentElement.style.setProperty('--app-safe-bottom', '34px');
    });
    await p.waitForFunction(() => {
      const bg = document.querySelector('#menu-background');
      return bg.width > 0 && !bg.hidden && document.querySelector('#menu-stage').getBoundingClientRect().top === 54;
    });
    const bleed = await p.evaluate(() => {
      const bg = document.querySelector('#menu-background'),
        rect = bg.getBoundingClientRect(),
        c = bg.getContext('2d');
      const pixel = y => Array.from(c.getImageData(0, y, 1, 1).data);
      return {
        rect: rect.toJSON(),
        top: pixel(0),
        bottom: pixel(bg.height - 1),
        viewport: { width: innerWidth, height: innerHeight },
        stage: document.querySelector('#menu-stage').getBoundingClientRect().toJSON()
      };
    });
    assert.equal(bleed.rect.y, 0);
    assert.equal(bleed.rect.height, bleed.viewport.height);
    assert.equal(bleed.top[3], 255);
    assert.equal(bleed.bottom[3], 255);
    assert.ok(
      bleed.top[0] > bleed.top[2] && bleed.bottom[0] > bleed.bottom[2],
      'red decoration fills safe areas, no navy strip'
    );
    assert.equal(Math.round(bleed.stage.bottom), bleed.viewport.height - 34);
    await shot(p, options.standalone ? 'standalone-portrait' : 'standalone-display-mode');
    await p.setViewportSize({ width: 844, height: 390 });
    await p.evaluate(() => {
      const s = document.documentElement.style;
      s.setProperty('--app-safe-top', '0px');
      s.setProperty('--app-safe-bottom', '21px');
      s.setProperty('--app-safe-left', '44px');
      s.setProperty('--app-safe-right', '44px');
    });
    await p.waitForFunction(
      () =>
        document.querySelector('#menu-background').getBoundingClientRect().width === 844 &&
        document.querySelector('#menu-stage').getBoundingClientRect().left === 44
    );
    await shot(p, 'standalone-landscape');
    await native(p);
    assert.equal(await p.locator('#app-install-banner').isVisible(), false);
    await p.context().close();
  }
  checks.push(
    'both standalone signals suppress promotion; decoration fills synthetic safe areas edge to edge, controls remain inset after rotation'
  );
  const ipad = await title({ ipad: true, width: 844, height: 390 });
  await ipad.locator('#app-install-banner').waitFor({ state: 'visible' });
  await shot(ipad, 'ipad-landscape');
  await ipad.context().close();
  const small = await title({ apple: true, width: 320, height: 568, blocked: true });
  await small.locator('#app-install-banner').waitFor({ state: 'visible' });
  assert.equal(await small.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  await shot(small, 'small-iphone');
  await small.getByRole('button', { name: 'Dismiss app banner', exact: true }).click();
  assert.equal(await small.locator('#app-install-banner').isVisible(), false);
  await small.context().close();
  checks.push('desktop-identifying iPad, 320px layout, blocked storage fallback');
  for (const [outcome, failure] of [
    ['accepted', false],
    ['dismissed', false],
    ['accepted', true]
  ]) {
    const p = await title();
    assert.equal(await p.locator('#app-install-banner').isVisible(), false);
    await native(p, outcome, failure);
    await p.getByRole('button', { name: 'Install', exact: true }).waitFor();
    assert.equal(await p.evaluate(() => window.installCalls), 0);
    assert.equal(await p.evaluate(() => window.installPrevented), true);
    await p.getByRole('button', { name: 'Install', exact: true }).click();
    assert.equal(await p.evaluate(() => window.installCalls), 1);
    if (failure) await p.locator('#app-install-help').waitFor({ state: 'visible' });
    else await p.locator('#app-install-banner').waitFor({ state: 'hidden' });
    await p.context().close();
  }
  const installed = await title({ apple: true });
  await installed.evaluate(() => window.dispatchEvent(new Event('appinstalled')));
  assert.equal(await installed.locator('#app-install-banner').isVisible(), false);
  await installed.context().close();
  checks.push(
    'feature-detected native prompt requires click, one-shot accept/dismiss/failure lifecycle, appinstalled suppression'
  );
  assertNoErrors(errors);
  console.log(
    JSON.stringify(
      {
        passed: true,
        engine,
        url: base,
        checks,
        limits: 'Synthetic platform and install events; OS Home Screen installation and physical iOS are not automated.'
      },
      null,
      2
    )
  );
} finally {
  await browser.close();
}
