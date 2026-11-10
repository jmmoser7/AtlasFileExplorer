const test = require('node:test');
const assert = require('node:assert/strict');
const {waitForLinuxCi} = require('../wait-linux-ci.cjs');
const sha = 'a'.repeat(40);
const run = extra => ({id: 4, head_sha: sha, head_branch: 'main', event: 'push', status: 'completed', conclusion: 'success', html_url: 'https://github.com/example/run/4', ...extra});
function fixture(pages) {
  let time = 0, calls = 0;
  const options = {
    context: {sha, repo: {owner: 'example', repo: 'app'}}, core: {info() {}},
    now: () => time, sleep: async ms => {time += ms;}, timeoutMs: 60, pollMs: 20,
    github: {rest: {actions: {listWorkflowRuns: async query => {
      assert.deepEqual(query, {owner: 'example', repo: 'app', workflow_id: 'ci.yml', branch: 'main', event: 'push', head_sha: sha, per_page: 100});
      return {data: {workflow_runs: pages[Math.min(calls++, pages.length - 1)]}};
    }}}},
  };
  return {options, calls: () => calls};
}
test('accepts successful exact candidate main push', async () => {
  assert.equal(await waitForLinuxCi(fixture([[run()]]).options), 4);
});
test('waits through missing, queued and running to exact success', async () => {
  const f = fixture([[], [run({status: 'queued', conclusion: null})], [run({status: 'in_progress', conclusion: null})], [run()]]);
  assert.equal(await waitForLinuxCi(f.options), 4);
  assert.equal(f.calls(), 4);
});
for (const conclusion of ['failure', 'cancelled', 'timed_out', 'skipped', 'neutral', 'action_required', null]) {
  test(`blocks completed ${conclusion} without retries`, async () => {
    const f = fixture([[run({conclusion})]]);
    await assert.rejects(waitForLinuxCi(f.options), /publication blocked/);
    assert.equal(f.calls(), 1);
  });
}
for (const wrong of [{head_sha: 'b'.repeat(40)}, {head_branch: 'feature'}, {event: 'pull_request'}]) {
  test(`does not accept foreign run ${JSON.stringify(wrong)}`, async () => {
    await assert.rejects(waitForLinuxCi(fixture([[run(wrong)]]).options), /Timed out/);
  });
}
test('latest exact run takes precedence over earlier success', async () => {
  await assert.rejects(waitForLinuxCi(fixture([[run(), run({id: 5, conclusion: 'failure'})]]).options), /run 5 ended failure/);
});
test('missing or permanently pending run times out', async () => {
  for (const page of [[], [run({status: 'in_progress', conclusion: null})]]) {
    await assert.rejects(waitForLinuxCi(fixture([page]).options), /Timed out/);
  }
});
test('API failure and malformed SHA block publication', async () => {
  const f = fixture([[run()]]);
  f.options.github.rest.actions.listWorkflowRuns = async () => {throw new Error('API unavailable');};
  await assert.rejects(waitForLinuxCi(f.options), /API unavailable/);
  f.options.context.sha = 'main';
  await assert.rejects(waitForLinuxCi(f.options), /exact candidate/);
});
