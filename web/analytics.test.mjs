import assert from 'node:assert/strict';
import test from 'node:test';
import { UsageAnalytics, analyticsAllowed, dataPoint, normalizeEvent } from './analytics.js';

test('events keep only enumerated fields and bounded numbers', () => {
  assert.equal(normalizeEvent({ name: 'unknown' }), null);
  assert.equal(normalizeEvent(['ride_start']), null);
  const event = normalizeEvent({ name: 'ride_start', mode: 'single', activity: 'race', map: 'nat03', vehicle: 'atv', quad: 12.4, seconds: 3.14159,
    player: 'Rider Name', room: 'ABCD1234', url: 'https://example.com/?q', map2: 'env01', device: 'tablet', cpu: 9, laps: 0 });
  assert.deepEqual(event, { name: 'ride_start', mode: 'single', activity: 'race', map: 'nat03', vehicle: 'atv', quad: 12, seconds: 3.14 });
  assert.equal(normalizeEvent({ name: 'ride_start', map: 'secret' }).map, undefined);
  assert.equal(normalizeEvent({ name: 'ride_time', seconds: Infinity }).seconds, undefined);
});

test('data points use a stable column layout without identifiers', () => {
  const point = dataPoint({ name: 'race_finish', map: 'sx02', place: 2, field: 4, seconds: 95.5, laps: 3, cpu: 3 }, { country: 'US', release: 'abcdef0123456789' });
  assert.deepEqual(point.indexes, ['race_finish']);
  assert.deepEqual(point.blobs, ['race_finish', 'US', 'abcdef012345', '', '', '', '', '', 'sx02', '']);
  assert.deepEqual(point.doubles, [0, 95.5, 2, 4, 3, 3, 0]);
  assert.deepEqual(dataPoint({ name: 'session_start' }, { country: '<script>', release: 'main' }).blobs.slice(1, 3), ['', '']);
});

test('automation, local hosts and privacy signals disable collection', () => {
  const scope = (navigator, hostname = 'furyremastered.com', protocol = 'https:') => ({ navigator, location: { hostname, protocol } });
  assert.equal(analyticsAllowed(scope({})), true);
  assert.equal(analyticsAllowed(scope({ webdriver: true })), false);
  assert.equal(analyticsAllowed(scope({ doNotTrack: '1' })), false);
  assert.equal(analyticsAllowed(scope({ globalPrivacyControl: true })), false);
  assert.equal(analyticsAllowed(scope({}, '127.0.0.1')), false);
  assert.equal(analyticsAllowed(scope({}, 'localhost', 'http:')), false);
});

test('riding time accrues only while a visible ride is out of the menu', () => {
  let now = 0; const sent = [];
  const usage = new UsageAnalytics({ send: event => sent.push(event), clock: () => now, enabled: true, context: { device: 'touch', display: 'app' } });
  usage.setRiding(true); now = 5000; // Menu closes before the ride is registered.
  usage.rideStarted({ mode: 'single', activity: 'freestyle', map: 'env02', vehicle: 'atv', quad: 3 }, { seconds: 4.2 });
  assert.deepEqual(sent.shift(), { name: 'ride_start', device: 'touch', display: 'app', mode: 'single', activity: 'freestyle', map: 'env02', vehicle: 'atv', quad: 3, seconds: 4.2 });
  now = 65000; usage.setVisible(false); now = 600000; usage.setVisible(true); now = 630000;
  usage.setRiding(false); now = 700000; usage.report(1);
  assert.equal(sent.length, 1);
  assert.equal(sent[0].name, 'ride_time'); assert.equal(sent[0].seconds, 90); assert.equal(sent[0].map, 'env02');
  usage.report(1); assert.equal(sent.length, 1);
  usage.setRiding(true); now = 710000; usage.rideFailed({ mode: 'single', map: 'env03' });
  assert.deepEqual(sent.slice(1).map(event => [event.name, event.seconds]), [['ride_time', 10], ['ride_failed', undefined]]);
  now = 800000; usage.report(1); assert.equal(sent.length, 3);
});

test('disabled analytics never sends', () => {
  const sent = [];
  const usage = new UsageAnalytics({ send: event => sent.push(event), enabled: false });
  usage.track('session_start'); usage.rideStarted({ map: 'env01' }); usage.report(0);
  assert.equal(sent.length, 0);
});
