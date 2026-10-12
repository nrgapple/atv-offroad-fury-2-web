#!/usr/bin/env node
// Exercise the real development-log feature board at a host or public URL.
// This check does not load the game runtime or any original game data.
import assert from 'node:assert/strict';
import process from 'node:process';
import { createRequire } from 'node:module';

const base = process.env.ATV_TEST_URL;
if (!base) throw new Error('Set ATV_TEST_URL to the host or public site base URL.');
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ headless: true });
const errors = [];
const observeErrors = page => {
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text());
  });
};

try {
  const desktop = await browser.newPage({ viewport: { width: 1440, height: 1080 } });
  observeErrors(desktop);
  await desktop.goto(new URL('/devlog.html', base).href, { waitUntil: 'networkidle' });
  await desktop.waitForFunction(() => document.querySelector('#feature-count')?.textContent.includes('features'));
  const metadata = await desktop.evaluate(async () => {
    const response = await fetch('/devlog.json', { cache: 'no-store' });
    if (!response.ok) throw new Error(`devlog metadata HTTP ${response.status}`);
    return response.json();
  });
  const features = metadata.port_status?.features;
  assert.ok(Array.isArray(features) && features.length > 0, 'metadata must define feature areas');
  const featureCount = features.length;
  const trainFeature = features.find(feature => feature.id === 'arlington-train');
  assert.ok(trainFeature, 'metadata must define the Arlington train feature');
  assert.match(await desktop.locator('#feature-count').textContent(), new RegExp(`^Showing ${featureCount} of ${featureCount} features$`));
  assert.match(await desktop.locator('#port-status-note').textContent(), /not percent completion/i);
  const statusCounts = await desktop.locator('.status-count strong').allTextContents();
  assert.equal(statusCounts.reduce((sum, value) => sum + Number(value), 0), featureCount, 'status counts must add up to all feature areas');
  assert.equal(await desktop.locator('#feature-arlington-train').count(), 1);
  assert.ok(await desktop.locator('#feature-crash-recovery .feature-scope').textContent());
  assert.equal(await desktop.locator('#feature-arlington-train').getAttribute('data-status'), trainFeature.status);

  await desktop.locator('#feature-category').selectOption('Front end');
  await desktop.locator('#feature-status').selectOption('PARTIAL');
  const filtered = desktop.locator('.feature-card:not([hidden])');
  const filteredCount = await filtered.count();
  assert.ok(filteredCount > 0);
  assert.match(await desktop.locator('#feature-count').textContent(), new RegExp(`Showing ${filteredCount} of ${featureCount} features`));
  for (const card of await filtered.all()) {
    assert.equal(await card.getAttribute('data-category'), 'Front end');
    assert.equal(await card.getAttribute('data-status'), 'PARTIAL');
  }
  await desktop.locator('#feature-category').selectOption('');
  await desktop.locator('#feature-status').selectOption('');

  const crashDetails = desktop.locator('#feature-crash-recovery details');
  await crashDetails.locator('summary').click();
  assert.equal(await crashDetails.evaluate(node => node.open), true);
  assert.ok((await desktop.locator('#feature-crash-recovery .feature-scope').textContent()).trim());
  assert.ok((await desktop.locator('#feature-crash-recovery .feature-source').textContent()).includes('research/WRECK.md'));
  assert.ok((await desktop.locator('#feature-crash-recovery .feature-next').textContent()).trim().length > 10);
  const trainDetails = desktop.locator('#feature-arlington-train details');
  await trainDetails.locator('summary').click();
  assert.equal(await trainDetails.evaluate(node => node.open), true);
  assert.equal((await desktop.locator('#feature-arlington-train .feature-scope').textContent()).trim(), trainFeature.scope);
  assert.equal((await desktop.locator('#feature-arlington-train .feature-status').textContent()).trim(), trainFeature.status);

  const mobile = await browser.newPage({
    viewport: { width: 390, height: 844 }, deviceScaleFactor: 1, isMobile: true, hasTouch: true
  });
  observeErrors(mobile);
  await mobile.goto(new URL('/devlog.html', base).href, { waitUntil: 'networkidle' });
  await mobile.waitForFunction(() => document.querySelector('#feature-count')?.textContent.includes('features'));
  const dimensions = await mobile.evaluate(() => ({
    width: document.documentElement.clientWidth,
    scrollWidth: document.documentElement.scrollWidth
  }));
  assert.ok(dimensions.scrollWidth <= dimensions.width, `mobile horizontal overflow: ${JSON.stringify(dimensions)}`);
  await mobile.locator('#feature-category').selectOption('Vehicle');
  await mobile.locator('#feature-status').selectOption('PARTIAL');
  const mobileFiltered = mobile.locator('.feature-card:not([hidden])');
  const mobileFilteredCount = await mobileFiltered.count();
  assert.ok(mobileFilteredCount > 0);
  assert.match(await mobile.locator('#feature-count').textContent(), new RegExp(`Showing ${mobileFilteredCount} of ${featureCount} features`));
  for (const card of await mobileFiltered.all()) {
    assert.equal(await card.getAttribute('data-category'), 'Vehicle');
    assert.equal(await card.getAttribute('data-status'), 'PARTIAL');
  }

  assert.deepEqual(errors, [], `browser errors: ${errors.join('; ')}`);
  console.log(JSON.stringify({ featureAreas: featureCount, trainStatus: trainFeature.status, trainScope: trainFeature.scope, desktopFilters: 'passed', expandableEvidence: 'passed', mobileFilters: 'passed', mobile: dimensions, errors }));
} finally {
  await browser.close();
}
