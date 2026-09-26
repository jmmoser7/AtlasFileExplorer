import assert from 'node:assert/strict';
import test from 'node:test';
import crypto from 'node:crypto';
import fs from 'node:fs/promises';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import { install } from './install.mjs';

function header(name, size, type = '0', prefix = '') {
  const block = Buffer.alloc(512);
  block.write(name, 0, 100, 'utf8');
  block.write('0000644\0', 100);
  block.write('0000000\0', 108);
  block.write('0000000\0', 116);
  block.write(size.toString(8).padStart(11, '0') + '\0', 124);
  block.write('00000000000\0', 136);
  block.write('        ', 148);
  block.write(type, 156);
  block.write('ustar\0', 257);
  block.write('00', 263);
  block.write(prefix, 345, 155, 'utf8');
  let sum = 0;
  for (const byte of block) sum += byte;
  block.write(sum.toString(8).padStart(6, '0') + '\0 ', 148);
  return block;
}

function entry(name, body, type = '0', prefix = '') {
  const data = Buffer.from(body);
  const pad = Buffer.alloc((512 - (data.length % 512)) % 512);
  return [header(name, data.length, type, prefix), data, pad];
}

function pax(pathName) {
  const record = (len) => `${len} path=${pathName}\n`;
  let len = record(0).length;
  while (record(len).length !== len) len = record(len).length;
  return entry('PaxHeader', record(len), 'x');
}

function tgz(parts) {
  return zlib.gzipSync(Buffer.concat([...parts.flat(), Buffer.alloc(1024)]));
}

function integrity(bytes) {
  return 'sha512-' + crypto.createHash('sha512').update(bytes).digest('base64');
}

const LONG = 'package/dist/' + 'nested-folder/'.repeat(8) + 'deep.js';

function packages() {
  return {
    'a-lib': tgz([
      entry('package/package.json', '{"name":"a-lib","version":"1.0.0"}'),
      entry('index.js', 'module.exports = 1;', '0', 'package/lib'),
      pax(LONG),
      entry('package/dist/short-name.js', 'deep'),
    ]),
    '@scope/b-lib': tgz([
      entry('package/package.json', '{"name":"@scope/b-lib","version":"2.0.0"}'),
      entry('package/bin/', '', '5'),
    ]),
    'foreign-bin': tgz([entry('package/package.json', '{"name":"foreign-bin"}')]),
  };
}

async function serve(files) {
  const hits = [];
  const server = http.createServer((req, res) => {
    hits.push(req.url);
    const body = files[req.url];
    if (!body) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { 'content-type': 'application/octet-stream' }).end(body);
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const base = `http://127.0.0.1:${server.address().port}/`;
  return { base, hits, close: () => new Promise((resolve) => server.close(resolve)) };
}

async function project(tarballs, edit = (lock) => lock) {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'slate-sidecar-install-'));
  const files = {};
  const lock = { name: 'fixture', lockfileVersion: 3, requires: true, packages: { '': { name: 'fixture' } } };
  for (const [name, bytes] of Object.entries(tarballs)) {
    const file = `${name}/-/${name.split('/').pop()}-1.0.0.tgz`;
    files['/' + file] = bytes;
    lock.packages[`node_modules/${name}`] = {
      version: '1.0.0',
      resolved: `https://registry.npmjs.org/${file}`,
      integrity: integrity(bytes),
    };
  }
  await fs.writeFile(path.join(dir, 'package-lock.json'), JSON.stringify(edit(lock), null, 2));
  return { dir, files };
}

const quiet = () => {};

test('installs every locked package without npm', async () => {
  const { dir, files } = await project(packages(), (lock) => {
    Object.assign(lock.packages['node_modules/foreign-bin'], { optional: true, os: ['plan9'], cpu: ['mips'] });
    return lock;
  });
  const server = await serve(files);
  try {
    await install({ dir, registry: server.base, log: quiet });
    const read = (p) => fs.readFile(path.join(dir, 'node_modules', p), 'utf8');
    assert.equal(await read('a-lib/lib/index.js'), 'module.exports = 1;', 'a ustar prefix joins the name');
    assert.equal(await read(LONG.replace('package/', 'a-lib/')), 'deep', 'a pax path replaces the short name');
    assert.equal(JSON.parse(await read('@scope/b-lib/package.json')).version, '2.0.0');
    assert.ok((await fs.stat(path.join(dir, 'node_modules/@scope/b-lib/bin'))).isDirectory());
    await assert.rejects(fs.stat(path.join(dir, 'node_modules/foreign-bin')), 'another platform is skipped');
    assert.ok(!server.hits.some((url) => url.includes('foreign-bin')), 'and never downloaded');
    const left = (await fs.readdir(dir)).filter((name) => name.startsWith('node_modules.'));
    assert.deepEqual(left, [], 'no staging folder is left behind');
  } finally {
    await server.close();
    await fs.rm(dir, { recursive: true, force: true });
  }
});

test('a second install replaces the previous packages', async () => {
  const { dir, files } = await project(packages());
  const server = await serve(files);
  try {
    await fs.mkdir(path.join(dir, 'node_modules/stale'), { recursive: true });
    await install({ dir, registry: server.base, log: quiet });
    await install({ dir, registry: server.base, log: quiet });
    await assert.rejects(fs.stat(path.join(dir, 'node_modules/stale')));
    assert.ok((await fs.stat(path.join(dir, 'node_modules/a-lib/package.json'))).isFile());
    assert.deepEqual((await fs.readdir(dir)).filter((name) => name.startsWith('node_modules.')), []);
  } finally {
    await server.close();
    await fs.rm(dir, { recursive: true, force: true });
  }
});

test('a tampered download installs nothing', async () => {
  const { dir, files } = await project(packages(), (lock) => {
    lock.packages['node_modules/a-lib'].integrity = integrity(Buffer.from('something else'));
    return lock;
  });
  const server = await serve(files);
  try {
    await assert.rejects(install({ dir, registry: server.base, log: quiet }), /a-lib.*integrity/);
    assert.deepEqual((await fs.readdir(dir)).filter((name) => name.startsWith('node_modules')), []);
  } finally {
    await server.close();
    await fs.rm(dir, { recursive: true, force: true });
  }
});

test('an archive may not write outside its package', async () => {
  for (const name of ['package/../../escape.js', '/etc/escape.js', 'package/C:/escape.js']) {
    const { dir, files } = await project({ evil: tgz([entry(name, 'x')]) });
    const server = await serve(files);
    try {
      await assert.rejects(install({ dir, registry: server.base, log: quiet }), /evil.*unsafe path/, name);
      await assert.rejects(fs.stat(path.join(dir, 'escape.js')));
      assert.deepEqual((await fs.readdir(dir)).filter((n) => n.startsWith('node_modules')), []);
    } finally {
      await server.close();
      await fs.rm(dir, { recursive: true, force: true });
    }
  }
});

test('an unreachable registry names the download that failed', async () => {
  const { dir } = await project(packages());
  try {
    await assert.rejects(
      install({ dir, registry: 'http://127.0.0.1:1/', log: quiet }),
      /could not download .*127\.0\.0\.1:1/,
    );
    assert.deepEqual((await fs.readdir(dir)).filter((name) => name.startsWith('node_modules')), []);
  } finally {
    await fs.rm(dir, { recursive: true, force: true });
  }
});
