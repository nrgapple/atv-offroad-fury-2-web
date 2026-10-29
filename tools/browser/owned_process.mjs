// Track only descendants of a process we created; identity checks prevent PID reuse.
import { spawn, execFileSync } from 'node:child_process';
const owners = new WeakMap();

function snapshot() {
  if (process.platform === 'win32') return new Map();
  const rows = execFileSync('ps', ['-axo', 'pid=,ppid=,lstart='], { encoding: 'utf8', timeout: 5000 });
  return new Map(rows.trim().split('\n').map(row => {
    const [, pid, parent, identity] = row.match(/^\s*(\d+)\s+(\d+)\s+(.+)$/) || [];
    return [Number(pid), { parent: Number(parent), identity }];
  }).filter(([pid]) => pid));
}

export function spawnOwned(command, args, options) {
  const child = spawn(command, args, { ...options, detached: process.platform !== 'win32' });
  const owned = new Map();
  const capture = () => {
    try {
      const rows = snapshot();
      const root = rows.get(child.pid);
      if (root && (!owned.has(child.pid) || owned.get(child.pid) === root.identity)) {
        owned.set(child.pid, root.identity);
        let changed = true;
        while (changed) {
          changed = false;
          for (const [pid, row] of rows) {
            if (!owned.has(pid) && owned.has(row.parent) && rows.get(row.parent)?.identity === owned.get(row.parent)) {
              owned.set(pid, row.identity); changed = true;
            }
          }
        }
      }
    } catch { /* group cleanup remains available if ps is temporarily unavailable */ }
  };
  capture();
  const timer = setInterval(capture, 1000);
  timer.unref();
  owners.set(child, { owned, capture, timer });
  return child;
}

export function stopOwned(child, signal = 'SIGTERM') {
  if (!child) return;
  const state = owners.get(child);
  if (!state) throw new Error('Refusing to stop an unowned process');
  clearInterval(state.timer);
  state.capture();
  if (process.platform === 'win32') { child.kill(signal); return; }
  const rows = snapshot();
  for (const [pid, identity] of [...state.owned].reverse()) {
    if (rows.get(pid)?.identity === identity) {
      try { process.kill(pid, signal); } catch (error) { if (error.code !== 'ESRCH') throw error; }
    }
  }
}
