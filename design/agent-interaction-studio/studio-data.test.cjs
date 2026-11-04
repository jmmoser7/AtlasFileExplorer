const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const context = { window: {} };
vm.runInNewContext(fs.readFileSync(path.join(__dirname, 'studio-data.js'), 'utf8'), context);
const data = JSON.parse(JSON.stringify(context.window.StudioPortalData));
const integration = data.summaries['04'].studio;
const release = data.summaries['05'].studio;
const milliseconds = seconds => Math.round(seconds * 1000);
assert.equal(integration.seconds, 7886.859);
assert.equal(release.seconds, 8987.842);
assert.equal(data.audit.seconds, 16874.701);
assert.equal(integration.end, release.start);
assert.equal(milliseconds(integration.seconds + release.seconds), milliseconds(data.audit.seconds));

for (const summary of [integration, release, data.audit]) {
  assert.equal(summary.unknownDurations.writing, null);
  assert.equal(summary.unknownDurations.debugging, null);
}
assert.deepEqual(Object.keys(data.images), ['studio']);
assert.deepEqual(Object.keys(data.summaries['04']), ['studio']);
assert.deepEqual(Object.keys(data.summaries['05']), ['studio']);
assert.equal(data.audit.retrospective, true);
assert.equal(data.audit.perToolDurationsAvailable, false);
assert.equal(data.audit.sourceCommit, '9ec989b12d9dd42a15447d23e2acfccf8b92992d');
assert.equal(data.audit.packageSHA256, 'ba8f14befc04460832ab4326e3c0bd6d921449aedf65d04e5ede4aa946d83127');

const allRecords = [...integration.records, ...release.records];
const records = new Map(allRecords.map(record => [record.id, record]));
assert.equal(records.size, allRecords.length);
for (const record of allRecords) {
  assert.ok(record.source && record.category && record.status);
  if (record.seconds == null) {
    assert.equal(record.start, null);
    assert.equal(record.end, null);
    continue;
  }
  assert.equal(record.seconds, (Date.parse(record.end) - Date.parse(record.start)) / 1000);
  assert.ok(record.seconds > 0);
  if (record.compilerSeconds != null) assert.ok(record.compilerSeconds <= record.seconds);
  if (record.parentId) {
    const parent = records.get(record.parentId);
    assert.equal(record.overlap, true);
    assert.ok(Date.parse(record.start) >= Date.parse(parent.start));
    assert.ok(Date.parse(record.end) <= Date.parse(parent.end));
  }
}
assert.equal(records.get('windows-first-125688304198').status, 'failed');
assert.equal(records.get('windows-first-workspace').result.atlas.failed, 1);
assert.equal(records.get('windows-retry-125700321628').status, 'passed');
assert.equal(records.get('main-preview-41574882622').parallel, true);
assert.equal(records.get('main-ci-41574882543').seconds, null);
assert.equal(integration.records[0].sourceCommit, '4172134d');
assert.ok(integration.records[0].seconds < integration.seconds);

assert.equal(data.images.studio.length, 5);
assert.equal(data.images.studio.filter(image => image.approval === 'approved-home-reference').length, 1);
assert.equal(data.images.studio.at(-1).approval, 'approved-home-reference');
assert.ok(data.images.studio.some(image => image.approval === 'unreviewed-draft'));
for (const image of data.images.studio) {
  assert.equal(image.download, image.src);
  assert.ok(image.promptDescription && image.promptDescriptionBasis && image.dateBasis);
  const file = path.resolve(__dirname, image.src);
  assert.ok(file.startsWith(path.join(__dirname, 'assets', 'studio-workflow') + path.sep));
  const bytes = fs.readFileSync(file);
  assert.equal(bytes.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  assert.ok(bytes.readUInt32BE(16) > 0 && bytes.readUInt32BE(20) > 0);
}
console.log('PASS: phase arithmetic, retained failures, overlap/unknown semantics, and five local gallery images.');
