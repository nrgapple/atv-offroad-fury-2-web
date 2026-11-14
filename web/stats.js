// Public rider stats page: renders GET /api/stats (cloudflare/stats.mjs).
// Sections without data stay hidden; live counts refresh every minute while visible.
import { statsOptOut } from './analytics.js';

const ENDPOINT = '/api/stats';
const REFRESH_MS = 60_000;
// Tracks listed before "Show all".
const TOP_TRACKS = 5;
const LABELS = {
  modes: ['Single player, multiplayer or Free Ride', { single: 'Single player', multiplayer: 'Multiplayer', freeride: 'Free Ride' }],
  activities: ['Race or Freestyle', { race: 'Race', freestyle: 'Freestyle' }],
  vehicles: ['ATV or motorcycle', { atv: 'ATV', motorcycle: 'Motorcycle' }],
  devices: ['Phone or computer', { touch: 'Phone & tablet', desktop: 'Computer' }],
};

const $ = id => document.getElementById(id);
const number = new Intl.NumberFormat('en');
const compact = new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 });
const count = n => (n >= 1e6 ? compact.format(n) : number.format(n));
const hours = h => (h >= 100 ? count(Math.round(h)) : h.toLocaleString('en', { maximumFractionDigits: 1 }));
const date = (day, options = { month: 'short', day: 'numeric' }) => new Date(`${day}T00:00:00Z`).toLocaleDateString('en', { ...options, timeZone: 'UTC' });
const plural = (n, one, many = `${one}s`) => `${count(n)} ${n === 1 ? one : many}`;
const show = (id, visible) => { $(id).hidden = !visible; };
function element(tag, props = {}, children = []) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}

let stats = null, period = 'allTime', rank = 'hours', allTracks = false;

function renderLive() {
  const { live, peak, daily } = stats;
  show('tile-riders', Boolean(live));
  if (live) {
    $('live-riders').textContent = count(live.riders);
    // The record shows once it means something: two riders at once.
    const notes = [live.rooms > 0 ? plural(live.rooms, 'room') : '', peak?.riders >= 2 ? `record ${count(peak.riders)}` : ''];
    $('live-note').textContent = notes.filter(Boolean).join(' · ');
  }
  show('tile-daily', Boolean(daily?.course));
  if (daily?.course) {
    $('daily-course').textContent = daily.course;
    $('daily-note').textContent = [daily.vehicle === 'motorcycle' ? 'Motorcycles' : 'ATVs', daily.entries ? plural(daily.entries, 'rider') : ''].filter(Boolean).join(' · ');
  }
}

function renderTotals() {
  const totals = stats.totals?.[period];
  for (const id of ['period', 'tile-hours', 'tile-rides', 'hero-more']) show(id, Boolean(totals));
  show('hero', Boolean(totals || stats.live));
  if (!totals) return;
  $('total-hours').textContent = hours(totals.hours);
  $('total-rides').textContent = count(totals.rides);
  $('total-races').textContent = count(totals.racesFinished);
  $('total-sessions').textContent = count(totals.sessions);
}

function renderDays() {
  const days = stats.days ?? [];
  show('days-section', days.some(day => day.rides > 0));
  if (!days.length) return;
  const max = Math.max(1, ...days.map(day => day.rides));
  const today = stats.updated.slice(0, 10);
  const chart = $('days-chart');
  chart.style.setProperty('--days', days.length);
  chart.setAttribute('role', 'img');
  const best = days.reduce((top, day) => (day.rides > top.rides ? day : top), days[0]);
  chart.setAttribute('aria-label', `Rides per day over the last ${days.length} days; busiest ${date(best.day)} with ${plural(best.rides, 'ride')}.`);
  chart.replaceChildren(...days.map(day => {
    const bar = element('span', { className: 'day', tabIndex: -1 }, [
      element('i'),
      element('span', { className: 'day-tip' }, [element('b', { textContent: date(day.day, { weekday: 'short', month: 'short', day: 'numeric' }) }), element('br'), `${plural(day.rides, 'ride')} · ${hours(day.hours)} h`]),
    ]);
    bar.style.setProperty('--h', `${Math.max(day.rides ? 2 : 0, (100 * day.rides) / max)}%`);
    if (day.day === today) bar.dataset.today = '';
    return bar;
  }));
  const axis = element('div', { className: 'days-axis' }, [element('span', { textContent: date(days[0].day) }), element('span', { textContent: days.at(-1).day === today ? 'Today' : date(days.at(-1).day) })]);
  chart.nextElementSibling?.classList.contains('days-axis') ? chart.nextElementSibling.replaceWith(axis) : chart.after(axis);
  $('days-table').tBodies[0].replaceChildren(...days.map(day => element('tr', {}, [
    element('th', { scope: 'row', textContent: date(day.day) }), element('td', { textContent: count(day.rides) }), element('td', { textContent: hours(day.hours) }),
  ])));
}

function renderVenues() {
  const venues = [...(stats.venues ?? [])].sort((a, b) => b[rank] - a[rank]).slice(0, 10);
  show('venues-section', venues.length > 0);
  if (!venues.length) return;
  const max = Math.max(...venues.map(venue => venue[rank])) || 1;
  $('venues').replaceChildren(...venues.map((venue, i) => {
    const fill = element('span', { className: 'bar-fill' });
    fill.style.setProperty('--w', `${Math.max(1, (100 * venue[rank]) / max)}%`);
    return element('li', { hidden: !allTracks && i >= TOP_TRACKS }, [
      element('span', { className: 'bar-name', textContent: venue.name, title: venue.name }),
      element('span', { className: 'bar-value', textContent: rank === 'hours' ? `${hours(venue.hours)} h` : plural(venue.rides, 'ride') }),
      element('span', { className: 'bar-track', ariaHidden: 'true' }, [fill]),
    ]);
  }));
  const more = $('venues-more');
  more.hidden = venues.length <= TOP_TRACKS;
  more.setAttribute('aria-expanded', String(allTracks));
  more.textContent = allTracks ? `Top ${TOP_TRACKS}` : `Show all ${venues.length}`;
}

function renderSplits() {
  const leaders = [];
  const splits = Object.entries(LABELS).flatMap(([field, [title, names]]) => {
    const items = stats[field];
    if (!items) return [];
    const value = item => item.rides ?? item.sessions;
    const total = items.reduce((sum, item) => sum + value(item), 0);
    if (!total) return [];
    const shown = items.filter(item => value(item) > 0);
    const share = item => Math.round((100 * value(item)) / total);
    const series = item => String(items.indexOf(item) + 1);
    const top = shown.reduce((a, b) => (value(b) > value(a) ? b : a));
    leaders.push(`${share(top)}% ${names[top.id]}`);
    return [element('div', { className: 'split', role: 'group', ariaLabel: title }, [
      element('div', { className: 'split-bar', ariaHidden: 'true' }, shown.map(item => {
        const part = element('span');
        part.dataset.series = series(item);
        part.style.setProperty('--share', value(item));
        return part;
      })),
      element('ul', { className: 'split-legend' }, items.map(item => {
        const row = element('li', {}, [element('strong', { textContent: `${share(item)}%` }), names[item.id]]);
        row.dataset.series = series(item);
        return row;
      })),
    ])];
  });
  show('splits-section', splits.length > 0);
  $('splits').replaceChildren(...splits);
  // Race or Freestyle and the vehicle say the most in one line.
  $('splits-teaser').textContent = leaders.slice(1, 3).join(' · ') || leaders[0] || '';
}

function renderBoards() {
  const boards = stats.boards;
  show('boards-section', Boolean(boards));
  if (!boards) return;
  $('board-times').textContent = count(boards.times);
  $('board-riders').textContent = count(boards.riders);
  $('board-daily').textContent = count(boards.dailyEntries);
}

function renderCountries() {
  const countries = stats.countries;
  show('countries-section', Boolean(countries?.count));
  if (!countries?.count) return;
  $('country-count').textContent = count(countries.count);
  let names = null;
  try { names = new Intl.DisplayNames(['en'], { type: 'region' }); } catch { /* Codes only. */ }
  const flag = code => String.fromCodePoint(...[...code].map(letter => 0x1f1a5 + letter.charCodeAt(0)));
  $('country-flags').textContent = countries.top.slice(0, 3).map(({ code }) => flag(code)).join(' ');
  const rest = countries.count - countries.top.length;
  $('countries').replaceChildren(...countries.top.map(({ code }) => element('li', {}, [
    element('span', { className: 'flag', ariaHidden: 'true', textContent: flag(code) }),
    element('span', { className: 'name', textContent: names?.of(code) ?? code }),
  ])), ...(rest > 0 ? [element('li', { className: 'flags-rest', textContent: `+${count(rest)} more` })] : []));
}

function render() {
  for (const step of [renderLive, renderTotals, renderDays, renderVenues, renderSplits, renderBoards, renderCountries]) {
    try { step(); } catch (failure) { console.warn('stats section failed', failure); }
  }
  show('more', ['splits-section', 'countries-section', 'boards-section'].some(id => !$(id).hidden));
}

async function load() {
  try {
    const response = await fetch(ENDPOINT, { headers: { Accept: 'application/json' } });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    stats = await response.json();
    $('stats-status').textContent = '';
    render();
  } catch {
    if (!stats) $('stats-status').textContent = 'Stats are taking a pit stop. Try again in a minute.';
  } finally {
    $('stats').setAttribute('aria-busy', 'false');
  }
}

// A radio group of buttons (period, venue ranking): click or arrow keys select.
function choose(attribute, apply) {
  const buttons = [...document.querySelectorAll(`[data-${attribute}]`)];
  const select = value => {
    for (const button of buttons) {
      const on = button.dataset[attribute] === value;
      button.setAttribute('aria-checked', String(on));
      button.tabIndex = on ? 0 : -1;
    }
    apply(value);
  };
  for (const button of buttons) {
    button.tabIndex = button.getAttribute('aria-checked') === 'true' ? 0 : -1;
    button.addEventListener('click', () => select(button.dataset[attribute]));
    button.addEventListener('keydown', event => {
      const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
      if (!step) return;
      event.preventDefault();
      const next = buttons[(buttons.indexOf(button) + step + buttons.length) % buttons.length];
      next.focus();
      select(next.dataset[attribute]);
    });
  }
}

choose('period', value => { period = value; if (stats) renderTotals(); });
choose('rank', value => { rank = value; if (stats) renderVenues(); });
$('venues-more').addEventListener('click', () => { allTracks = !allTracks; renderVenues(); });
show('opted-out', statsOptOut());

$('stats-status').textContent = 'Loading…';
load();
setInterval(() => { if (!document.hidden) load(); }, REFRESH_MS);
