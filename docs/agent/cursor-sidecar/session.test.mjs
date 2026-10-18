import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

// Run the real watcher against an isolated local SDK double, never an account
// or model service. This checks the session/request boundary, not only helpers.
async function request({ parameters = { speed: 'quick' }, streamError = false, collaborator = false } = {}) {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'cursor-sidecar-session-'));
  try {
    const index = await fs.readFile(new URL('./index.mjs', import.meta.url), 'utf8');
    await fs.writeFile(path.join(dir, 'index.mjs'), index);
    for (const match of index.matchAll(/from ['"](.\/[^'"]+\.mjs)['"]/g)) {
      await fs.copyFile(new URL(match[1], import.meta.url), path.join(dir, match[1]));
    }
    await fs.copyFile(new URL('./board-link-v1.md', import.meta.url), path.join(dir, 'board-link-v1.md'));
    const sdk = path.join(dir, 'node_modules', '@cursor', 'sdk');
    await fs.mkdir(sdk, { recursive: true });
    await fs.writeFile(path.join(sdk, 'package.json'), JSON.stringify({ type: 'module', exports: './index.mjs' }));
    await fs.writeFile(path.join(sdk, 'index.mjs'), `
      import fs from 'node:fs/promises';
      import path from 'node:path';
      const trace = { catalogCalls: 0, waited: false };
      const save = () => fs.writeFile(path.join(process.env.ATLAS_AI_WORKSPACE, 'trace.json'), JSON.stringify(trace));
      export class CursorAgentError extends Error {}
      export const Cursor = { models: { list: async () => {
        trace.catalogCalls++; await save();
        return [{ id: 'test-model', parameters: [{ id: 'speed', values: [{ value: 'quick' }] }] }];
      } } };
      const agent = { agentId: 'isolated-conversation', reload: async () => {},
        send: async (message, options) => {
          trace.model = options.model; trace.message = message; await save();
          for (const update of [
            { type: 'text-delta', text: 'Reading the reference.' },
            { type: 'thinking-delta', text: 'private reasoning' },
            { type: 'tool-call-started', toolCall: { type: 'shell', args: { command: 'private command' } } },
            { type: 'tool-call-completed', toolCall: { type: 'shell', result: { stdout: 'private result' } } },
          ]) await options.onDelta({ update });
          return { stream: async function* () {
            yield { type: 'assistant', message: { content: [{ type: 'text', text: 'Reading the reference.' }] } };
            if (${streamError}) throw new Error('stream disconnected');
          }, wait: async () => { trace.waited = true; await save(); return { status: 'completed', result: 'Done' }; } };
        }
      };
      export const Agent = { create: async () => agent, resume: async () => agent };
    `);
    await fs.writeFile(path.join(dir, 'session.json'), JSON.stringify({ activity: [
      { id: 'old:activity:1', request: 'old', kind: 'commentary', text: 'Previous job', at: 1 },
    ] }));
    await fs.writeFile(path.join(dir, 'request.json'), JSON.stringify({ id: 'new', prompt: 'Create a dashboard', model: 'test-model', model_parameters: parameters,
      ...(collaborator ? {collaborator:{session_id:'s',grant_id:'g',workspace_id:'w'},inputs:{context:[{node:1,text:'ambient-private-context',images:[]}],wired:[]}} : {}) }));
    const child = spawnSync(process.execPath, [path.join(dir, 'index.mjs')], { encoding: 'utf8', timeout: 10000,
      env: { SYSTEMROOT: process.env.SYSTEMROOT ?? '', ATLAS_AI_WORKSPACE: dir, ATLAS_AGENT_LINK_DIR: dir,
        ATLAS_AGENT_SESSION: 'isolated', ATLAS_AGENT_ONCE: '1' } });
    assert.equal(child.status, 0, child.stderr || child.error?.message);
    const session = JSON.parse(await fs.readFile(path.join(dir, 'session.json'), 'utf8'));
    const trace = JSON.parse(await fs.readFile(path.join(dir, 'trace.json'), 'utf8'));
    return { session, trace };
  } finally {
    await fs.rm(dir, { recursive: true, force: true });
  }
}

test('real watcher validates catalog parameters and publishes request-scoped public activity', async () => {
  const { session, trace } = await request();
  assert.deepEqual(trace.model, { id: 'test-model', params: [{ id: 'speed', value: 'quick' }] });
  assert.equal(trace.catalogCalls, 1);
  assert.equal(trace.waited, true);
  assert.equal(session.status, 'idle');
  assert.deepEqual(session.activity.map((item) => item.kind), ['commentary', 'tool_started', 'tool_finished']);
  assert.ok(session.activity.every((item) => item.request === 'new'));
  assert.ok(!JSON.stringify(session).includes('private'));
  assert.equal(session.turns.filter((turn) => turn.role === 'assistant').length, 1);
});

test('a stream failure still awaits run completion before surfacing the failure', async () => {
  const { session, trace } = await request({ streamError: true });
  assert.equal(trace.waited, true);
  assert.deepEqual(session.status, { error: 'stream disconnected' });
});

test('invalid fresh model parameters fail visibly before a run is submitted', async () => {
  const { session, trace } = await request({ parameters: { speed: 'invented' } });
  assert.equal(trace.catalogCalls, 1);
  assert.ok(!Object.hasOwn(trace, 'model'));
  assert.match(session.status.error, /does not support speed=invented/);
  assert.deepEqual(session.activity, []);
});

test('real watcher submits the shared scoped collaborator guide without ambient context', async () => {
  const { session, trace } = await request({collaborator:true});
  const message=JSON.stringify(trace.message);
  assert.equal(session.status,'idle');
  assert.ok(message.includes('slate_board_read'));
  assert.ok(message.includes('board-calls/'));
  assert.ok(!message.includes('ambient-private-context'));
  assert.ok(!message.includes('you cannot draw shapes'));
  assert.ok(!message.includes('place.json'));
});
