// Shared Playwright setup for the tools/test_*.mjs browser checks.
//
// Environment:
//   PLAYWRIGHT_MODULE  path or name of the Playwright package (default 'playwright')
//   ATV_TEST_URL       site under test (default http://127.0.0.1:4173, `make serve`)
//   ATV_BROWSER_GPU    WebGL backend for Chromium: 'metal' (macOS default), 'swiftshader'
//                      (default elsewhere, e.g. CI) or 'none' for no flags. Headless Chromium
//                      otherwise has no WebGL 2 and the game cannot start.
//   ATV_NATIVE         'skip' lets native-comparison tests run only their content checks
//   ATV_CONTENT        'none' when the site serves no private content (content-free group)
//
// tools/run_browser_tests.mjs starts a local server and sets these for each group.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

const require = createRequire(import.meta.url);
let module;

/** The Playwright package, resolved once from PLAYWRIGHT_MODULE. */
export function playwright() {
  return module ??= require(process.env.PLAYWRIGHT_MODULE || 'playwright');
}

/** False when the runner serves no private content (ATV_CONTENT=none, content-free group). */
export const contentAvailable = process.env.ATV_CONTENT !== 'none';

/** Base URL of the site under test, without a trailing slash. */
export const base = (process.env.ATV_TEST_URL || 'http://127.0.0.1:4173').replace(/\/+$/, '');

/** Absolute URL for a site path such as '/play.html?drive=1'. */
export const url = (pathname = '/') => new URL(pathname, base + '/').href;

/** Chromium WebGL backend flags selected by ATV_BROWSER_GPU (see above). */
export const METAL_ARGS = ['--enable-gpu', '--use-angle=metal'];
export const SWIFTSHADER_ARGS = ['--enable-unsafe-swiftshader'];
export function gpuArgs() {
  const mode = process.env.ATV_BROWSER_GPU || (process.platform === 'darwin' ? 'metal' : 'swiftshader');
  return mode === 'metal' ? [...METAL_ARGS] : mode === 'swiftshader' ? [...SWIFTSHADER_ARGS] : [];
}

/**
 * Launch a headless browser. `engine` is 'chromium' (default), 'webkit' or 'firefox'.
 * Chromium receives gpuArgs() unless `args` already chooses an ANGLE backend.
 */
export async function launch({ engine = 'chromium', args = [], ...options } = {}) {
  const type = playwright()[engine];
  assert.ok(type, `Unknown Playwright engine ${engine}`);
  const chosen = args.some(arg => arg.startsWith('--use-angle') || arg.includes('swiftshader'));
  const flags = engine === 'chromium' && !chosen ? [...gpuArgs(), ...args] : args;
  return type.launch({ headless: true, ...options, args: flags });
}

/**
 * Record uncaught page exceptions (and optionally console errors) into `errors`.
 * Tests assert the array is empty with assertNoErrors() before reporting PASS.
 */
export function watchErrors(page, errors = [], { console: consoleErrors = false } = {}) {
  page.on('pageerror', error => errors.push(error.message));
  if (consoleErrors) page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  return errors;
}

/** New page (own context) whose page errors are collected into `errors`. */
export async function newPage(browser, options = {}, errors = [], watch = {}) {
  const page = await browser.newPage(options);
  watchErrors(page, errors, watch);
  return page;
}

export function assertNoErrors(errors, message = 'browser errors') {
  assert.deepEqual(errors, [], `${message}: ${errors.join('; ')}`);
}

/** Snapshot of the game diagnostics hook (window.atvTerrainStatus). */
export const gameState = page => page.evaluate(() => window.atvTerrainStatus);

/**
 * Wait until the game reports a loaded scene, failing immediately with the
 * visible status text if loading reports an error instead of timing out.
 * `until` adds an extra page predicate, e.g. `() => window.atvTerrainStatus.simulation`.
 */
export async function waitForGame(page, { timeout = 120000, until = null } = {}) {
  await page.waitForFunction(
    () => window.atvTerrainStatus?.ready || document.querySelector('#status')?.classList.contains('error'),
    null, { timeout });
  const ready = await page.evaluate(() => Boolean(window.atvTerrainStatus?.ready));
  assert.ok(ready, `Game failed to load: ${await page.locator('#status').textContent()}`);
  if (until) await page.waitForFunction(until, null, { timeout });
  return gameState(page);
}

/** Wait until the fixed 60 Hz simulation has advanced by `count` steps. */
export async function waitForSteps(page, count, { timeout = 60000 } = {}) {
  const start = await page.evaluate(() => window.atvTerrainStatus.simulationSteps);
  await page.waitForFunction(target => window.atvTerrainStatus.simulationSteps >= target, start + count, { timeout });
  return start + count;
}

/** Hold a key for a number of simulation steps instead of wall-clock time. */
export async function holdKey(page, key, steps, options) {
  await page.keyboard.down(key);
  try { await waitForSteps(page, steps, options); } finally { await page.keyboard.up(key); }
}

/** Wait for `count` animation frames in the page. */
export async function nextFrames(page, count = 2) {
  await page.evaluate(count => new Promise(resolve => {
    const step = left => left ? requestAnimationFrame(() => step(left - 1)) : resolve();
    step(count);
  }), count);
}

/** Wait until no CSS/Web animation in the document is still running (menu transitions). */
export async function waitForAnimations(page, { timeout = 10000 } = {}) {
  await page.waitForFunction(() => document.getAnimations().every(a => a.playState !== 'running'), null, { timeout });
}

/**
 * Wait until the front-end shows menu page `name`, failing with the menu
 * notice if the front end reports an error instead.
 */
export async function waitForScreen(page, name, { timeout = 90000 } = {}) {
  await page.waitForFunction(name => {
    const frontend = document.querySelector('#frontend');
    return (frontend.dataset.page === name && !frontend.hidden) || frontend.dataset.error;
  }, name, { timeout });
  assert.equal(await page.locator('#frontend').getAttribute('data-error'), null,
    await page.locator('#menu-notice').textContent());
}

/** Exact-name button locator. */
export const button = (page, name) => page.getByRole('button', { name, exact: true });

/** Screenshot only when the given output prefix environment variable is set. */
export async function optionalScreenshot(page, variable, name, options = {}) {
  const prefix = process.env[variable];
  if (prefix) await page.screenshot({ ...options, path: `${prefix}-${name}.png` });
}

/** Write pretty JSON, creating parent directories. */
export async function writeJson(file, value) {
  await mkdir(path.dirname(file), { recursive: true });
  await writeFile(file, JSON.stringify(value, null, 2));
}

/**
 * Load a private original-game comparison report named by `variable`.
 * Without it the test fails loudly, unless ATV_NATIVE=skip explicitly requests
 * a content-only run (the runner's content group); that skip is printed.
 */
export async function nativeData(variable, description) {
  const file = process.env[variable];
  if (!file) {
    if (process.env.ATV_NATIVE === 'skip') {
      console.log(`NATIVE COMPARISON SKIPPED (ATV_NATIVE=skip): ${variable} unset; ${description} not checked.`);
      return null;
    }
    throw new Error(`${variable} must name the private native report for ${description}. ` +
      'Set it to run the original-game comparison, or set ATV_NATIVE=skip to run only the content checks.');
  }
  try {
    return JSON.parse(await readFile(file, 'utf8'));
  } catch (error) {
    throw new Error(`${variable}=${file} is not a readable JSON native report: ${error.message}`);
  }
}
