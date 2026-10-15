// Keeps tools/browser/manifest.mjs in step with the tools/test_*.mjs files. Run by `make check`.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { GROUPS, TESTS } from './manifest.mjs';

const TOOLS = path.resolve(import.meta.dirname, '..');
const files = readdirSync(TOOLS).filter(name => /^test_.*\.mjs$/.test(name)).sort();

test('every browser test file is classified in the manifest', () => {
  const missing = files.filter(name => !TESTS[name]);
  assert.deepEqual(missing, [], `Add these to tools/browser/manifest.mjs: ${missing.join(', ')}`);
});

test('manifest has no entries for removed files', () => {
  const stale = Object.keys(TESTS).filter(name => !files.includes(name));
  assert.deepEqual(stale, [], `Remove stale manifest entries: ${stale.join(', ')}`);
});

test('entries use known groups and explain non-runnable groups', () => {
  for (const [name, entry] of Object.entries(TESTS)) {
    assert.ok(GROUPS.includes(entry.group), `${name}: unknown group ${entry.group}`);
    if (['relay', 'manual', 'broken'].includes(entry.group)) assert.ok(entry.reason?.trim(), `${name}: ${entry.group} needs a reason`);
    if (entry.timeout !== undefined) assert.ok(Number.isFinite(entry.timeout) && entry.timeout > 0, `${name}: bad timeout`);
  }
});

test('native comparisons are declared and load their data loudly', () => {
  for (const name of files) {
    const source = readFileSync(path.join(TOOLS, name), 'utf8');
    const used = [...new Set(source.match(/ATV_[A-Z0-9_]*_NATIVE\b/g) ?? [])].sort();
    const declared = Object.keys(TESTS[name]?.native ?? {});
    assert.deepEqual(declared.filter(variable => variable.endsWith('_NATIVE')).sort(), used, `${name}: manifest native list must match the ATV_*_NATIVE variables it reads`);
    for (const variable of declared) assert.ok(source.includes(variable), `${name}: does not read ${variable}`);
    for (const variable of used) {
      assert.match(source, new RegExp(`nativeData\\('${variable}'`), `${name}: read ${variable} with nativeData() so a missing report fails loudly`);
    }
  }
});

test('browser tests stay readable (format with the command in research/BROWSER_TESTS.md)', () => {
  for (const name of files) {
    const long = readFileSync(path.join(TOOLS, name), 'utf8').split('\n')
      .map((line, index) => [index + 1, line.length]).filter(([, length]) => length > 500);
    assert.deepEqual(long, [], `${name}: lines over 500 characters [line, length]`);
  }
});
