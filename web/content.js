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
    if (!/^\/(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_.-]+\.(RES|VAG)$/.test(record.path) || !integer(record.lba) || !integer(record.bytes) || record.bytes === 0 || record.bytes > 128 * 1024 * 1024) throw new Error('Invalid hosted content record.');
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
  async function get(chunk, lazy) {
    if (!cache.has(chunk.url)) {
      const result = (async () => {
        const bytes = new Uint8Array(await fetchBlob(chunk.url));
        if (bytes.byteLength !== chunk.bytes || await digest(bytes) !== chunk.sha256) throw new Error('Hosted content integrity check failed.');
        return bytes;
      })();
      cache.set(chunk.url, { result, lazy });
      result.catch(() => cache.delete(chunk.url));
    }
    const value = cache.get(chunk.url);
    // Retain startup archives and only the two latest soundtrack chunks.
    cache.delete(chunk.url); cache.set(chunk.url, value);
    const music = [...cache].filter(([, item]) => item.lazy);
    for (const [key] of music.slice(0, -2)) cache.delete(key);
    return value.result;
  }
  const file = {
    size: files.reduce((maximum, entry) => Math.max(maximum, entry.lba * 2048 + entry.bytes), 0),
    async readRange(offset, count) {
      if (!integer(offset) || !integer(count) || count > 128 * 1024 * 1024) throw new Error('Invalid content read.');
      const entry = files.find(item => offset >= item.lba * 2048 && offset + count <= item.lba * 2048 + item.bytes);
      if (!entry) throw new Error('Content read is outside the hosted package.');
      const start = offset - entry.lba * 2048, end = start + count, result = new Uint8Array(count);
      for (const chunk of entry.chunks) {
        const low = Math.max(start, chunk.offset), high = Math.min(end, chunk.offset + chunk.bytes);
        if (low >= high) continue;
        const bytes = await get(chunk, entry.lazy);
        result.set(bytes.subarray(low - chunk.offset, high - chunk.offset), low - start);
      }
      return result;
    },
  };
  return { file, files };
}
export async function openHostedContent() {
  const response = await fetch('/content/manifest.json');
  if (!response.ok) throw new Error(`Original content package is unavailable (${response.status}).`);
  return contentSource(await response.json(), async url => {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Original game content could not load (${response.status}).`);
    return response.arrayBuffer();
  });
}
