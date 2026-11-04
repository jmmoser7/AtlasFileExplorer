const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const code = fs.readFileSync(`${__dirname}/release.js`, 'utf8');
const published = version => ({
  tag_name: `v${version}`, draft: false, prerelease: false,
  published_at: '2026-11-04T00:00:00Z',
  assets: [{ name: 'AtlasSuite-stable-Setup.exe', size: 100, state: 'uploaded' }]
});

async function labelFor(release, responseOk = true) {
  const labels = [{ textContent: 'Latest stable build' }, { textContent: 'Latest stable build' }];
  await vm.runInNewContext(code, {
    document: { querySelectorAll: () => labels },
    fetch: async url => {
      assert.equal(url, 'https://api.github.com/repos/jmmoser7/AtlasFileExplorer/releases/latest');
      return { ok: responseOk, json: async () => release };
    }
  });
  return labels.map(label => label.textContent);
}

(async () => {
  for (const version of ['1.0.5', '1.0.6']) {
    assert.deepEqual(await labelFor(published(version)), [`Build ${version}`, `Build ${version}`]);
  }
  for (const release of [
    { ...published('1.0.6'), draft: true },
    { ...published('1.0.6'), prerelease: true },
    { ...published('1.0.6'), published_at: null },
    { ...published('1.0.6'), assets: [] },
    { ...published('1.0.6'), tag_name: 'preview' }
  ]) {
    assert.deepEqual(await labelFor(release), ['Latest stable build', 'Latest stable build']);
  }
  assert.deepEqual(await labelFor(null, false), ['Latest stable build', 'Latest stable build']);
  console.log('PASS: published installer versions update both labels; unavailable metadata retains the stable fallback.');
})().catch(error => { console.error(error); process.exitCode = 1; });
