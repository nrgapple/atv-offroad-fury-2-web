import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { contentSource, openArchive } from './content.js';
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
test('music streams must be same-origin hash-addressed AAC on soundtrack songs', () => {
  const sha256 = 'a'.repeat(64), stream = { url: `/content/${sha256}.m4a`, bytes: 3, sha256, type: 'audio/mp4', seconds: 187.8 };
  const { manifest } = fixture(); manifest.files[0].stream = stream;
  assert.equal(contentSource(manifest, () => {}).files[0].stream.url, stream.url);
  for (const change of [{ url: 'https://example.com/a.m4a' }, { url: `/content/${sha256}.bin` }, { type: 'audio/ogg' }, { bytes: 0 }, { seconds: 0 }, { sha256: 'b'.repeat(64) }]) {
    const altered = structuredClone(manifest); Object.assign(altered.files[0].stream, change);
    assert.throws(() => contentSource(altered, () => {}), /music stream/);
  }
  const archive = structuredClone(manifest); archive.files[0].path = '/UI/RESOURCE/PAGEA/PAGE14.RES';
  assert.throws(() => contentSource(archive, () => {}), /music stream/);
});
test('loading-screen icon archives are accepted, other extensions are not', () => {
  const { manifest } = fixture();
  manifest.files[0].path = '/UI/LOADICON/ENVA/E01_OB.LSI';
  assert.doesNotThrow(() => contentSource(manifest, () => {}));
  manifest.files[0].path = '/UI/LOADICON/ENVA/E01_OB.EXE';
  assert.throws(() => contentSource(manifest, () => {}), /record/);
});

const words = values => new Uint8Array(new Uint32Array(values).buffer);
const join = parts => { const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0)); parts.reduce((at, p) => (out.set(p, at), at + p.length), 0); return out; };
const listing = members => join(members.flatMap(([name, offset, bytes]) => { const encoded = new TextEncoder().encode(name); return [words([encoded.length]), encoded, words([offset, bytes])]; }));
test('a plain archive reads only its directory and the requested member', async () => {
  const directory = listing([['audio/engine/a.vag', 0, 3], ['audio\\ui\\b.vag', 3, 2]]);
  const archive = join([words([3, 12, 0x100, 0, 0]), words([2, directory.length]), directory, new Uint8Array([1, 2, 3, 4, 5])]);
  const reads = [], base = 7 * 2048, read = async (file, at, count) => { reads.push([at - base, count]); return archive.slice(at - base, at - base + count); };
  const bank = await openArchive({ file: null, read, codec: () => assert.fail('plain members need no Rust extraction'), entry: { lba: 7, bytes: archive.length } });
  assert.deepEqual(bank.entries.map(e => e.name), ['audio/engine/a.vag', 'audio/ui/b.vag']);
  assert.deepEqual([...await bank.member(1)], [4, 5]);
  assert.deepEqual(reads.at(-1), [archive.length - 2, 2]);
});
test('a compressed member is extracted from only the blocks it spans', async () => {
  const blocks = [new Uint8Array(5).fill(1), new Uint8Array(7).fill(2), new Uint8Array(3).fill(3)];
  const table = join([...blocks.map(b => new Uint8Array(new Uint16Array([b.length - 1]).buffer)), new Uint8Array(2)]);
  const directory = listing([['first.vag', 0, 10], ['span.vag', 0x6000 - 4, 8]]);
  const archive = join([words([3, 12 + table.length, 0x340, 0, table.length]), table, words([2, directory.length]), directory, ...blocks]);
  const calls = [], codec = async (op, bytes, index) => { calls.push({ op, bytes, index }); return new Uint8Array([9]); };
  const bank = await openArchive({ file: null, read: async (file, at, count) => archive.slice(at, at + count), codec, entry: { lba: 0, bytes: archive.length } });
  await bank.member(1);
  const { op, bytes, index } = calls.at(-1), view = new DataView(bytes.buffer);
  assert.deepEqual([op, index], [3, 0]);
  assert.deepEqual([0, 1, 2, 3, 4].map(i => view.getUint32(i * 4, true)), [3, 18, 0x340, 0, 6]);
  assert.deepEqual([view.getUint16(20, true), view.getUint16(22, true), view.getUint16(24, true)], [4, 6, 0]);
  assert.deepEqual([view.getUint32(26 + 13, true), view.getUint32(26 + 17, true)], [0x6000 - 4, 8]);
  assert.deepEqual([...bytes.subarray(-12)], [...blocks[0], ...blocks[1]]);
});
test('an unrecognized archive is read whole and extracted by Rust', async () => {
  const archive = new Uint8Array(40); new DataView(archive.buffer).setUint32(0, 2, true);
  const calls = [], codec = async (op, bytes, index) => { calls.push([op, bytes.length, index]); return op === 2 ? new TextEncoder().encode('[{"name":"x.vag","offset":0,"bytes":1}]') : new Uint8Array([9]); };
  const bank = await openArchive({ file: null, read: async (file, at, count) => archive.slice(at, at + count), codec, entry: { lba: 0, bytes: archive.length } });
  assert.deepEqual([bank.entries[0].name, [...await bank.member(0)]], ['x.vag', [9]]);
  assert.deepEqual(calls, [[2, 40, undefined], [3, 40, 0]]);
});
