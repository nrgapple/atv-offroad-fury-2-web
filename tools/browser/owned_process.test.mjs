import { test } from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { spawnOwned, stopOwned } from './owned_process.mjs';

test('timeout cleanup stops a detached descendant but preserves unrelated processes', { timeout: 10000 }, async () => {
  if (process.platform === 'win32') return;
  const unrelated = spawnOwned(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: 'ignore' });
  const source = `const {spawn}=require('node:child_process');
    const c=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{detached:true,stdio:'ignore'});
    console.log(c.pid);setInterval(()=>{},1000);`;
  const parent = spawnOwned(process.execPath, ['-e', source], { stdio: ['ignore', 'pipe', 'ignore'] });
  try {
    const [chunk] = await once(parent.stdout, 'data');
    const descendant = Number(chunk.toString().trim());
    await new Promise(resolve => setTimeout(resolve, 1200));
    const exited = once(parent, 'exit');
    stopOwned(parent, 'SIGKILL');
    await exited;
    assert.equal(unrelated.exitCode, null);
    // A just-killed descendant can be a zombie briefly; it must not still execute.
    const { execFileSync } = await import('node:child_process');
    let state = '';
    for (let attempt = 0; attempt < 20; attempt++) {
      try { state = execFileSync('ps', ['-p', String(descendant), '-o', 'stat='], { encoding: 'utf8' }).trim(); } catch { state = ''; }
      if (!state || state.startsWith('Z')) break;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    assert.ok(!state || state.startsWith('Z'), state);
  } finally {
    stopOwned(parent, 'SIGKILL');
    stopOwned(unrelated, 'SIGKILL');
  }
});

test('refuses unowned children', () => {
  assert.throws(() => stopOwned({ pid: process.pid }), /unowned/);
});
