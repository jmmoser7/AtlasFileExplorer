import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import { artifactGenerationGuide, artifactGenerationPrompt, displayPrompt } from './artifacts.mjs';

function request() {
  const root = path.join(os.tmpdir(), 'artifact-profile-test');
  const output = path.join(root, '.drafts', 'request-1', 'output');
  return { id: 'request-1', prompt: 'Make this pie interactive.', output_dir: output,
    artifact: { family_id: 'family-1', run_id: 'request-1', source_root: root, staging_output: output } };
}

test('artifact mode uses the canonical stage-only offline fidelity profile', () => {
  const req = request();
  const text = artifactGenerationPrompt(req, path.join(os.tmpdir(), 'link'));
  assert.ok(text.includes('Artifact generation profile v1'));
  assert.ok(text.includes('Never edit an upstream revision'));
  assert.ok(text.includes('self-contained offline HTML'));
  assert.ok(text.includes('Identify estimated'));
  assert.ok(!text.includes('Edit existing files where they are'));
  assert.ok(!text.includes('window.slateLink.inputs.data'));
  assert.ok(text.includes('"id":"request-1"'));
  assert.ok(text.includes(JSON.stringify(path.join(req.output_dir, 'index.html'))));
  assert.equal(displayPrompt(text), req.prompt);
});

test('only explicit wired input and checkpoint history enter an artifact request', () => {
  const req = request();
  req.inputs = { context: [{ text: 'ambient-not-allowed' }], wired: [{ text: 'explicit-chart-labels' }] };
  req.history = [{ role: 'assistant', text: 'chosen-prefix-only' }];
  const text = artifactGenerationPrompt(req);
  assert.ok(text.includes('explicit-chart-labels'));
  assert.ok(text.includes('chosen-prefix-only'));
  assert.ok(text.includes('replayed into a fresh session'));
  assert.ok(!text.includes('ambient-not-allowed'));
  assert.ok(!displayPrompt(text).includes('Artifact generation profile'));
});

test('ancestor output, another request, relative paths, and mixed modes refuse', () => {
  for (const edit of [
    req => { req.output_dir = req.artifact.source_root; },
    req => { req.artifact.run_id = 'another'; },
    req => { req.artifact.staging_output = req.output_dir = 'relative/output'; },
    req => { req.image = {}; },
    req => { req.oneshot = true; },
  ]) {
    const req = request(); edit(req);
    assert.throws(() => artifactGenerationGuide(req));
  }
});

test('literal template braces in a local path are quoted without substitution', () => {
  const req = request();
  req.output_dir = req.artifact.staging_output = path.join(req.output_dir, '{{request_id}}');
  assert.ok(artifactGenerationGuide(req).includes(JSON.stringify(req.output_dir)));
});

test('the shipped runtime refreshes resources beside an existing SDK cache', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'artifact-sidecar-package-'));
  try {
    const packaging = await fs.readFile(new URL('../../../scripts/package-windows.ps1', import.meta.url), 'utf8');
    const list = packaging.match(/foreach \(\$file in @\(([^)]*)\)\)/)?.[1];
    assert.ok(list, 'the Windows package owns the installed sidecar resource list');
    const files = [...list.matchAll(/'([^']+)'/g)].map(match => match[1]);
    for (const required of ['index.mjs', 'artifacts.mjs', 'artifact-generation-v1.md', 'board-link.mjs', 'board-link-v1.md', 'policy.mjs', 'attachments.mjs', 'supervise.mjs', 'models.mjs', 'activity.mjs']) {
      assert.ok(files.includes(required), `package must carry ${required}`);
    }
    const sdk = path.join(dir, 'node_modules', '@cursor', 'sdk');
    await fs.mkdir(sdk, { recursive: true });
    await fs.writeFile(path.join(sdk, 'package.json'), JSON.stringify({ type: 'module', exports: './index.mjs' }));
    await fs.writeFile(path.join(sdk, 'index.mjs'),
      'export const Agent={};export class CursorAgentError extends Error{};export const Cursor={models:{list:async()=>[{id:"installed-smoke-model",displayName:"Installed smoke"}]}};');
    await fs.writeFile(path.join(dir, 'artifact-generation-v1.md'), 'stale cached profile');
    await fs.writeFile(path.join(dir, 'policy.mjs'), 'throw new Error("stale cached policy");');
    for (const file of files) {
      await fs.copyFile(new URL('./' + file, import.meta.url), path.join(dir, file));
    }
    const packaged = await import(pathToFileURL(path.join(dir, 'artifacts.mjs')).href);
    assert.equal(packaged.artifactGenerationGuide(request()), artifactGenerationGuide(request()));
    const child = spawnSync(process.execPath, [path.join(dir, 'index.mjs'), '--models'], { encoding: 'utf8', timeout: 10000 });
    assert.equal(child.status, 0, child.stderr || child.error?.message);
    assert.equal(JSON.parse(child.stdout)[0].id, 'installed-smoke-model');
  } finally {
    await fs.rm(dir, { recursive: true, force: true });
  }
});
