// Opt-in actual UI capture, persisted remapping, music menu and DPR checks.
import assert from 'node:assert/strict';
import {
  assertNoErrors,
  base,
  button,
  launch,
  newPage,
  nextFrames,
  waitForGame,
  waitForScreen,
  waitForSteps
} from './browser/harness.mjs';
const browser = await launch();
const errors = [];
try {
  const page = await newPage(
    browser,
    { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, hasTouch: true },
    errors
  );
  const screen = name => waitForScreen(page, name);
  const click = name => button(page, name).click();
  const state = () => page.evaluate(() => window.atvTerrainStatus);
  const screenshot = async name => {
    if (process.env.ATV_SETTINGS_SCREENSHOTS)
      await page.screenshot({ path: `${process.env.ATV_SETTINGS_SCREENSHOTS}-${name}.png` });
  };
  await page.goto(base + '/play.html?drive=1');
  await waitForGame(page, { until: () => window.atvTerrainStatus.audio.ready });
  // Pause reaches Controls and Audio through the Settings hub (PR #73).
  await click('Pause');
  await screen('pause');
  await click('Settings');
  await screen('settings');
  await click('Controls');
  await screen('controls');
  await click('Keyboard bindings');
  await screen('bindings');
  await page.getByRole('button', { name: /^Gas:/ }).click();
  await page.getByRole('button', { name: 'Cancel', exact: true }).waitFor();
  await page.keyboard.press('KeyT');
  await page.getByRole('button', { name: 'Gas: T', exact: true }).waitFor();
  await screenshot('keyboard');
  assert.equal(
    await page.evaluate(() => document.querySelector('#menu-canvas').width),
    1170,
    'Menu uses 3x device resolution'
  );
  await click('Back');
  await screen('controls');
  await click('Back');
  await screen('settings');
  await click('Back');
  await screen('pause');
  await click('Resume');
  await page.keyboard.down('KeyW');
  await waitForSteps(page, 12);
  assert.equal((await state()).input.throttle, 0);
  await page.keyboard.up('KeyW');
  await page.keyboard.down('KeyT');
  await page.waitForFunction(() => window.atvTerrainStatus.input?.throttle === 1);
  await page.keyboard.up('KeyT');
  await page.waitForFunction(() => window.atvTerrainStatus.input?.throttle === 0);
  // Keyboard capture persists across an actual page reload.
  await page.reload();
  await waitForGame(page, { until: () => window.atvTerrainStatus.simulation });
  await page.locator('#game').focus();
  await page.keyboard.down('KeyT');
  await page.waitForFunction(() => window.atvTerrainStatus.input?.throttle === 1);
  await page.keyboard.up('KeyT');
  await click('Pause');
  await screen('pause');
  await click('Settings');
  await screen('settings');
  await click('Controls');
  await screen('controls');
  await click('Controller settings');
  await screen('controller-settings');
  await click('Controller bindings');
  await screen('bindings');
  await page.evaluate(() => {
    window.settingsPad = {
      connected: true,
      index: 0,
      id: 'settings-test',
      axes: [0, 0, 0, 0],
      buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 }))
    };
    Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => [window.settingsPad] });
  });
  await page.getByRole('button', { name: /^Gas:/ }).click();
  await page.getByRole('button', { name: 'Cancel', exact: true }).waitFor();
  await nextFrames(page, 6);
  await page.evaluate(() => (window.settingsPad.buttons[5] = { pressed: true, value: 1 }));
  await nextFrames(page, 6);
  await page.evaluate(() => (window.settingsPad.buttons[5] = { pressed: false, value: 0 }));
  await page.getByRole('button', { name: 'Gas: R1', exact: true }).waitFor();
  // Signed stick-axis remapping is also captured by actual controller movement.
  await page.getByRole('button', { name: /^Steer left:/ }).click();
  await page.getByRole('button', { name: 'Cancel', exact: true }).waitFor();
  await nextFrames(page, 6);
  await page.evaluate(() => (window.settingsPad.axes[2] = -0.8));
  await nextFrames(page, 6);
  await page.evaluate(() => (window.settingsPad.axes[2] = 0));
  await page.getByRole('button', { name: /^Steer left: Right stick left/ }).waitFor();
  await screenshot('controller');
  for (const button of await page.locator('#menu-buttons button').all())
    assert((await button.boundingBox()).height >= 44);
  await click('Back');
  await screen('controller-settings');
  // Controller Settings: the pad row names the pad in use, sliders persist.
  await page.getByRole('button', { name: 'Controller: Auto (settings-test)', exact: true }).waitFor();
  const slider = async (action, value) => {
    await page.locator(`input[data-setting="${action}"]`).evaluate((input, v) => {
      input.value = String(v);
      input.dispatchEvent(new Event('input', { bubbles: true }));
    }, value);
  };
  await slider('pad-triggers', 50);
  await slider('pad-steering', 150);
  assert.deepEqual(
    await page.evaluate(() => {
      const saved = JSON.parse(localStorage.getItem('atv2-controller-v1'));
      return [saved.triggers, saved.steering, saved.lean, saved.deadzone];
    }),
    [50, 150, 100, 15]
  );
  await click('Controller: Auto (settings-test)');
  await page.getByRole('button', { name: 'Controller: settings-test', exact: true }).waitFor();
  // Presets: the remapped profile is Custom; each original layout is one tap
  // away and Custom comes back after the presets.
  const preset = name => page.getByRole('button', { name: `Preset: ${name}`, exact: true });
  for (const [from, to] of [['Custom', 'Fury Remastered'], ['Fury Remastered', 'ATV Offroad Fury 2'], ['ATV Offroad Fury 2', 'MX vs. ATV Unleashed']]) {
    await preset(from).click();
    await preset(to).waitFor();
  }
  await click('Controller bindings');
  await screen('bindings');
  await page.getByRole('button', { name: 'Gas: Cross', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'Next' }).click();
  await page.getByRole('button', { name: 'Stunt L1: R1', exact: true }).waitFor();
  await click('Back');
  await screen('controller-settings');
  await preset('MX vs. ATV Unleashed').click();
  await preset('Custom').waitFor();
  await screenshot('controller-settings');
  await click('Back');
  await screen('controls');
  await click('Back');
  await screen('settings');
  await click('Back');
  await screen('pause');
  await click('Resume');
  // Windows Chrome often lists an idle virtual pad (Steam Input, DS4Windows)
  // ahead of the real controller; driving must follow the pad in use.
  await page.evaluate(() => {
    const idle = { connected: true, index: 0, id: 'Steam Virtual Gamepad', mapping: 'standard', axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })) };
    window.settingsPad.index = 1;
    Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => [idle, window.settingsPad] });
  });
  // The pinned pad keeps working; 50% gas sensitivity squares a half trigger.
  await page.evaluate(() => (window.settingsPad.buttons[5] = { pressed: true, value: 0.5 }));
  await page.waitForFunction(() => window.atvTerrainStatus.input?.throttle === 0.25);
  await page.evaluate(() => (window.settingsPad = null));
  await page.waitForFunction(() => window.atvTerrainStatus.input?.throttle === 0);
  await click('Pause');
  await screen('pause');
  await click('Settings');
  await screen('settings');
  await click('Controls');
  await screen('controls');
  await click('Controller settings');
  await screen('controller-settings');
  await click('Controller bindings');
  await screen('bindings');
  await click('Clear');
  await page.getByRole('button', { name: 'Gas: Unbound', exact: true }).waitFor();
  await click('Defaults');
  await page.getByRole('button', { name: /^Gas: Cross \/ R2$/ }).waitFor();
  await click('Back');
  await screen('controller-settings');
  await click('Defaults');
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('atv2-controller-v1')).triggers === 100);
  await click('Back');
  await screen('controls');
  await click('Back');
  await screen('settings');
  await click('Audio');
  await screen('music');
  const selected = new Set();
  for (let p = 0; p < 6; p++) {
    for (const id of await page
      .locator('[data-action^="song:"]')
      .evaluateAll(buttons => buttons.map(button => button.dataset.action)))
      selected.add(id);
    if (p === 0) {
      await page.locator('[data-action^="song:"]').first().click();
      await page.waitForFunction(
        () => window.atvTerrainStatus.audio.music.playingIndex === 0 && window.atvTerrainStatus.audio.music.voices > 0
      );
      await screenshot('music');
    }
    if (p < 5) {
      await click('Next');
      await page.waitForFunction(p => document.querySelector('#frontend').dataset.musicPage === String(p), p + 1);
    }
  }
  assert.equal(selected.size, 21, 'All original songs have selector entries');
  await page.waitForFunction(
    () =>
      window.atvTerrainStatus.audio.musicPreview &&
      window.atvTerrainStatus.audio.context === 'running' &&
      window.atvTerrainStatus.audio.music.voices > 0
  );
  // Time-based on purpose: proves the music preview keeps physics paused.
  const frozen = (await state()).frame;
  await page.waitForTimeout(200);
  assert.deepEqual((await state()).frame, frozen, 'Music preview keeps physics paused');
  await click('Music: On');
  await page.waitForFunction(
    () => !window.atvTerrainStatus.audio.music.enabled && window.atvTerrainStatus.audio.music.voices === 0
  );
  await click('Music: Off');
  await page.waitForFunction(() => window.atvTerrainStatus.audio.music.voices > 0);
  await page.locator('.pause-panel [data-action=musicvolume]').click();
  assert.equal((await state()).audio.music.volume, 0.25 * Math.round((await state()).audio.music.volume * 4));
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await page.waitForFunction(
    () => window.atvTerrainStatus.audio.context === 'suspended' && !window.atvTerrainStatus.audio.musicPreview
  );
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await page.waitForFunction(() => window.atvTerrainStatus.audio.musicPreview);
  // Pause keeps the soundtrack (PR #40); resuming the ride ends the preview.
  await click('Back');
  await screen('settings');
  await click('Back');
  await screen('pause');
  await click('Resume');
  await page.waitForFunction(
    () => !window.atvTerrainStatus.audio.musicPreview && document.querySelector('#frontend').hidden
  );
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  assertNoErrors(errors);
  console.log(
    'PASS: keyboard/button/signed-axis capture, actual drive, reload persistence, 21-song menu, silent paused preview/focus/off/on, 3x menu rendering and mobile bounds.'
  );
} finally {
  await browser.close();
}
