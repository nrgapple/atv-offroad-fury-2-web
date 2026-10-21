// Presentation-interpolation parity for the Rust engine at high refresh
// rates. Plays a script through the native frame loop at a fixed render rate
// (`fury --present-trace --present-hz`, the browser's 60 Hz batching), then
// feeds the delivered states and timings it logged through the JavaScript
// game's own presenters (web/motorcycle-presentation.js: MotorcyclePresentation
// always, AtvPresentation with the preset's interpolation) and compares every
// presented pose and camera. The simulation is bit-identical to the
// JavaScript game (tools/compare_engine.mjs), so the same delivered states are
// the reference input.
//
//   FURY_BIN=... FURY_CONTENT=... node tools/compare_presentation.mjs \
//     [venue] [script.json] [out-dir] [--quad N] [--graphics original|enhanced|high] [--hz 144]
//     [--race LAPS,CPU,SEED]
// A `--log presented.json` instead compares an existing log (e.g. the browser
// build's `?presentTrace=1` capture from tools/test_engine_interpolation.mjs).
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { replay } from './presentation_replay.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const at = args.indexOf(name); return at >= 0 ? args.splice(at, 2)[1] : fallback; };
const quad = Number(option('--quad', 1)), graphics = option('--graphics', 'enhanced'), hz = Number(option('--hz', 144));
const logFile = option('--log', null);
// --race LAPS,CPU,SEED: the race (CPU riders interpolate too).
const race = option('--race', null)?.split(',');
const [venue = 'env01', scriptPath, outArg] = args;
const out = path.resolve(outArg || `private/reports/engine-parity/present-${quad}-${graphics}-${hz}`);
await mkdir(out, { recursive: true });

let log;
if (logFile) log = JSON.parse(await readFile(logFile, 'utf8'));
else {
  if (!process.env.FURY_BIN) throw new Error('FURY_BIN is required');
  const script = scriptPath ? JSON.parse(await readFile(scriptPath, 'utf8')) : [{ frames: 60, input: {} }, { frames: 300, input: { throttle: 1, steer: 0.3 } }];
  const scriptFile = path.join(out, 'script.json');
  await writeFile(scriptFile, JSON.stringify(script));
  const traceFile = path.join(out, 'rust-native-presentation.json');
  const content = process.env.FURY_CONTENT ? ['--content', process.env.FURY_CONTENT] : [];
  execFileSync(process.env.FURY_BIN, [...content, '--venue', venue, '--quad', String(quad), '--graphics', graphics, '--script', scriptFile, '--present-trace', traceFile, '--present-hz', String(hz), ...(race ? ['--race', '--laps', race[0], '--cpu', race[1], '--seed', race[2]] : [])], { stdio: 'inherit' });
  log = JSON.parse(await readFile(traceFile, 'utf8'));
}
const report = { venue, quad, graphics, hz: log.hz, interpolation: log.interpolation, ...replay(log) };
await writeFile(path.join(out, 'report.json'), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
const tolerance = 1e-3;
if (report.mismatched || Object.values(report.worst).some(v => v > tolerance)) { console.error('FAIL: presentation differs from the JavaScript presenters'); process.exit(1); }
if (log.interpolation || quad >= 27 ? report.interpolated === 0 : report.interpolated !== 0) { console.error('FAIL: unexpected interpolation count'); process.exit(1); }
console.log(`PASS: ${report.interpolated}/${report.samples} frames interpolated, matching the JavaScript presenters`);
