// Installs package-lock.json into node_modules without npm. Cursor's bundled
// Node ships without npm, and Slate may not install software system-wide.
//
//   node install.mjs [folder]
//
// Every tarball is checked against its lockfile integrity before anything is
// unpacked, and the new node_modules replaces the old one only when complete.
// Set npm_config_registry to use a mirror of registry.npmjs.org.
import crypto from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath, pathToFileURL } from 'node:url';

const NPM_REGISTRY = 'https://registry.npmjs.org/';

export async function install({
  dir,
  registry = NPM_REGISTRY,
  platform = process.platform,
  arch = process.arch,
  fetch: get = globalThis.fetch,
  log = console.log,
}) {
  const lock = JSON.parse(await fs.readFile(path.join(dir, 'package-lock.json'), 'utf8'));
  const wanted = Object.entries(lock.packages ?? {})
    .filter(([key, meta]) => key.startsWith('node_modules/') && !meta.dev && !meta.link)
    .filter(([, meta]) => !meta.optional || (fits(meta.os, platform) && fits(meta.cpu, arch)))
    .map(([key, meta]) => ({ key, name: key.slice(key.lastIndexOf('node_modules/') + 13), meta }));
  const base = registry.endsWith('/') ? registry : registry + '/';

  const downloads = await Promise.allSettled(
    wanted.map(async (pkg) => {
      const url = pkg.meta.resolved.startsWith(NPM_REGISTRY)
        ? base + pkg.meta.resolved.slice(NPM_REGISTRY.length)
        : pkg.meta.resolved;
      const bytes = await download(url, get);
      verify(bytes, pkg.meta.integrity, `${pkg.name}@${pkg.meta.version}`);
      log(`fetched ${pkg.name}@${pkg.meta.version}`);
      return { ...pkg, bytes };
    }),
  );
  const failed = downloads.find((d) => d.status === 'rejected');
  if (failed) throw failed.reason;

  const target = path.join(dir, 'node_modules');
  const staging = path.join(dir, `node_modules.partial-${process.pid}`);
  await fs.rm(staging, { recursive: true, force: true });
  try {
    for (const { value: pkg } of downloads) {
      await unpack(zlib.gunzipSync(pkg.bytes), path.join(staging, ...segments(pkg.key.slice(13))), pkg.name);
    }
  } catch (e) {
    await fs.rm(staging, { recursive: true, force: true });
    throw e;
  }
  await swap(staging, target);
  log(`installed ${wanted.length} packages into ${target}`);
}

function fits(list, value) {
  if (!list?.length) return true;
  if (list.includes(`!${value}`)) return false;
  return list.includes(value) || list.every((v) => v.startsWith('!'));
}

async function download(url, get) {
  let res;
  try {
    res = await get(url);
  } catch (e) {
    throw new Error(`could not download ${url}: ${e.cause?.code ?? e.cause?.message ?? e.message}`);
  }
  if (!res.ok) throw new Error(`could not download ${url}: HTTP ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

function verify(bytes, integrity, label) {
  const hashes = String(integrity ?? '')
    .split(/\s+/)
    .map((token) => token.match(/^(sha512|sha384|sha256|sha1)-(.+)$/))
    .filter(Boolean);
  const ok = hashes.some(([, alg, want]) => crypto.createHash(alg).update(bytes).digest('base64') === want);
  if (!ok) throw new Error(`${label}: integrity check failed`);
}

function segments(key) {
  const parts = key.split('/').filter((p) => p !== 'node_modules');
  if (parts.some((p) => !p || p === '.' || p === '..' || p.includes(':') || p.includes('\\'))) {
    throw new Error(`unsafe package name: ${key}`);
  }
  return parts;
}

function text(buf) {
  const end = buf.indexOf(0);
  return buf.subarray(0, end < 0 ? buf.length : end).toString('utf8');
}

function octal(buf, label) {
  if (buf[0] & 0x80) throw new Error(`${label}: unsupported archive size field`);
  const value = parseInt(text(buf).trim() || '0', 8);
  if (Number.isNaN(value)) throw new Error(`${label}: corrupt archive`);
  return value;
}

function paxPath(body) {
  let at = 0;
  let found = null;
  while (at < body.length) {
    const space = body.indexOf(0x20, at);
    if (space < 0) break;
    const len = parseInt(body.subarray(at, space).toString(), 10);
    if (!len) break;
    const record = body.subarray(space + 1, at + len - 1).toString('utf8');
    if (record.startsWith('path=')) found = record.slice(5);
    at += len;
  }
  return found;
}

// A package archive's entries, relative to its top folder ("package/").
function* entries(tar, label) {
  let offset = 0;
  let longName = null;
  let pax = null;
  while (offset + 512 <= tar.length) {
    const head = tar.subarray(offset, offset + 512);
    if (head.every((b) => b === 0)) break;
    const size = octal(head.subarray(124, 136), label);
    const type = head[156] ? String.fromCharCode(head[156]) : '0';
    const start = offset + 512;
    if (start + size > tar.length) throw new Error(`${label}: truncated archive`);
    const body = tar.subarray(start, start + size);
    offset = start + Math.ceil(size / 512) * 512;
    if (type === 'x') {
      pax = paxPath(body) ?? pax;
      continue;
    }
    if (type === 'g') continue;
    if (type === 'L') {
      longName = text(body);
      continue;
    }
    let name = text(head.subarray(0, 100));
    const prefix = head.subarray(257, 262).toString() === 'ustar' ? text(head.subarray(345, 500)) : '';
    if (prefix) name = `${prefix}/${name}`;
    name = pax ?? longName ?? name;
    pax = longName = null;
    yield { name, type, mode: octal(head.subarray(100, 108), label), body };
  }
}

function inside(name) {
  const parts = name.replace(/\\/g, '/').split('/');
  if (name.startsWith('/') || parts.some((p) => p === '..' || p.includes(':'))) return null;
  return parts.slice(1).filter((p) => p && p !== '.');
}

async function unpack(tar, into, label) {
  await fs.mkdir(into, { recursive: true });
  for (const entry of entries(tar, label)) {
    const parts = inside(entry.name);
    if (!parts) throw new Error(`${label}: unsafe path in archive: ${entry.name}`);
    if (!parts.length) continue;
    const dest = path.join(into, ...parts);
    if (entry.type === '5') {
      await fs.mkdir(dest, { recursive: true });
    } else if (entry.type === '0' || entry.type === '7') {
      await fs.mkdir(path.dirname(dest), { recursive: true });
      await fs.writeFile(dest, entry.body, { mode: (entry.mode & 0o777) | 0o600 });
    }
  }
}

async function swap(staging, target) {
  const old = `${target}.old-${process.pid}`;
  let moved = false;
  try {
    await fs.rename(target, old);
    moved = true;
  } catch (e) {
    if (e.code !== 'ENOENT') {
      await fs.rm(staging, { recursive: true, force: true });
      throw new Error(`could not replace ${target} (close anything using it, then retry): ${e.message}`);
    }
  }
  try {
    await fs.rename(staging, target);
  } catch (e) {
    if (moved) await fs.rename(old, target).catch(() => {});
    await fs.rm(staging, { recursive: true, force: true });
    throw new Error(`could not create ${target}: ${e.message}`);
  }
  if (moved) await fs.rm(old, { recursive: true, force: true }).catch(() => {});
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const dir = path.resolve(process.argv[2] ?? path.dirname(fileURLToPath(import.meta.url)));
  install({ dir, registry: process.env.npm_config_registry || NPM_REGISTRY }).catch((e) => {
    console.error(`Could not install the Cursor sidecar packages: ${e.message}`);
    process.exit(1);
  });
}
