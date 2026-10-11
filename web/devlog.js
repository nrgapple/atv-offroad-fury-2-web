const $ = id => document.getElementById(id);
let revision = '', loaded = false;
const date = value => new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
function element(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}
function entryCard(entry) {
  const article = element('article'); article.id = `checkpoint-${entry.id}`;
  const copy = element('div', undefined, 'entry-copy');
  const meta = element('div', undefined, 'entry-meta');
  const kind = element('span', entry.kind, 'kind'); kind.dataset.kind = entry.kind;
  const time = element('time', date(entry.timestamp)); time.dateTime = entry.timestamp;
  meta.append(kind, time); copy.append(meta, element('h2', entry.title), element('p', entry.summary));
  if (entry.details?.length) { const list = element('ul'); for (const item of entry.details) list.append(element('li', item)); copy.append(list); }
  if (entry.limitation) copy.append(element('p', entry.limitation, 'limitation'));
  article.append(copy);
  if (entry.image && /^\/devlog-images\/[a-z0-9-]+\.png$/.test(entry.image)) {
    const figure = element('figure'), link = element('a'), image = element('img');
    link.href = entry.image; link.target = '_blank'; link.rel = 'noopener';
    link.setAttribute('aria-label', `Open full-size screenshot: ${entry.title}`);
    image.src = entry.image; image.alt = entry.caption || entry.title; image.loading = 'lazy'; image.decoding = 'async';
    link.append(image); figure.append(link, element('figcaption', entry.caption || 'Build screenshot · Open for full size')); article.append(figure);
  }
  return article;
}
async function refresh() {
  try {
    const response = await fetch('devlog.json', { cache: 'no-store' });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const log = await response.json();
    if (!Array.isArray(log.entries)) throw new Error('Invalid checkpoint data');
    const next = JSON.stringify(log);
    if (next !== revision) {
      $('build-state').textContent = log.status;
      $('working-on').textContent = log.working_on ? `Current work: ${log.working_on}` : '';
      const entries = [...log.entries].sort((a, b) => Date.parse(b.timestamp) - Date.parse(a.timestamp));
      // Keep existing cards/images when appending new checkpoints during viewing.
      const old = new Map([...$('entries').children].map(node => [node.id, node]));
      const cards = entries.map(entry => {
        const id = `checkpoint-${entry.id}`, existing = old.get(id), encoded = JSON.stringify(entry);
        if (existing?.dataset.revision === encoded) return existing;
        const node = entryCard(entry); node.dataset.revision = encoded; return node;
      });
      $('entries').replaceChildren(...cards);
      $('count').textContent = `${entries.length} recorded`;
      revision = next;
    }
    $('updated').textContent = `Updated ${date(log.updated_at)} · Refreshes every 15 seconds`;
    loaded = true;
  } catch {
    $('updated').textContent = loaded ? 'Refresh unavailable; showing the last loaded checkpoints.' : 'Could not load the log. Retrying in 15 seconds.';
  }
}
await refresh();
setInterval(() => { if (!document.hidden) void refresh(); }, 15000);
document.addEventListener('visibilitychange', () => { if (!document.hidden) void refresh(); });
