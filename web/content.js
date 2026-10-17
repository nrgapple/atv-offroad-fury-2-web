// Hosted original runtime files. No ISO, boot executable or BIOS is requested.
const BUILD = '11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49';
const MAX_BLOB = 25 * 1024 * 1024;
const integer = value => Number.isSafeInteger(value) && value >= 0;
const digest = async bytes => [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(value => value.toString(16).padStart(2, '0')).join('');
export function contentSource(manifest, fetchBlob) {
  if (manifest.version !== 1 || manifest.source_build_sha256 !== BUILD || !Array.isArray(manifest.files) || manifest.files.length > 512) throw new Error('Unsupported hosted content package.');
  const files = [], cache = new Map();
  const validBlob = blob => integer(blob.bytes) && blob.bytes > 0 && blob.bytes <= MAX_BLOB && /^[a-f0-9]{64}$/.test(blob.sha256) && blob.url === `/content/${blob.sha256}.bin`;
  for (const record of manifest.files) {
    if (!/^\/(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_.-]+\.(RES|VAG|LSI)$/.test(record.path) || !integer(record.lba) || !integer(record.bytes) || record.bytes === 0 || record.bytes > 128 * 1024 * 1024) throw new Error('Invalid hosted content record.');
    const chunks = record.chunks ?? [{ ...record, offset: 0 }];
    if (!Array.isArray(chunks) || chunks.length === 0 || chunks.length > 128) throw new Error('Invalid hosted content chunks.');
    let end = 0;
    for (const chunk of chunks) {
      if (!validBlob(chunk) || chunk.offset !== end) throw new Error('Invalid hosted content extent.');
      end += chunk.bytes;
    }
    if (end !== record.bytes) throw new Error('Truncated hosted content record.');
    files.push({ ...record, chunks });
  }
  files.sort((a, b) => a.lba - b.lba);
  for (let i = 1; i < files.length; i++) {
    if (files[i].lba * 2048 < files[i - 1].lba * 2048 + files[i - 1].bytes) throw new Error('Overlapping hosted content records.');
  }
  async function get(chunk, lazy, priority, prefetching = false) {
    if (!cache.has(chunk.url)) {
      const result = (async () => {
        const bytes = new Uint8Array(await fetchBlob(chunk.url, priority));
        if (bytes.byteLength !== chunk.bytes || await digest(bytes) !== chunk.sha256) throw new Error('Hosted content integrity check failed.');
        return bytes;
      })();
      cache.set(chunk.url, { result, lazy, prefetched: prefetching });
      result.catch(() => cache.delete(chunk.url));
    }
    const value = cache.get(chunk.url);
    if (prefetching) return value.result;
    value.prefetched = false;
    // Retain startup archives and only the two latest soundtrack chunks.
    // A prefetched file stays until the read it was fetched for consumes it.
    cache.delete(chunk.url); cache.set(chunk.url, value);
    const music = [...cache].filter(([, item]) => item.lazy && !item.prefetched);
    for (const [key] of music.slice(0, -2)) cache.delete(key);
    return value.result;
  }
  const record = path => files.find(item => item.path.toLowerCase() === path.toLowerCase());
  const file = {
    size: files.reduce((maximum, entry) => Math.max(maximum, entry.lba * 2048 + entry.bytes), 0),
    async readRange(offset, count) {
      if (!integer(offset) || !integer(count) || count > 128 * 1024 * 1024) throw new Error('Invalid content read.');
      const entry = files.find(item => offset >= item.lba * 2048 && offset + count <= item.lba * 2048 + item.bytes);
      if (!entry) throw new Error('Content read is outside the hosted package.');
      const start = offset - entry.lba * 2048, end = start + count;
      // A whole-chunk read shares the verified cached bytes instead of copying
      // them again; readers treat archive bytes as immutable.
      const whole = entry.chunks.find(chunk => chunk.offset === start && chunk.bytes === count);
      if (whole) return get(whole, entry.lazy);
      const result = new Uint8Array(count);
      // Chunks of one read download together.
      await Promise.all(entry.chunks.map(async chunk => {
        const low = Math.max(start, chunk.offset), high = Math.min(end, chunk.offset + chunk.bytes);
        if (low >= high) return;
        const bytes = await get(chunk, entry.lazy);
        result.set(bytes.subarray(low - chunk.offset, high - chunk.offset), low - start);
      }));
      return result;
    },
  };
  // Start downloading files a load is about to read, so they arrive in
  // parallel instead of one request after another. Reads reuse the same
  // in-flight request; a failed prefetch is retried by the real read.
  // `head` fetches only the first chunk, e.g. an archive read by directory.
  function prefetch(paths, priority, { head = false } = {}) {
    for (const path of paths) {
      const entry = path && record(path);
      if (!entry) continue;
      for (const chunk of entry.chunks.slice(0, entry.lazy || head ? 1 : undefined)) get(chunk, entry.lazy, priority, true).catch(() => {});
    }
  }
  return { file, files, prefetch };
}
export async function openHostedContent() {
  const response = await fetch('/content/manifest.json');
  if (!response.ok) throw new Error(`Original content package is unavailable (${response.status}).`);
  return contentSource(await response.json(), async (url, priority) => {
    const response = await fetch(url, priority ? { priority } : undefined);
    if (!response.ok) throw new Error(`Original game content could not load (${response.status}).`);
    return response.arrayBuffer();
  });
}

// Version-3 resource containers, read through their directory so a caller
// downloads only the members it uses. A compressed member is inflated from
// just the 24 KiB blocks it spans, wrapped as a one-member container for the
// Rust extractor. Anything unexpected falls back to reading the whole file.
// `warm(index)` only downloads a member's bytes, for reads that follow soon.
const BLOCK = 0x6000;
function blockResource(lengths, offset, bytes, blocks) {
  const table = 2 * lengths.length + 2, name = 13, out = new Uint8Array(20 + table + 8 + name + blocks.byteLength), view = new DataView(out.buffer);
  [3, 12 + table, 0x340, 0, table].forEach((value, i) => view.setUint32(i * 4, value, true));
  lengths.forEach((length, i) => view.setUint16(20 + i * 2, length - 1, true));
  const directory = 20 + table;
  [1, name, 1].forEach((value, i) => view.setUint32(directory + i * 4, value, true));
  out[directory + 12] = 0x6d; // "m"
  view.setUint32(directory + 13, offset, true); view.setUint32(directory + 17, bytes, true);
  out.set(blocks, directory + 8 + name);
  return out;
}
export async function openArchive({ file, read, codec, entry }) {
  const base = entry.lba * 2048, total = entry.bytes;
  const view = bytes => new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const word = (bytes, at) => at + 4 <= bytes.byteLength ? view(bytes).getUint32(at, true) : NaN;
  const half = (bytes, at) => at + 2 <= bytes.byteLength ? view(bytes).getUint16(at, true) : NaN;
  let head = await read(file, base, Math.min(total, 65536));
  const flags = word(head, 8), known = word(head, 0) === 3 && word(head, 12) === 0 && (flags === 0x100 || flags === 0x340);
  const directory = known ? word(head, 4) + 8 : NaN;
  if (directory >= 20 && directory + 8 <= total) {
    if (directory + 8 > head.byteLength) head = await read(file, base, directory + 8);
    const count = word(head, directory), payload = directory + 8 + word(head, directory + 4);
    if (payload <= total && payload > head.byteLength) head = await read(file, base, payload);
    const names = new TextDecoder(), entries = [];
    for (let at = directory + 8; payload <= head.byteLength && entries.length < count && at + 4 <= payload;) {
      const length = word(head, at);
      if (!(length > 0) || at + 12 + length > payload) break;
      entries.push({ name: names.decode(head.subarray(at + 4, at + 4 + length)).replaceAll('\\', '/'), offset: word(head, at + 4 + length), bytes: word(head, at + 8 + length) });
      at += 12 + length;
    }
    if (count > 0 && entries.length === count) {
      if (flags === 0x100 && entries.every(e => payload + e.offset + e.bytes <= total))
        return { entries, member: index => read(file, base + payload + entries[index].offset, entries[index].bytes), warm: index => read(file, base + payload + entries[index].offset, entries[index].bytes).then(() => {}) };
      const lengths = [], starts = [payload];
      if (flags === 0x340 && word(head, 16) === directory - 20 && directory > 22 && half(head, directory - 2) === 0)
        for (let at = 20; at < directory - 2; at += 2) { lengths.push(half(head, at) + 1); starts.push(starts.at(-1) + lengths.at(-1)); }
      if (lengths.length && starts.at(-1) === total && entries.every(e => e.offset + e.bytes <= lengths.length * BLOCK)) {
        const span = index => { const { offset, bytes } = entries[index], first = Math.floor(offset / BLOCK), last = Math.floor((offset + Math.max(bytes, 1) - 1) / BLOCK); return { offset, bytes, first, last }; };
        const blocks = ({ first, last }) => read(file, base + starts[first], starts[last + 1] - starts[first]);
        return { entries, warm: async index => { await blocks(span(index)); }, async member(index) {
          const member = span(index);
          if (!member.bytes) return new Uint8Array(0);
          return codec(3, blockResource(lengths.slice(member.first, member.last + 1), member.offset - member.first * BLOCK, member.bytes, await blocks(member)), 0);
        } };
      }
    }
  }
  const bytes = head.byteLength === total ? head : await read(file, base, total);
  const entries = JSON.parse(new TextDecoder().decode(await codec(2, bytes)));
  return { entries, member: index => codec(3, bytes, index), warm: async () => {} };
}
