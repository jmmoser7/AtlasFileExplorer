import assert from 'node:assert/strict';
import test from 'node:test';
import { agentPolicyOptions, READ_ONLY_TOOLS, refusal, REFUSAL, sendMode, withPolicy } from './policy.mjs';

const RO = { ATLAS_AGENT_READ_ONLY: '1' };

test('read-only tools can only look', () => {
  for (const tool of ['edit', 'shell', 'delete', 'task', 'mcp', 'applyAgentDiff', 'generateImage', 'write']) {
    assert.ok(!READ_ONLY_TOOLS.includes(tool), tool);
  }
  assert.ok(READ_ONLY_TOOLS.includes('read'));
  assert.throws(() => READ_ONLY_TOOLS.push('shell'));
});

test('agent options restrict tools and sandbox only when started read-only', () => {
  assert.deepEqual(agentPolicyOptions({}), {});
  assert.deepEqual(agentPolicyOptions({ ATLAS_AGENT_READ_ONLY: '0' }), {});
  assert.deepEqual(agentPolicyOptions(undefined), {});
  assert.deepEqual(agentPolicyOptions(RO), {
    tools: READ_ONLY_TOOLS.slice(),
    local: { sandboxOptions: { enabled: true }, autoReview: true },
  });
});

test('policy merges into local options and overrides full access', () => {
  const base = { model: { id: 'auto' }, local: { cwd: 'C:/p', settingSources: ['all'], autoReview: false } };
  assert.deepEqual(withPolicy(base, {}), base);
  const ro = withPolicy(base, RO);
  assert.deepEqual(ro.local, { cwd: 'C:/p', settingSources: ['all'], autoReview: true, sandboxOptions: { enabled: true } });
  assert.deepEqual(ro.tools, READ_ONLY_TOOLS.slice());
  assert.deepEqual(ro.model, { id: 'auto' });
  assert.equal(base.local.autoReview, false, 'the input is not mutated');
  assert.deepEqual(withPolicy({ model: 1 }, RO).local, { sandboxOptions: { enabled: true }, autoReview: true });
});

test('read-only sends run in plan mode', () => {
  assert.equal(sendMode({}, RO), 'plan');
  assert.equal(sendMode({ policy: 'read_only' }, {}), undefined);
});

test('a read-only turn on a writing sidecar is refused', () => {
  assert.equal(refusal({ policy: 'read_only' }, {}), REFUSAL);
  assert.equal(refusal({ policy: 'read_only' }, { ATLAS_AGENT_READ_ONLY: '0' }), REFUSAL);
  assert.equal(refusal({ policy: 'read_only' }, RO), null);
  assert.equal(refusal({}, {}), null);
  assert.equal(refusal({ policy: 'default' }, RO), null);
  assert.equal(REFUSAL, 'This Cursor sidecar was started with write tools; Slate restarts it read-only. Send again.');
});
