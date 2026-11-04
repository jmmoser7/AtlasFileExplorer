const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const context = { window: {} };
for (const name of ['studio-model.js', 'studio-data.js', 'js-baselines.js']) {
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, name), 'utf8'), context);
}
const images = context.window.StudioPortalData.images;
const captures = Object.values(images).flat().filter(image => image.captureKind === 'browser-screenshot');
assert.ok(captures.length >= 11);
assert.equal(images.studio.filter(image => image.approval !== 'baseline-capture').length, 5,
  'Workflow concepts and draft must survive the catalog append');
const elements = context.StudioPortalModel.elements(null);
const ids = new Set();
for (const element of elements) {
  const entries = captures.filter(image => image.elementId === element.id);
  assert.ok(entries.length >= (element.id === 'image-generator' ? 5 : 1), element.id);
}
for (const capture of captures) {
  assert.ok(!ids.has(capture.id)); ids.add(capture.id);
  assert.equal(capture.approval, 'baseline-capture');
  assert.equal(capture.sourcePhase, '01');
  assert.equal(capture.viewport.width, 960);
  assert.equal(capture.viewport.height, 900);
  assert.equal(capture.devicePixelRatio, 1);
  assert.equal(capture.dpi, 96);
  assert.equal(capture.browserZoom, 1);
  assert.equal(capture.browser.name, 'Microsoft Edge');
  assert.equal(capture.browser.headless, true);
  assert.equal(capture.jsSourceUrl, capture.sourceUrl);
  assert.equal(capture.capture.url, capture.sourceUrl);
  assert.equal(capture.download, capture.src);
  assert.ok(Number.isFinite(Date.parse(capture.capturedAt)));
  const url = new URL(capture.sourceUrl);
  assert.ok(['127.0.0.1', 'localhost'].includes(url.hostname));
  assert.equal(url.searchParams.get('sample'), '1');
  assert.equal(url.searchParams.get('phase'), capture.elementId === 'studio' ? null : '01');
  assert.ok(capture.relevantSourceHashes['index.html']);
  assert.ok(capture.relevantSourceHashes['studio.js']);
  assert.ok(Object.values(capture.relevantSourceHashes).every(value => /^[a-f0-9]{64}$/.test(value)));
  assert.equal(capture.observed.openMenus, 0);
  assert.ok(capture.observed.canvas.top >= capture.observed.header.bottom - .5);
  assert.ok(capture.observed.tabLabels.every(label => label.scrollWidth <= label.width + 1));
  const filename = path.resolve(__dirname, capture.src);
  assert.ok(filename.startsWith(path.join(__dirname, 'assets', 'js-baselines') + path.sep));
  const bytes = fs.readFileSync(filename);
  assert.equal(bytes.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  assert.equal(bytes.readUInt32BE(16), 960); assert.equal(bytes.readUInt32BE(20), 900);
  assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), capture.screenshotSHA256);
}
console.log(`PASS: ${captures.length} actual JS captures, source metadata, PNG hashes and preserved workflow gallery.`);
