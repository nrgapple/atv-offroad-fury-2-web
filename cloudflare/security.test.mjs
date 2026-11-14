import assert from 'node:assert/strict';
import test from 'node:test';
import worker from './worker.mjs';

test('multiplayer is the game server: the site keeps only redirects for the public Free Ride counts', async () => {
  const env = { GAME_SERVER_URL: 'https://mp.furyremastered.com', ASSETS: { fetch: async () => { throw Error('no asset lookups'); } } };
  for (const path of ['/multiplayer/freeride/riders', '/multiplayer/freeride/days']) {
    for (const method of ['GET', 'HEAD']) {
      const r = await worker.fetch(new Request(`https://furyremastered.com${path}`, { method }), env);
      assert.equal(r.status, 302);
      assert.equal(r.headers.get('Location'), `https://mp.furyremastered.com${path}`);
    }
    assert.equal((await worker.fetch(new Request(`https://furyremastered.com${path}?x=1`, { method: 'POST' }), env)).status, 404);
  }
  // The retired relay's rooms, claims and identity are gone.
  for (const [path, method] of [['/multiplayer/rooms', 'POST'], ['/multiplayer/rooms/ABCD2345', 'GET'], ['/multiplayer/freeride', 'POST'], ['/multiplayer/version', 'GET']]) {
    const r = await worker.fetch(new Request(`https://furyremastered.com${path}`, { method, headers: { Origin: 'https://furyremastered.com' } }), env);
    assert.equal(r.status, 404, path);
  }
  // Without a configured game server nothing is redirected.
  assert.equal((await worker.fetch(new Request('https://furyremastered.com/multiplayer/freeride/riders'), { ASSETS: env.ASSETS })).status, 404);
});

test('security headers protect assets/errors/redirects and permit browser WASM and game server sockets', async () => {
  const env = { CANONICAL_ORIGIN: 'https://furyremastered.com', ASSETS: { fetch: async () => new Response('html', { headers: { 'Content-Type': 'text/html' } }) } };
  for (const path of ['https://furyremastered.com/play.html', 'https://www.furyremastered.com/play.html', 'https://furyremastered.com/content/source.iso']) {
    const r = await worker.fetch(new Request(path), env);
    assert.equal(r.headers.get('X-Frame-Options'), 'DENY');
    assert.equal(r.headers.get('Referrer-Policy'), 'no-referrer');
    assert.equal(r.headers.get('X-Content-Type-Options'), 'nosniff');
    assert.match(r.headers.get('Strict-Transport-Security'), /max-age=31536000/);
    assert.match(r.headers.get('Content-Security-Policy'), /'wasm-unsafe-eval'/);
    // The game server's routes and room sockets (research/GAME_SERVER.md);
    // the site itself opens no sockets.
    assert.match(r.headers.get('Content-Security-Policy'), /connect-src 'self' https:\/\/mp\.furyremastered\.com wss:\/\/mp\.furyremastered\.com;/);
    assert.match(r.headers.get('Content-Security-Policy'), /frame-ancestors 'none'/);
  }
});
