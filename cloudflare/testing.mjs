// Test stand-ins for the Worker's bindings (boards.test.mjs and the browser
// test tools/test_engine_world_submit.mjs): a D1 database over real SQLite
// with the production migrations applied, and a rate limiter. Not part of
// the deployed Worker.
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';

const MIGRATIONS = ['0001_cloud_saves.sql', '0002_world_boards.sql', '0003_board_ghosts.sql'];

// `db` is the SQLite handle; `prepared` counts the statements prepared (one
// per D1 query).
export function d1() {
  const db = new DatabaseSync(':memory:');
  for (const file of MIGRATIONS) db.exec(readFileSync(new URL(`./migrations/${file}`, import.meta.url), 'utf8'));
  const statement = (sql, params = []) => ({
    bind: (...values) => statement(sql, values),
    run: async () => statement(sql, params).exec(),
    all: async () => statement(sql, params).exec(),
    first: async () => statement(sql, params).exec().results[0] ?? null,
    exec() {
      const prepared = db.prepare(sql);
      if (/^\s*SELECT/i.test(sql)) return { results: prepared.all(...params).map(row => ({ ...row })), meta: { changes: 0 } };
      return { results: [], meta: { changes: Number(prepared.run(...params).changes) } };
    },
  });
  const d1 = {
    db,
    prepared: 0,
    prepare: sql => { d1.prepared += 1; return statement(sql); },
    batch: async statements => {
      db.exec('BEGIN');
      try { const results = statements.map(item => item.exec()); db.exec('COMMIT'); return results; } catch (failure) { db.exec('ROLLBACK'); throw failure; }
    },
  };
  return d1;
}

export function limiter(limit = Infinity) {
  const counts = new Map();
  return { limit: async ({ key }) => { counts.set(key, (counts.get(key) ?? 0) + 1); return { success: counts.get(key) <= limit }; } };
}
