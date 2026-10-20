/* Read-only source provenance check. Never rewrites the captured baseline. */
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const {execFileSync} = require('node:child_process');
const {createHash} = require('node:crypto');
const baseline = require('./source-baseline.json');
const root = path.resolve(__dirname, '../..');
const hash = bytes => createHash('sha256').update(bytes.toString('utf8').replace(/\r\n/g, '\n')).digest('hex');
let mismatches = 0;
for (const source of baseline.sources) {
  const captured = execFileSync('git', ['show', `${baseline.revision}:${source.path}`], {cwd: root});
  const current = fs.readFileSync(path.join(root, source.path));
  if (hash(captured) !== source.baselineSha256 || hash(current) !== source.baselineSha256) {
    console.error(`Source drift: ${source.path}`); mismatches++;
  }
}
if (mismatches) { console.error(`${mismatches} source owner(s) differ; review before claiming the latest native configuration.`); process.exitCode = 1; }
else console.log(`${baseline.sources.length} native owners match ${baseline.revision.slice(0, 8)} / ${baseline.build}.`);
