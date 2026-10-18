import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs/promises';
import { ACTIVITY_LIMIT, ACTIVITY_TEXT_LIMIT, boundedActivity, recordActivity } from './activity.mjs';

test('public text deltas coalesce separately from transcript turns', () => {
  const first = recordActivity([], { type: 'text-delta', text: 'Reading ' }, 'r1', 10);
  const next = recordActivity(first, { type: 'text-delta', text: 'the reference.' }, 'r1', 11);
  assert.deepEqual(next, [{ id: 'r1:activity:1', request: 'r1', kind: 'commentary', text: 'Reading the reference.', at: 11, semantics: { action: 'chat' } }]);
  assert.equal(first[0].text, 'Reading ');
});

test('tool activity includes types only and discards thinking and all tool payloads', () => {
  const update = { type: 'tool-call-started', callId: 'private-call', toolCall: {
    type: 'shell', args: { command: 'secret command' }, result: { stdout: 'private output' },
  } };
  const started = recordActivity([], update, 'r1', 1);
  const finished = recordActivity(started, { ...update, type: 'tool-call-completed' }, 'r1', 2);
  assert.deepEqual(finished.map((item) => [item.kind, item.text]), [
    ['tool_started', 'Started shell tool.'], ['tool_finished', 'Finished shell tool.'],
  ]);
  assert.ok(!JSON.stringify(finished).includes('private'));
  assert.ok(!JSON.stringify(finished).includes('secret'));
  for (const type of ['thinking-delta', 'thinking', 'tool-call-delta', 'status']) {
    assert.equal(recordActivity(finished, { type, text: 'private reasoning' }, 'r1', 3), finished);
  }
  assert.equal(recordActivity([], { type: 'tool-call-started', toolCall: { type: 'secret/path\ncommand' } }, 'r1', 1)[0].text, 'Started tool.');
});

test('activity is bounded, request-scoped, and text is limited without losing recent deltas', () => {
  let history = [];
  for (let i = 0; i < 80; i++) history = recordActivity(history, { type: 'tool-call-completed', toolCall: { type: 'read' } }, 'r1', i);
  assert.equal(history.length, ACTIVITY_LIMIT);
  assert.equal(history[0].id, 'r1:activity:17');
  const old = history;
  history = recordActivity(history, { type: 'text-delta', text: 'x'.repeat(ACTIVITY_TEXT_LIMIT * 2 + 2) }, 'r2', 90);
  assert.equal(history.length, 3);
  assert.equal(history.map((item) => item.text).join('').length, ACTIVITY_TEXT_LIMIT * 2 + 2);
  assert.ok(history.every((item) => item.request === 'r2' && item.text.length <= ACTIVITY_TEXT_LIMIT));
  assert.equal(old.length, ACTIVITY_LIMIT);
});

test('restored activity strips unexpected fields and invalid rows', () => {
  assert.deepEqual(boundedActivity([{ id: 'a', request: 'r', kind: 'commentary', text: 'x'.repeat(1100), at: 3, private: 'payload' },
    { id: 'bad', request: 'r', kind: 'thinking', text: 'hidden', at: 3 }]), [
    { id: 'a', request: 'r', kind: 'commentary', text: 'x'.repeat(ACTIVITY_TEXT_LIMIT), at: 3 },
  ]);
});

test('astral text uses Unicode character limits without splitting surrogate pairs', () => {
  const first = recordActivity([], { type: 'text-delta', text: 'a'.repeat(ACTIVITY_TEXT_LIMIT - 1) }, 'r1', 1);
  const next = recordActivity(first, { type: 'text-delta', text: '🧭🧭' }, 'r1', 2);
  assert.deepEqual(next.map((item) => Array.from(item.text).length), [ACTIVITY_TEXT_LIMIT, 1]);
  assert.equal(next[0].text, 'a'.repeat(ACTIVITY_TEXT_LIMIT - 1) + '🧭');
  assert.equal(next[1].text, '🧭');
  const restored = boundedActivity([{ id: 'r:activity:1', request: 'r', kind: 'commentary', text: '🧭'.repeat(ACTIVITY_TEXT_LIMIT + 1), at: 1 }]);
  assert.equal(restored[0].text, '🧭'.repeat(ACTIVITY_TEXT_LIMIT));
});

test('failed tool completion reports failure using its type only', () => {
  const history = recordActivity([], { type: 'tool-call-completed', toolCall: {
    type: 'read', args: { path: 'private path' }, result: { status: 'error', error: 'private output' },
  } }, 'r1', 1);
  assert.equal(history[0].kind, 'tool_finished');
  assert.equal(history[0].text, 'Failed read tool.');
  assert.ok(!JSON.stringify(history).includes('private'));
  assert.deepEqual(history[0].semantics, { action: 'error' });
});

test('known public tool types supply categories without inspecting arguments or output', () => {
  const categories = { read: 'reading', grep: 'searching', glob: 'searching', ls: 'discovery',
    semSearch: 'searching', readLints: 'verifying', edit: 'editing', write: 'editing',
    delete: 'editing', applyAgentDiff: 'editing', generateImage: 'creating-image',
    task: 'delegating', shell: 'working', mcp: 'working', futureTool: 'working', constructor: 'working', toString: 'working' };
  for (const [type, action] of Object.entries(categories)) {
    const toolCall = { type, get args() { throw Error('must not inspect private arguments'); },
      result: { status: 'success', get value() { throw Error('must not inspect raw result'); } } };
    for (const updateType of ['tool-call-started', 'tool-call-completed']) {
      const [row] = recordActivity([], { type: updateType, toolCall }, 'r', 1);
      assert.deepEqual(row.semantics, { action }, type);
      assert.equal(row.semantics.target, undefined);
    }
  }
  const [commentary] = recordActivity([], { type: 'text-delta', text: 'I am angry and reading.' }, 'r', 1);
  assert.deepEqual(commentary.semantics, { action: 'chat' });
});

test('restored semantics accept the native protocol catalog and remove unknown/private metadata', async () => {
  const catalog = JSON.parse(await fs.readFile(new URL('../../../crates/atlas-agent/assets/agent-activities.json', import.meta.url), 'utf8'));
  const base = { id: 'a', request: 'r', kind: 'tool_started', text: 'Working', at: 1 };
  for (const { id: action } of catalog.activities) {
    const [row] = boundedActivity([{ ...base, semantics: { action,
      target: { node_id: 'node-42', private: 'secret' }, private: 'payload' } }]);
    assert.deepEqual(row.semantics, { action, target: { node_id: 'node-42' } });
  }
  for (const semantics of [null, { action: 'future-category' }, { action: 'Thinking' }, []])
    assert.deepEqual(boundedActivity([{ ...base, semantics }]), [base]);
  for (const node_id of [' ', 'node\nprivate', '界'.repeat(257)]) {
    const [row] = boundedActivity([{ ...base, semantics: { action: 'reading', target: { node_id } } }]);
    assert.deepEqual(row.semantics, { action: 'reading' });
  }
});
