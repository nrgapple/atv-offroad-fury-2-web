import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { contentSource } from './content.js';
const build = '11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49';
function fixture() {
  const chunks = [new Uint8Array([1, 2]), new Uint8Array([3, 4, 5])];
  let offset = 0;
  const records = chunks.map(bytes => {
    const sha256 = createHash('sha256').update(bytes).digest('hex');
    const record = { offset, bytes: bytes.length, sha256, url: `/content/${sha256}.bin` };
    offset += bytes.length; return record;
  });
  return { manifest: { version: 1, source_build_sha256: build, files: [{ path: '/AUDIO/MUSIC/TEST.VAG', lba: 10, bytes: 5, lazy: true, chunks: records }] }, chunks };
}
test('hosted reads cross chunk boundaries, verify hashes and reuse bytes', async () => {
  const { manifest, chunks } = fixture(); let calls = 0;
  const source = contentSource(manifest, async url => { calls++; return chunks[manifest.files[0].chunks.findIndex(chunk => chunk.url === url)]; });
  assert.deepEqual([...await source.file.readRange(20481, 3)], [2, 3, 4]);
  assert.deepEqual([...await source.file.readRange(20480, 5)], [1, 2, 3, 4, 5]);
  assert.equal(calls, 2);
  await assert.rejects(source.file.readRange(20479, 1), /outside/);
});
test('hosted reader rejects altered bytes, foreign URLs and chunk gaps', async () => {
  const { manifest } = fixture();
  const source = contentSource(manifest, async () => new Uint8Array([9, 9]));
  await assert.rejects(source.file.readRange(20480, 1), /integrity/);
  const external = structuredClone(manifest); external.files[0].chunks[0].url = 'https://example.com/content';
  assert.throws(() => contentSource(external, () => {}), /extent/);
  manifest.files[0].chunks[1].offset = 3;
  assert.throws(() => contentSource(manifest, () => {}), /extent/);
});
