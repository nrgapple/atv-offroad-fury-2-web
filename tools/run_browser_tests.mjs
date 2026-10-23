#!/usr/bin/env node
// Run classified browser tests (tools/browser/manifest.mjs) against a local static
// server started here, then print a summary. See research/BROWSER_TESTS.md.
//
//   node tools/run_browser_tests.mjs                     # content-free smoke group
//   node tools/run_browser_tests.mjs --group content-free,content
//   node tools/run_browser_tests.mjs --group native      # needs ATV_*_NATIVE reports
//   node tools/run_browser_tests.mjs --only test_fog.mjs,test_race.mjs
//   node tools/run_browser_tests.mjs --group relay --url http://127.0.0.1:8787
//   node tools/run_browser_tests.mjs --list
import { spawn } from 'node:child_process';
import { existsSync, rmSync } from 'node:fs';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { parseArgs } from 'node:util';
import { SELECTABLE, TESTS } from './browser/manifest.mjs';

const ROOT = path.resolve(import.meta.dirname, '..');
const CONTENT = path.join(ROOT, 'private/reports/hosted-content');
const LOGS = path.join(ROOT, 'work/verification/browser-tests');
const DEFAULT_TIMEOUT = 600;

const { values: args } = parseArgs({ options: {
  group: { type: 'string', default: '' },
  only: { type: 'string', default: '' },
  url: { type: 'string', default: '' },
  list: { type: 'boolean', default: false },
  verbose: { type: 'boolean', short: 'v', default: false },
  bail: { type: 'boolean', default: false },
  help: { type: 'boolean', short: 'h', default: false },
} });

if (args.help) {
  console.log(`Usage: run_browser_tests.mjs [--group ${SELECTABLE.join('|')}[,...]] [--only test_x.mjs[,...]]
  [--url <existing site>] [--list] [--verbose] [--bail]
Default group: content-free. Logs: work/verification/browser-tests/<test>.log`);
  process.exit(0);
}

const split = value => value.split(',').map(item => item.trim()).filter(Boolean);
const inGroup = (name, group) => group === 'native' ? Boolean(TESTS[name].native) : TESTS[name].group === group;

if (args.list) {
  for (const group of SELECTABLE) {
    const names = Object.keys(TESTS).filter(name => inGroup(name, group));
    console.log(`\n${group} (${names.length})`);
    for (const name of names) {
      const { note, reason, native, defaults } = TESTS[name];
      console.log(`  ${name}${native ? ` [native: ${Object.keys(native).join(', ')}]` : ''}${defaults ? ` {defaults: ${defaults.join(', ')}}` : ''}${reason ? ` -- ${reason}` : note ? ` -- ${note}` : ''}`);
    }
  }
  process.exit(0);
}

// Each selected run is {name, group}. --only runs named tests in their own group
// (and may name manual/broken tests deliberately).
const runs = [];
if (args.only) {
  for (const name of split(args.only).map(n => n.endsWith('.mjs') ? n : `${n}.mjs`)) {
    if (!TESTS[name]) throw new Error(`${name} is not in tools/browser/manifest.mjs`);
    runs.push({ name, group: TESTS[name].group });
  }
} else {
  for (const group of split(args.group || 'content-free')) {
    if (!SELECTABLE.includes(group)) throw new Error(`Unknown group ${group}; choose from ${SELECTABLE.join(', ')}`);
    for (const name of Object.keys(TESTS)) if (inGroup(name, group)) runs.push({ name, group });
  }
}
if (!runs.length) { console.log('No tests selected.'); process.exit(0); }

if (!process.env.PLAYWRIGHT_MODULE) {
  try { (await import('node:module')).createRequire(import.meta.url).resolve('playwright'); }
  catch { console.error('Set PLAYWRIGHT_MODULE to an installed Playwright package (see research/BROWSER_TESTS.md).'); process.exit(2); }
}

const freePort = () => new Promise((resolve, reject) => {
  const server = createServer().once('error', reject).listen(0, '127.0.0.1', () => {
    const { port } = server.address(); server.close(() => resolve(port));
  });
});

// One server per content mode: 'none' serves an empty content directory so a
// content-free pass proves the test does not depend on private resources.
const servers = new Map();
let emptyContent = null;
async function site(mode) {
  if (args.url) return args.url.replace(/\/+$/, '');
  if (servers.has(mode)) return servers.get(mode).url;
  if (!existsSync(path.join(ROOT, 'web/dist/atv_app_bg.wasm'))) throw new Error('web/dist is missing; run `make web` first.');
  let content = CONTENT;
  if (mode === 'none') content = emptyContent ??= await mkdtemp(path.join(tmpdir(), 'atv-no-content-'));
  else if (!existsSync(path.join(CONTENT, 'manifest.json'))) {
    throw new Error(`Private runtime content is missing (${path.relative(ROOT, CONTENT)}/manifest.json). ` +
      'Content tests need the ignored hosted-content package; see research/BROWSER_TESTS.md.');
  }
  const port = await freePort();
  const child = spawn('python3', ['tools/serve_web.py', '--bind', '127.0.0.1', '--port', String(port), '--content', content],
    { cwd: ROOT, stdio: ['ignore', 'ignore', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', chunk => { stderr += chunk; });
  const url = `http://127.0.0.1:${port}`;
  for (let attempt = 0; ; attempt++) {
    if (child.exitCode !== null) throw new Error(`serve_web.py exited: ${stderr}`);
    try { if ((await fetch(`${url}/index.html`)).ok) break; } catch {}
    if (attempt > 100) throw new Error(`serve_web.py did not start on ${url}`);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  servers.set(mode, { url, child });
  console.log(`Serving web/dist at ${url} (${mode === 'none' ? 'no private content' : 'private content'})`);
  return url;
}

function runTest({ name, group }, url) {
  const entry = TESTS[name];
  const env = { ...process.env, ...entry.env, ATV_TEST_URL: url };
  if (group === 'content-free') env.ATV_CONTENT = 'none';
  if (group !== 'native') env.ATV_NATIVE = 'skip';
  else {
    delete env.ATV_NATIVE;
    for (const [variable, file] of Object.entries(entry.native)) {
      if (!env[variable] && existsSync(path.join(ROOT, file))) env[variable] = path.join(ROOT, file);
    }
  }
  const timeout = (entry.timeout ?? DEFAULT_TIMEOUT) * 1000;
  return new Promise(resolve => {
    const started = Date.now();
    const child = spawn(process.execPath, [path.join('tools', name)], { cwd: ROOT, env, stdio: ['ignore', 'pipe', 'pipe'] });
    running = child;
    let output = '', timedOut = false;
    const collect = stream => chunk => { output += chunk; if (args.verbose) stream.write(chunk); };
    child.stdout.on('data', collect(process.stdout));
    child.stderr.on('data', collect(process.stderr));
    const timer = setTimeout(() => { timedOut = true; child.kill('SIGKILL'); }, timeout);
    child.on('close', async code => {
      clearTimeout(timer);
      await mkdir(LOGS, { recursive: true });
      await writeFile(path.join(LOGS, name.replace(/\.mjs$/, '.log')), output);
      resolve({ ok: code === 0 && !timedOut, code, timedOut, seconds: (Date.now() - started) / 1000, output });
    });
  });
}

const results = [];
let exitCode = 0;
let running = null;
// Interrupted runs must not leave the test or its local servers behind.
for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(signal, () => {
  running?.kill('SIGKILL');
  for (const { child } of servers.values()) child.kill();
  if (emptyContent) rmSync(emptyContent, { recursive: true, force: true });
  process.exit(130);
});
try {
  for (const run of runs) {
    const entry = TESTS[run.name];
    const mode = run.group === 'content-free' ? 'none' : 'private';
    if (['relay'].includes(run.group) && !args.url) {
      console.log(`SKIP ${run.name}: relay tests need --url <site with a multiplayer relay>`);
      results.push({ ...run, status: 'skip' });
      continue;
    }
    if (run.group === 'native') {
      const missing = Object.entries(entry.native)
        .filter(([variable, file]) => variable.endsWith('_NATIVE') && !process.env[variable] && !existsSync(path.join(ROOT, file)))
        .map(([variable, file]) => `${variable} (default ${file})`);
      if (missing.length) console.log(`NOTE ${run.name}: ${missing.join(', ')} unavailable; the test must fail loudly.`);
    }
    process.stdout.write(`RUN  ${run.name} [${run.group}] ... `);
    const result = await runTest(run, await site(mode));
    const status = result.ok ? 'pass' : 'fail';
    console.log(`${status.toUpperCase()} (${result.seconds.toFixed(1)}s${result.timedOut ? ', timed out' : ''})`);
    if (!result.ok && !args.verbose) console.log(result.output.trim().split('\n').slice(-25).map(line => `     ${line.slice(0, 400)}`).join('\n'));
    results.push({ ...run, status, seconds: result.seconds });
    if (!result.ok) { exitCode = 1; if (args.bail) break; }
  }
} finally {
  for (const { child } of servers.values()) child.kill();
  if (emptyContent) await rm(emptyContent, { recursive: true, force: true });
}

console.log('\nSummary');
for (const group of [...new Set(results.map(r => r.group))]) {
  const rows = results.filter(r => r.group === group);
  const count = status => rows.filter(r => r.status === status).length;
  console.log(`  ${group}: ${count('pass')} passed, ${count('fail')} failed, ${count('skip')} skipped`);
  for (const r of rows.filter(r => r.status !== 'pass')) console.log(`    ${r.status.toUpperCase()} ${r.name}`);
}
process.exit(exitCode);
