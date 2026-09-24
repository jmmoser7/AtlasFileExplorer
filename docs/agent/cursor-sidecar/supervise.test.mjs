import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { claimLink, processAlive, releaseLink, supervisionLost } from './supervise.mjs';

test('the newest watcher owns the link folder', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'slate-sidecar-'));
  const pidPath = path.join(dir, 'sidecar.pid');
  const alive = () => true;
  await claimLink(pidPath, 100);
  assert.equal(await supervisionLost(pidPath, 100, '', alive), null);
  await claimLink(pidPath, 200);
  assert.match(await supervisionLost(pidPath, 100, '', alive), /another sidecar/);
  await releaseLink(pidPath, 100);
  assert.equal(await fs.readFile(pidPath, 'utf8'), '200', 'an older watcher leaves the newer record');
  await releaseLink(pidPath, 200);
  assert.match(await supervisionLost(pidPath, 200, '', alive), /another sidecar/, 'a removed record stops the watcher');
  await fs.rm(dir, { recursive: true, force: true });
});

test('a watcher stops when Slate is gone', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'slate-sidecar-'));
  const pidPath = path.join(dir, 'sidecar.pid');
  await claimLink(pidPath, 100);
  assert.equal(await supervisionLost(pidPath, 100, '42', () => true), null);
  assert.equal(await supervisionLost(pidPath, 100, '42', () => false), 'Slate exited');
  assert.equal(await supervisionLost(pidPath, 100, 'not-a-pid', () => false), null);
  assert.equal(processAlive(process.pid), true);
  assert.equal(processAlive(2 ** 30), false);
  await fs.rm(dir, { recursive: true, force: true });
});
