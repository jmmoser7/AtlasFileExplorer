import assert from 'node:assert/strict';
import test from 'node:test';
import worker, { handleRequest } from './worker.mjs';

const key = await crypto.subtle.generateKey({
  name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048,
  publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256',
}, true, ['sign', 'verify']);
const privateKey = Buffer.from(await crypto.subtle.exportKey('pkcs8', key.privateKey)).toString('base64');
const origin = 'https://jmmoser7.github.io';
const url = 'https://intake.example/suggestions';
const now = new Date('2026-11-30T23:59:58Z');
const repoPath = '/repos/owner/private-feedback';
const owner = { id: 7, login: 'owner', type: 'User' };
const repo = { private: true, visibility: 'private', full_name: 'owner/private-feedback', id: 11, owner };
const sample = {
  id: '202611-01234567-89ab-4def-8123-456789abcdef',
  source: 'website',
  kind: 'Feature',
  title: 'Improve zoom',
  body: 'A few steps. 汉字 🚀\nSecond line.',
};
const filePath = `${repoPath}/contents/submissions/2026/11/${sample.id}.json`;
const env = {
  BROWSER_ORIGIN: origin,
  GITHUB_OWNER: 'owner',
  GITHUB_REPO: 'private-feedback',
  GITHUB_REPOSITORY_ID: '11',
  GITHUB_APP_CLIENT_ID: 'Iv1.sample',
  GITHUB_APP_INSTALLATION_ID: '101',
  GITHUB_APP_PRIVATE_KEY: `-----BEGIN PRIVATE KEY-----\n${privateKey}\n-----END PRIVATE KEY-----`,
  SUGGESTION_RATE_LIMIT: { async limit({ key }) { assert.equal(key, '192.0.2.1'); return { success: true }; } },
};

function post(data = sample, headers = {}) {
  return new Request(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'CF-Connecting-IP': '192.0.2.1', ...headers },
    body: JSON.stringify(data),
  });
}

function github({ metadata = repo, rejectedWrite = null, collaborators = [owner], accessStatus = 200, accessHeaders = {} } = {}) {
  const calls = [];
  const files = new Map();
  const fetcher = async (url, options) => {
    const parsed = new URL(url);
    const { method, headers, body, redirect, signal } = options;
    assert.equal(parsed.origin, 'https://api.github.com');
    assert.equal(redirect, 'manual');
    assert.ok(signal instanceof AbortSignal);
    const path = parsed.pathname;
    calls.push([method, path]);
    if (path === '/app/installations/101/access_tokens') {
      assert.equal(method, 'POST');
      assert.deepEqual(JSON.parse(body), { repository_ids: [11], permissions: { contents: 'write' } });
      const [header, payload, signature] = headers.Authorization.slice(7).split('.');
      assert.deepEqual(JSON.parse(Buffer.from(header, 'base64url')), { alg: 'RS256', typ: 'JWT' });
      const jwt = JSON.parse(Buffer.from(payload, 'base64url'));
      assert.equal(jwt.iss, 'Iv1.sample');
      assert.equal(jwt.exp - jwt.iat, 540);
      assert.equal(await crypto.subtle.verify('RSASSA-PKCS1-v1_5', key.publicKey,
        Buffer.from(signature, 'base64url'), Buffer.from(`${header}.${payload}`)), true);
      return Response.json({ token: 'ghs_test' }, { status: 201 });
    }
    assert.equal(headers.Authorization, 'Bearer ghs_test');
    if (path === repoPath) return Response.json(metadata);
    if (path === `${repoPath}/collaborators`) {
      assert.equal(method, 'GET');
      assert.equal(parsed.search, '?affiliation=all&per_page=2');
      return Response.json(collaborators, { status: accessStatus, headers: accessHeaders });
    }
    if (method === 'PUT') {
      if (rejectedWrite) return Response.json({ message: 'secret upstream detail' }, { status: rejectedWrite });
      if (files.has(path)) return Response.json({}, { status: 422 });
      const commit = JSON.parse(body);
      assert.deepEqual(Object.keys(commit), ['message', 'content']);
      assert.match(commit.message, /^Add suggestion 20/);
      files.set(path, commit.content);
      return Response.json({}, { status: 201 });
    }
    if (files.has(path)) return Response.json({ type: 'file', encoding: 'base64', content: files.get(path) });
    return Response.json({}, { status: 404 });
  };
  return { calls, files, fetcher };
}

const unexpectedFetch = () => assert.fail('unexpected upstream call');

test('permits production browser preflight and rejects other origins and public reads', async () => {
  const headers = { Origin: origin, 'Access-Control-Request-Method': 'POST', 'Access-Control-Request-Headers': 'content-type' };
  const preflight = await worker.fetch(new Request(url, { method: 'OPTIONS', headers }), env);
  assert.equal(preflight.status, 204);
  assert.equal(preflight.headers.get('Access-Control-Allow-Origin'), origin);
  assert.equal(preflight.headers.get('Access-Control-Allow-Headers'), 'Content-Type');
  assert.equal(preflight.headers.get('Access-Control-Allow-Credentials'), null);
  const other = await handleRequest(post(sample, { Origin: 'https://other.example' }), env, { fetcher: unexpectedFetch, now });
  assert.equal(other.status, 403);
  assert.equal(other.headers.get('Access-Control-Allow-Origin'), null);
  assert.equal((await handleRequest(new Request(url), env, { fetcher: unexpectedFetch, now })).status, 405);
  assert.equal((await handleRequest(new Request(url + '/records'), env, { fetcher: unexpectedFetch, now })).status, 404);
});

test('rejects unexpected fields, malformed input, traversal, future sharding, and oversized submissions', async () => {
  for (const value of [
    null, [], { ...sample, key: 'x' }, { ...sample, source: 'admin' }, { ...sample, kind: 'Idea' },
    { ...sample, id: '../private-folder' }, { ...sample, id: sample.id.replace('202611', '202701') },
    { ...sample, title: '' }, { ...sample, title: 'a'.repeat(161) },
    { ...sample, body: 'b'.repeat(6001) }, { ...sample, body: 'b\u0000b' }, { ...sample, version: 1 },
  ]) {
    const response = await handleRequest(post(value), env, { fetcher: unexpectedFetch, now });
    assert.equal(response.status, 400);
    assert.deepEqual(await response.json(), { error: 'invalid_submission' });
  }
  for (const [body, headers] of [
    ['not json', { 'Content-Type': 'application/json' }],
    ['x'.repeat(32769), { 'Content-Type': 'application/json' }],
    ['{}', { 'Content-Type': 'text/plain' }],
    ['{}', { 'Content-Type': 'application/json', 'Content-Length': '32769' }],
  ]) {
    const request = new Request(url, { method: 'POST', body, headers: { ...headers, 'CF-Connecting-IP': '192.0.2.1' } });
    assert.equal((await handleRequest(request, env, { fetcher: unexpectedFetch, now })).status, 400);
  }
});

test('fails closed if origin-independent ingress limits are absent or exhausted', async () => {
  const blocked = { ...env, SUGGESTION_RATE_LIMIT: { async limit() { return { success: false }; } } };
  const response = await handleRequest(post(), blocked, { fetcher: unexpectedFetch, now });
  assert.equal(response.status, 429);
  assert.deepEqual(await response.json(), { error: 'rate_limited' });
  const missingIp = new Request(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(sample) });
  assert.equal((await handleRequest(missingIp, env, { fetcher: unexpectedFetch, now })).status, 503);
});

test('requires the same private, personally owned repository before any content call', async () => {
  for (const metadata of [
    { ...repo, private: false, visibility: 'public' }, { ...repo, visibility: 'internal' },
    { ...repo, full_name: 'someone-else/private-feedback' }, { ...repo, id: 99 },
    { ...repo, owner: { ...owner, type: 'Organization' } }, { ...repo, owner: { ...owner, login: 'someone-else' } },
  ]) {
    const upstream = github({ metadata });
    const result = await handleRequest(post(), env, { fetcher: upstream.fetcher, now });
    assert.equal(result.status, 503);
    assert.deepEqual(upstream.calls.map(call => call[0]), ['POST', 'GET']);
    assert.deepEqual(await result.json(), { error: 'submission_unavailable' });
  }
});

test('requires a complete access list with no human collaborators besides the owner', async () => {
  const opts = [
    { collaborators: [owner, { id: 3, login: 'collaborator' }] },
    { collaborators: [{ ...owner, id: 99 }] },
    { collaborators: [null] },
    { collaborators: { entries: [owner] } },
    { accessHeaders: { Link: '<https://api.github.com/example?page=2>; rel="next"' } },
    { accessStatus: 403 },
  ];
  for (const option of opts) {
    const upstream = github(option);
    const response = await handleRequest(post(), env, { fetcher: upstream.fetcher, now });
    assert.equal(response.status, 503);
    assert.deepEqual(upstream.calls.map(call => call[0]), ['POST', 'GET', 'GET']);
    assert.deepEqual(await response.json(), { error: 'submission_unavailable' });
  }
  const upstream = github({ collaborators: [] });
  const saved = await handleRequest(post(), env, { fetcher: upstream.fetcher, now });
  assert.equal(saved.status, 201);
  assert.deepEqual(upstream.calls.map(call => call[0]), ['POST', 'GET', 'GET', 'PUT']);
});

test('writes browser and desktop payloads as JSON only, with a small generic receipt', async () => {
  const upstream = github();
  const options = { fetcher: upstream.fetcher, now };
  const browser = await handleRequest(post(sample, { Origin: origin }), env, options);
  assert.equal(browser.status, 201);
  assert.equal(browser.headers.get('Access-Control-Allow-Origin'), origin);
  assert.deepEqual(await browser.json(), { id: sample.id });
  assert.deepEqual(JSON.parse(Buffer.from(upstream.files.get(filePath), 'base64')), { ...sample, submittedAt: now.toISOString() });
  const desktopSample = { ...sample, source: 'slate', id: sample.id.replace(/cdef$/, 'ffff'), version: '1.0.6' };
  const desktop = await handleRequest(post(desktopSample), env, options);
  assert.equal(desktop.status, 201);
  assert.equal(upstream.files.size, 2);
  assert.equal(upstream.calls.filter(([method, path]) => method === 'PUT' && path === filePath).length, 1);
});

test('acknowledges matching retry after month rollover and does not overwrite a conflicting ID', async () => {
  const upstream = github();
  const opts = { fetcher: upstream.fetcher, now };
  assert.equal((await handleRequest(post(), env, opts)).status, 201);
  const next = { ...opts, now: new Date('2026-12-01T00:00:03Z') };
  const retry = await handleRequest(post(), env, next);
  assert.equal(retry.status, 200);
  assert.deepEqual(await retry.json(), { id: sample.id });
  const before = upstream.files.get(filePath);
  const conflict = await handleRequest(post({ ...sample, title: 'Different' }), env, next);
  assert.equal(conflict.status, 409);
  assert.deepEqual(await conflict.json(), { error: 'id_conflict' });
  assert.equal(upstream.files.size, 1);
  assert.equal(upstream.files.get(filePath), before);
});

test('returns a safe failure on upstream refusal, timeout, invalid credential, or uncaptured write', async () => {
  const upstream = github({ rejectedWrite: 500 });
  for (const [settings, fetcher] of [
    [env, upstream.fetcher],
    [env, github({ rejectedWrite: 302 }).fetcher],
    [env, async () => { throw new DOMException('secret URL', 'TimeoutError'); }],
    [{ ...env, GITHUB_APP_PRIVATE_KEY: 'invalid' }, unexpectedFetch],
    [env, github({ rejectedWrite: 409 }).fetcher],
  ]) {
    const response = await handleRequest(post(sample, { Origin: origin }), settings, { fetcher, now });
    assert.equal(response.status, 503);
    assert.deepEqual(await response.json(), { error: 'submission_unavailable' });
    assert.equal(response.headers.get('Cache-Control'), 'no-store');
    assert.equal(response.headers.get('Access-Control-Allow-Origin'), origin);
  }
});
