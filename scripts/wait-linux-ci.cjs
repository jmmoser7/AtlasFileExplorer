// Publication gate only: the existing Linux workflow owns all Linux checks.
async function waitForLinuxCi({github, context, core, now = Date.now,
  sleep = ms => new Promise(resolve => setTimeout(resolve, ms)),
  timeoutMs = 120 * 60 * 1000, pollMs = 30000}) {
  if (!/^[a-f0-9]{40}$/.test(context.sha)) throw new Error('Expected an exact candidate commit SHA.');
  const deadline = now() + timeoutMs;
  let previous = '';
  while (true) {
    const {data} = await github.rest.actions.listWorkflowRuns({
      ...context.repo, workflow_id: 'ci.yml', branch: 'main', event: 'push',
      head_sha: context.sha, per_page: 100,
    });
    const run = data.workflow_runs.filter(run => run.head_sha === context.sha &&
      run.head_branch === 'main' && run.event === 'push')
      .sort((a, b) => b.id - a.id)[0];
    const state = run ? `${run.id}: ${run.status}/${run.conclusion || 'pending'}` : 'matching run not created';
    if (state !== previous) {
      core.info(`Linux CI for ${context.sha}: ${state}`);
      previous = state;
    }
    if (run?.status === 'completed') {
      if (run.conclusion !== 'success') throw new Error(`Linux CI run ${run.id} ended ${run.conclusion}; publication blocked.`);
      core.info(`Verified Linux CI success: ${run.html_url}`);
      return run.id;
    }
    if (now() >= deadline) throw new Error('Timed out waiting for successful Linux CI on the exact candidate SHA and main push; publication blocked.');
    await sleep(Math.min(pollMs, deadline - now()));
  }
}
module.exports = {waitForLinuxCi};
