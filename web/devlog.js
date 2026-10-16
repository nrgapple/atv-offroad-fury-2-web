const $ = id => document.getElementById(id);
let revision = '', loaded = false;
const date = value => new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
function element(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}
function featureCard(feature) {
  const card = element('article', undefined, 'feature-card');
  card.id = `feature-${feature.id}`;
  card.dataset.category = feature.category;
  card.dataset.status = feature.status;
  const head = element('div', undefined, 'feature-head');
  head.append(element('span', feature.category, 'feature-category'));
  const status = element('span', feature.status, 'feature-status');
  status.dataset.status = feature.status;
  head.append(status);
  card.append(head, element('h3', feature.title), element('p', feature.summary));
  const more = element('details', undefined, 'feature-more');
  more.append(element('summary', 'Evidence and next step'));
  const content = element('div', undefined, 'feature-more-content');
  const scope = element('span', feature.scope, 'feature-scope');
  scope.dataset.scope = feature.scope;
  content.append(scope);
  if (feature.validation) content.append(element('p', feature.validation, 'feature-validation'));
  if (feature.evidence) content.append(element('p', `Evidence: ${feature.evidence}`, 'feature-source'));
  const next = element('p', undefined, 'feature-next');
  next.append(element('strong', 'Next step'), document.createTextNode(feature.next_step));
  content.append(next);
  more.append(content);
  card.append(more);
  return card;
}
function renderPortStatus(port) {
  if (!port || !Array.isArray(port.features)) return;
  $('port-status-note').textContent = port.note;
  const release = $('port-release');
  release.replaceChildren();
  if (port.public_release) {
    const label = element('span', `VERIFIED BASELINE · ${port.public_release.commit.slice(0, 12)}`);
    release.append(label, document.createElement('br'));
    const link = element('a', port.public_release.url);
    link.href = port.public_release.url;
    link.target = '_blank'; link.rel = 'noopener';
    release.append(link, document.createElement('br'));
    release.append(document.createTextNode(`CI ${port.public_release.ci_run}`));
  }
  const statusNames = [
    ['LIVE', 'Live'], ['PARTIAL', 'Partial'],
    ['RECOVERED NOT CONNECTED', 'Recovered, not connected'],
    ['NEEDS RESEARCH', 'Needs research']
  ];
  const counts = new Map(statusNames.map(([status]) => [status, 0]));
  for (const feature of port.features) counts.set(feature.status, (counts.get(feature.status) || 0) + 1);
  $('status-summary').replaceChildren(...statusNames.map(([status, label]) => {
    const cell = element('button', undefined, 'status-count');
    cell.type = 'button';
    cell.dataset.status = status;
    cell.append(element('strong', String(counts.get(status) || 0)), element('span', label));
    cell.onclick = () => {
      const select = $('feature-status');
      select.value = select.value === status ? '' : status;
      filterFeatures();
    };
    return cell;
  }));
  $('board-count').textContent = String(port.features.length);
  $('port-priorities').replaceChildren(...(port.next_priorities || []).map((priority) => element('li', priority)));
  const category = $('feature-category');
  const selectedCategory = category.value;
  category.replaceChildren(new Option('All categories', ''));
  for (const name of [...new Set(port.features.map(feature => feature.category))].sort()) {
    category.add(new Option(name, name));
  }
  if ([...category.options].some(option => option.value === selectedCategory)) category.value = selectedCategory;
  category.onchange = filterFeatures;
  $('feature-status').onchange = filterFeatures;
  const board = $('feature-board');
  board.replaceChildren(...port.features.map(featureCard));
  filterFeatures();
}
function filterFeatures() {
  const cards = [...$('feature-board').querySelectorAll('.feature-card')];
  const category = $('feature-category').value;
  const status = $('feature-status').value;
  let shown = 0;
  for (const card of cards) {
    const visible = (!category || card.dataset.category === category) && (!status || card.dataset.status === status);
    card.hidden = !visible;
    if (visible) shown++;
  }
  $('feature-count').textContent = `Showing ${shown} of ${cards.length} features`;
  for (const chip of $('status-summary').children) chip.setAttribute('aria-pressed', String(chip.dataset.status === status));
  if (cards.length && shown === 0) {
    let empty = $('feature-board').querySelector('.feature-empty');
    if (!empty) { empty = element('p', 'No features match both filters.', 'feature-empty'); $('feature-board').append(empty); }
    empty.hidden = false;
  } else {
    $('feature-board').querySelector('.feature-empty')?.remove();
  }
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
      renderPortStatus(log.port_status);
      const entries = [...log.entries].sort((a, b) => Date.parse(b.timestamp) - Date.parse(a.timestamp));
      // Keep existing cards/images when appending new checkpoints during viewing.
      const old = new Map([...$('entries').children].map(node => [node.id, node]));
      const cards = entries.map(entry => {
        const id = `checkpoint-${entry.id}`, existing = old.get(id), encoded = JSON.stringify(entry);
        if (existing?.dataset.revision === encoded) return existing;
        const node = entryCard(entry); node.dataset.revision = encoded; return node;
      });
      $('entries').replaceChildren(...cards);
      $('count').textContent = String(entries.length);
      if (!revision) revealHashTarget();
      revision = next;
    }
    loaded = true;
  } catch {
    if (!loaded) $('entries').replaceChildren(element('p', 'Could not load the log. Retrying in 15 seconds.', 'empty'));
  }
}
const tabs = { log: [$('tab-log'), $('panel-log')], board: [$('tab-board'), $('port-status')] };
function showTab(name, { focus = false, remember = false } = {}) {
  for (const [key, [tab, panel]] of Object.entries(tabs)) {
    const active = key === name;
    tab.setAttribute('aria-selected', String(active));
    tab.tabIndex = active ? 0 : -1;
    panel.hidden = !active;
    if (active && focus) tab.focus();
  }
  if (remember) history.replaceState(null, '', name === 'board' ? '#board' : location.pathname + location.search);
}
function tabForHash() {
  return location.hash === '#board' || location.hash.startsWith('#feature-') ? 'board' : 'log';
}
function revealHashTarget() {
  showTab(tabForHash());
  const target = location.hash.length > 1 && document.getElementById(decodeURIComponent(location.hash.slice(1)));
  if (target && target.closest('[role=tabpanel]')) target.scrollIntoView();
}
for (const [name, [tab]] of Object.entries(tabs)) {
  tab.addEventListener('click', () => { showTab(name, { remember: true }); scrollTo({ top: Math.min(scrollY, $('main-anchor').offsetTop) }); });
  tab.addEventListener('keydown', event => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    showTab(name === 'log' ? 'board' : 'log', { focus: true, remember: true });
  });
}
addEventListener('hashchange', revealHashTarget);
showTab(tabForHash());
// Long priority lists push the board below the fold on phones; start them collapsed there.
if (matchMedia('(max-width: 600px)').matches) $('next-priorities').open = false;
await refresh();
setInterval(() => { if (!document.hidden) void refresh(); }, 15000);
document.addEventListener('visibilitychange', () => { if (!document.hidden) void refresh(); });
