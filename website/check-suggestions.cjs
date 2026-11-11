const assert = require('node:assert/strict');
const { webcrypto } = require('node:crypto');
const fs = require('node:fs');
const vm = require('node:vm');
const { test } = require('node:test');

const script = fs.readFileSync(`${__dirname}/suggestions/suggestions.js`, 'utf8');
const relay = 'https://feedback.example.org/suggestions';

function mount({ endpoint = relay, respond, date = () => new Date() } = {}) {
  const requests = [];
  const fields = Object.fromEntries(Object.entries({ kind: 'Bug', title: '  A & B #1  ', body: 'Line one\nUnicode: café / 画像 & ? #' })
    .map(([key, value]) => ['#' + key, { value, setCustomValidity() {}, reportValidity() { this.invalid = true; }, addEventListener() {} }]));
  const button = {};
  const status = fields['#suggestion-status'] = {};
  let handler;
  const form = {
    dataset: { endpoint },
    querySelector() { return button; },
    addEventListener(type, fn) { handler = fn; },
    reset() { fields['#title'].value = fields['#body'].value = ''; },
  };
  vm.runInNewContext(script, {
    AbortSignal,
    crypto: webcrypto,
    Date: class { constructor() { return date(); } },
    document: { querySelector: selector => selector === '.suggestion-form' ? form : fields[selector] },
    async fetch(url, options) {
      const body = JSON.parse(options.body);
      requests.push({ url, options, body });
      return respond?.(body, requests.length) ?? receipt(body.id);
    },
  });
  return {
    button, fields, status, requests,
    async submit() {
      let prevented = false;
      await handler({ preventDefault() { prevented = true; } });
      assert.equal(prevented, true);
    },
  };
}

function receipt(id, status = 201) {
  return { status, async json() { return { id }; } };
}

test('private POST preserves Unicode and clears after a matching receipt', async () => {
  const page = mount();
  await page.submit();
  const [{ url, options, body }] = page.requests;
  assert.equal(url, relay);
  assert.equal(options.method, 'POST');
  assert.equal(options.headers['Content-Type'], 'application/json');
  assert.equal(options.credentials, 'omit');
  assert.equal(options.redirect, 'error');
  assert.match(body.id, /^\d{6}-[a-f0-9-]{36}$/);
  assert.deepEqual(body, {
    id: body.id,
    source: 'website',
    kind: 'Bug',
    title: 'A & B #1',
    body: 'Line one\nUnicode: café / 画像 & ? #',
  });
  assert.equal(page.fields['#title'].value, '');
  assert.equal(page.fields['#body'].value, '');
  assert.match(page.status.textContent, /was sent/);
});

test('failed deliveries keep drafts and retries reuse their IDs across a month boundary', async () => {
  let current = new Date('2026-11-30T23:59:59Z');
  const page = mount({ date: () => current, respond({ id }, attempt) {
    if (attempt === 1) throw new Error('Connection ended before receipt');
    if (attempt === 2) return receipt('wrong-id');
    return receipt(id, 200);
  } });
  await page.submit();
  assert.match(page.status.textContent, /Couldn’t confirm delivery/);
  assert.match(page.fields['#body'].value, /Unicode/);
  assert.equal(page.button.disabled, false);
  current = new Date('2026-12-01T00:00:01Z');
  await page.submit();
  assert.match(page.status.textContent, /Couldn’t confirm delivery/);
  assert.match(page.fields['#body'].value, /Unicode/);
  await page.submit();
  const ids = page.requests.map(request => request.body.id);
  assert.match(ids[0], /^202611-/);
  assert.deepEqual(ids, [ids[0], ids[0], ids[0]]);
  assert.match(page.status.textContent, /was sent/);
});

test('rate limiting preserves the draft and edits create a new submission', async () => {
  const page = mount({ respond({ id }, attempt) { return receipt(id, attempt === 1 ? 429 : 201); } });
  await page.submit();
  assert.match(page.status.textContent, /Too many suggestions/);
  page.fields['#title'].value = 'Updated title';
  await page.submit();
  assert.notEqual(page.requests[0].body.id, page.requests[1].body.id);
  assert.equal(page.requests[1].body.title, 'Updated title');
});

test('missing configuration and blank drafts never send a request', async () => {
  const disabled = mount({ endpoint: '' });
  assert.equal(disabled.button.disabled, true);
  await disabled.submit();
  assert.match(disabled.status.textContent, /unavailable/);
  assert.equal(disabled.requests.length, 0);

  const blank = mount();
  blank.fields['#title'].value = '   ';
  await blank.submit();
  assert.equal(blank.fields['#title'].invalid, true);
  assert.equal(blank.requests.length, 0);
});
