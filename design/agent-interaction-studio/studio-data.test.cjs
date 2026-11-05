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
assert.deepEqual(Object.keys(data.images), ['video', 'studio']);
assert.equal(data.images.video.length, 6);
const video = data.images.video[0];
assert.equal(video.id, 'video-native-early-20261104');
assert.equal(video.approval, 'review-capture');
assert.match(video.caption, /recovery\/stress not yet proved/);
assert.match(video.initialObservation, /Pause failed/);
assert.match(video.caption, /not conclusive/);
assert.match(video.clarification, /V3 component UI subsequently passed settled Play\/Pause/);
assert.match(video.caption, /not a stripe defect/);
assert.equal(data.nativeCanvas.video.capture, data.images.video.at(-1).src);
assert.equal(fs.readFileSync(path.join(__dirname,video.src)).subarray(0,3).toString('hex'),'ffd8ff');
assert.equal(JSON.parse(fs.readFileSync(path.join(__dirname,video.receipt),'utf8')).binarySha256,'4da9c447c42ab9b54ffce5aff57bdc07fe087a119dc38826dcf62e519d5ebf18');
assert.deepEqual(data.images.video.slice(1,3).map(image=>image.id),['video-native-v3-crash-recovered-20261104','video-native-v3-original-retried-20261104']);
for(const capture of data.images.video.slice(1,3)) {
  assert.match(capture.caption,/V3 component UI only/);
  assert.match(capture.caption,/Replay-opening and adaptive corpus tests were still blocked/);
  assert.equal(fs.readFileSync(path.join(__dirname,capture.src)).subarray(0,3).toString('hex'),'ffd8ff');
  const receipt=JSON.parse(fs.readFileSync(path.join(__dirname,capture.receipt),'utf8'));
  assert.match(receipt.oneClickPause,/Pass visualPaused0.625/);
  assert.match(receipt.differentClipAfterCrash,/Pass singlePlay/);
  assert.match(receipt.sameClipRetryAfterCrash,/Pass singlePlay/);
  assert.equal(JSON.parse(fs.readFileSync(path.join(__dirname,capture.binaryReceipt),'utf8')).sha256,'28770C81C8629F2C91599B2FBA8B0218C38EF17FC8D1161DB4BC92EC0F864CE1');
  assert.ok(fs.statSync(path.join(__dirname,capture.crashReceipt)).size>0);
}
const lifecycle=data.images.video[3];
assert.equal(lifecycle.id,'video-native-v8-clear-reload-20261104');
assert.match(lifecycle.caption,/V8 component UI lifecycle only/);
assert.match(lifecycle.caption,/Historical V9 native adaptive scrub was blocked/);
assert.match(lifecycle.caption,/V10 later passed the full component stress/);
assert.equal(fs.readFileSync(path.join(__dirname,lifecycle.src)).subarray(0,3).toString('hex'),'ffd8ff');
const lifecycleProof=JSON.parse(fs.readFileSync(path.join(__dirname,lifecycle.receipt),'utf8'));
assert.match(lifecycleProof.clear,/0positions/);
assert.match(lifecycleProof.reload,/all96positions/);
assert.match(lifecycleProof.oneClickAfterReload,/actual8frames thenEnded/);
assert.equal(JSON.parse(fs.readFileSync(path.join(__dirname,lifecycle.binaryReceipt),'utf8')).sha256,lifecycleProof.binarySha256);
assert.equal(lifecycle.failureHistory.length,3);
for(const receipt of lifecycle.failureHistory) assert.equal(JSON.parse(fs.readFileSync(path.join(__dirname,receipt),'utf8')).recovery_pass,false);
const pass=data.images.video[4];
assert.equal(pass.id,'video-native-v10-stress-passed-20261104');
assert.match(pass.caption,/V10 component UI: 92\/92/);
assert.equal(fs.readFileSync(path.join(__dirname,pass.src)).subarray(0,3).toString('hex'),'ffd8ff');
const passProof=JSON.parse(fs.readFileSync(path.join(__dirname,pass.receipt),'utf8'));
assert.equal(passProof.actions,92);assert.equal(passProof.planned_actions,92);
assert.equal(passProof.aborted,null);assert.equal(passProof.recovery_pass,true);
assert.equal(passProof.new_playback_frames,162);
assert.ok(passProof.peak_retained_rgba_bytes<=passProof.strip_budget_bytes);
assert.ok(passProof.peak_queued_frame_bytes<=8*1024*1024);
assert.ok(passProof.clips.filter(clip=>clip.required_decode_pass!==null).every(clip=>clip.required_decode_pass));
assert.equal(JSON.parse(fs.readFileSync(path.join(__dirname,pass.binaryReceipt),'utf8')).sha256,'1A08245B7DD2AD5ADDD1A288D4B0F119D4802AF219E53A4A8B3C71F2555799BE');
assert.equal(require('node:crypto').createHash('sha256').update(fs.readFileSync(path.join(__dirname,lifecycle.failureHistory[2]))).digest('hex'),'eab0803a5ddf27317e8d341415e161a086564173ed73de4d97ef06e390354261');
const launch=data.images.video[5];
assert.equal(launch.id,'video-native-v10-dashboard-launch-20261104');
assert.equal(fs.readFileSync(path.join(__dirname,launch.src)).subarray(0,3).toString('hex'),'ffd8ff');
const launchProof=JSON.parse(fs.readFileSync(path.join(__dirname,launch.receipt),'utf8'));
assert.match(launchProof.route,/via visible dashboard button/);
assert.match(launchProof.result,/primary real poster96positions/);
assert.equal(launchProof.binarySha256,'1A08245B7DD2AD5ADDD1A288D4B0F119D4802AF219E53A4A8B3C71F2555799BE');
assert.match(JSON.parse(fs.readFileSync(path.join(__dirname,launch.pointerReceipt),'utf8')).scrub,/Pass nativeaccessibilitypointerclick/);
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
console.log('PASS: historical phase arithmetic/failures, five workflow images, labeled Video captures/receipts and clarified early AX inference.');
