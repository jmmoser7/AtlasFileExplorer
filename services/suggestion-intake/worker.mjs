const MAX_REQUEST_BYTES = 32 * 1024;
const MAX_GITHUB_BYTES = 128 * 1024;
const MAX_TITLE = 160;
const MAX_BODY = 6000;
const MAX_VERSION = 64;
const ID = /^(20\d\d)(0[1-9]|1[0-2])-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const SOURCES = new Set(['website', 'slate']);
const KINDS = new Set(['Bug', 'Feature', 'Other']);
const FIELDS = new Set(['id', 'source', 'kind', 'title', 'body', 'version']);
const LINE_CONTROL = /[\u0000-\u001f\u007f]/;
const BODY_CONTROL = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/;
const utf8 = new TextEncoder();
const github = 'https://api.github.com';

function response(status, body, origin = null) {
  const headers = { 'Cache-Control': 'no-store', 'Vary': 'Origin', 'X-Content-Type-Options': 'nosniff' };
  if (body) headers['Content-Type'] = 'application/json; charset=utf-8';
  if (origin) headers['Access-Control-Allow-Origin'] = origin;
  if (status === 204) {
    headers['Access-Control-Allow-Methods'] = 'POST';
    headers['Access-Control-Allow-Headers'] = 'Content-Type';
  }
  return new Response(body ? JSON.stringify(body) : null, { status, headers });
}

async function boundedJson(message, maxBytes) {
  const length = message.headers.get('Content-Length');
  if (!message.body || (length !== null && (!/^\d+$/.test(length) || Number(length) > maxBytes))) {
    await message.body?.cancel();
    throw new Error('invalid length');
  }
  const reader = message.body.getReader();
  const decoder = new TextDecoder('utf-8', { fatal: true });
  let count = 0;
  let json = '';
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      count += value.byteLength;
      if (count > maxBytes) throw new Error('too long');
      json += decoder.decode(value, { stream: true });
    }
    return JSON.parse(json + decoder.decode());
  } catch (error) {
    await reader.cancel().catch(() => {});
    throw error;
  } finally {
    reader.releaseLock();
  }
}

function submission(value, now) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).some(key => !FIELDS.has(key))) return null;
  if (typeof value.id !== 'string' || !SOURCES.has(value.source) || !KINDS.has(value.kind)) return null;
  const match = ID.exec(value.id);
  if (!match || typeof value.title !== 'string' || typeof value.body !== 'string') return null;
  const [, year, month] = match;
  // Allow one month of client clock skew and keep older IDs valid for retries.
  const latestMonth = Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1);
  if (Date.UTC(Number(year), Number(month) - 1) > latestMonth) return null;
  const title = value.title.trim();
  const body = value.body.trim();
  if (!title || title.length > MAX_TITLE || LINE_CONTROL.test(title) || !body || body.length > MAX_BODY || BODY_CONTROL.test(body)) return null;
  const version = value.version;
  if (version !== undefined && (typeof version !== 'string' || !version.trim() || version.length > MAX_VERSION || LINE_CONTROL.test(version))) return null;
  const payload = { id: value.id, source: value.source, kind: value.kind, title, body };
  if (version !== undefined) payload.version = version.trim();
  return { payload, path: `submissions/${year}/${month}/${value.id}.json` };
}

function config(env) {
  const { GITHUB_OWNER: owner, GITHUB_REPO: repo, GITHUB_REPOSITORY_ID, GITHUB_APP_CLIENT_ID: clientId, GITHUB_APP_INSTALLATION_ID: installationId, GITHUB_APP_PRIVATE_KEY: key } = env;
  const repositoryId = Number(GITHUB_REPOSITORY_ID);
  if (!/^[A-Za-z0-9-]{1,39}$/.test(owner || '') || !/^[A-Za-z0-9._-]{1,100}$/.test(repo || '') || repo === '.' || repo === '..') return null;
  if (!Number.isSafeInteger(repositoryId) || repositoryId <= 0 || !/^[A-Za-z0-9.]{1,100}$/.test(clientId || '') || !/^[1-9]\d{0,18}$/.test(installationId || '') || typeof key !== 'string') return null;
  if (typeof env.SUGGESTION_RATE_LIMIT?.limit !== 'function') return null;
  return { owner, repo, repositoryId, clientId, installationId, key };
}

function base64(bytes) {
  return btoa(String.fromCharCode(...bytes));
}

function base64url(bytes) {
  return base64(bytes).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function unbase64(value) {
  return Uint8Array.from(atob(value), char => char.charCodeAt(0));
}

async function appJwt(settings, now) {
  const pem = settings.key.trim();
  const match = /^-----BEGIN PRIVATE KEY-----\s+([A-Za-z0-9+/=\s]+)\s+-----END PRIVATE KEY-----$/.exec(pem);
  if (!match) throw new Error('invalid key format');
  const algorithm = 'RSASSA-PKCS1-v1_5';
  const privateKey = await crypto.subtle.importKey('pkcs8', unbase64(match[1].replace(/\s/g, '')), { name: algorithm, hash: 'SHA-256' }, false, ['sign']);
  const current = Math.floor(now.valueOf() / 1000);
  const header = base64url(utf8.encode(JSON.stringify({ alg: 'RS256', typ: 'JWT' })));
  const claims = base64url(utf8.encode(JSON.stringify({ iat: current - 60, exp: current + 480, iss: settings.clientId })));
  const input = `${header}.${claims}`;
  const signature = await crypto.subtle.sign(algorithm, privateKey, utf8.encode(input));
  return `${input}.${base64url(new Uint8Array(signature))}`;
}

function githubClient(fetcher, signal) {
  return (path, bearer, method = 'GET', body = undefined) => fetcher(github + path, {
    method,
    redirect: 'manual',
    signal,
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${bearer}`,
      'Content-Type': 'application/json',
      'User-Agent': 'Slate-Suggestion-Intake',
      'X-GitHub-Api-Version': '2026-03-10',
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

async function save(entry, settings, now, fetcher) {
  const call = githubClient(fetcher, AbortSignal.timeout(8000));
  const jwt = await appJwt(settings, now);
  const access = await call(`/app/installations/${settings.installationId}/access_tokens`, jwt, 'POST', {
    repository_ids: [settings.repositoryId],
    permissions: { contents: 'write' },
  });
  if (access.status !== 201) return 503;
  const { token } = await boundedJson(access, MAX_GITHUB_BYTES);
  if (typeof token !== 'string' || token.length > 8192 || !/^[A-Za-z0-9_.-]+$/.test(token)) return 503;
  const repo = `/repos/${settings.owner}/${settings.repo}`;
  const metadata = await call(repo, token);
  if (metadata.status !== 200) return 503;
  const target = await boundedJson(metadata, MAX_GITHUB_BYTES);
  const owner = target.owner;
  const expectedOwner = settings.owner.toLowerCase();
  if (target.id !== settings.repositoryId || target.private !== true || target.visibility !== 'private' ||
      typeof target.full_name !== 'string' || target.full_name.toLowerCase() !== `${expectedOwner}/${settings.repo.toLowerCase()}` ||
      owner?.type !== 'User' || !Number.isSafeInteger(owner.id) || owner.id <= 0 ||
      typeof owner.login !== 'string' || owner.login.toLowerCase() !== expectedOwner) return 503;
  const accessList = await call(`${repo}/collaborators?affiliation=all&per_page=2`, token);
  if (accessList.status !== 200 || accessList.headers.has('Link')) return 503;
  const collaborators = await boundedJson(accessList, MAX_GITHUB_BYTES);
  if (!Array.isArray(collaborators) || collaborators.length > 2 || collaborators.some(user =>
    user?.id !== owner.id || typeof user.login !== 'string' || user.login.toLowerCase() !== expectedOwner
  )) return 503;
  const path = `${repo}/contents/${entry.path}`;
  const content = { ...entry.payload, submittedAt: now.toISOString() };
  const write = await call(path, token, 'PUT', {
    message: `Add suggestion ${entry.payload.id}`,
    content: base64(utf8.encode(JSON.stringify(content, null, 2) + '\n')),
  });
  if (write.status === 201) return 201;
  if (write.status !== 409 && write.status !== 422) return 503;

  // A write without a SHA cannot update an existing entry. Verify identical
  // content before acknowledging retries or distinguish a conflicting ID.
  const prior = await call(path, token);
  if (prior.status !== 200) return 503;
  const file = await boundedJson(prior, MAX_GITHUB_BYTES);
  if (file.type !== 'file' || file.encoding !== 'base64' || typeof file.content !== 'string') return 503;
  const recorded = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(unbase64(file.content)));
  return [...FIELDS].every(field => recorded[field] === entry.payload[field]) ? 200 : 409;
}

export async function handleRequest(request, env, { fetcher = fetch, now = new Date() } = {}) {
  const origin = request.headers.get('Origin');
  if (origin !== null && (!env.BROWSER_ORIGIN || origin !== env.BROWSER_ORIGIN)) return response(403, { error: 'invalid_origin' });
  const reply = (status, body) => response(status, body, origin);
  const fail = () => reply(503, { error: 'submission_unavailable' });
  const url = new URL(request.url);
  if (url.protocol !== 'https:') return reply(400, { error: 'invalid_submission' });
  if (url.pathname !== '/suggestions' || url.search) return reply(404, { error: 'not_found' });
  if (request.method === 'OPTIONS' && origin && request.headers.get('Access-Control-Request-Method') === 'POST') return reply(204);
  if (request.method !== 'POST') return reply(405, { error: 'method_not_allowed' });
  if (!/^application\/json(?:;\s*charset=utf-8)?$/i.test(request.headers.get('Content-Type') || '')) return reply(400, { error: 'invalid_submission' });
  const settings = config(env);
  const ip = request.headers.get('CF-Connecting-IP');
  if (!settings || !ip || !/^[0-9a-f:.]{3,45}$/i.test(ip)) return fail();
  try {
    const limit = await env.SUGGESTION_RATE_LIMIT.limit({ key: ip });
    if (!limit.success) return reply(429, { error: 'rate_limited' });
  } catch {
    return fail();
  }
  let entry;
  try {
    entry = submission(await boundedJson(request, MAX_REQUEST_BYTES), now);
  } catch {
    return reply(400, { error: 'invalid_submission' });
  }
  if (!entry) return reply(400, { error: 'invalid_submission' });
  try {
    const status = await save(entry, settings, now, fetcher);
    if (status === 200 || status === 201) return reply(status, { id: entry.payload.id });
    if (status === 409) return reply(409, { error: 'id_conflict' });
  } catch {
    // Keep submission text, credentials and upstream response details private.
  }
  return fail();
}

export default { fetch: handleRequest };
