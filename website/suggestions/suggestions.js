const form = document.querySelector('.suggestion-form');
const kindField = document.querySelector('#kind');
const titleField = document.querySelector('#title');
const bodyField = document.querySelector('#body');
const button = form.querySelector('button[type="submit"]');
const status = document.querySelector('#suggestion-status');
const endpoint = form.dataset.endpoint;
let pending;
let sending = false;

function readDraft() {
  return { kind: kindField.value, title: titleField.value.trim(), body: bodyField.value.trim() };
}

button.disabled = !endpoint;
if (!endpoint) status.textContent = 'Online suggestions are currently unavailable.';

form.addEventListener('submit', async (event) => {
  event.preventDefault();
  if (!endpoint || sending) return;
  const draft = readDraft();
  if (!draft.title || !draft.body) {
    const emptyField = draft.title ? bodyField : titleField;
    emptyField.setCustomValidity('Please add a few words before sending.');
    emptyField.reportValidity();
    emptyField.addEventListener('input', () => emptyField.setCustomValidity(''), { once: true });
    return;
  }

  const key = JSON.stringify(draft);
  sending = true;
  button.disabled = true;
  status.textContent = 'Sending…';
  try {
    if (pending?.key !== key) {
      const month = new Date().toISOString().slice(0, 7).replace('-', '');
      pending = { key, id: `${month}-${crypto.randomUUID()}` };
    }
    const response = await fetch(endpoint, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'omit',
      redirect: 'error',
      signal: AbortSignal.timeout(20_000),
      body: JSON.stringify({ id: pending.id, source: 'website', ...draft }),
    });
    if (response.status === 409) pending = undefined;
    if (response.status === 429) {
      status.textContent = 'Too many suggestions. Please try again in a few minutes.';
      return;
    }
    if (![200, 201].includes(response.status) || (await response.json())?.id !== pending.id) {
      throw new Error('Suggestion receipt missing');
    }
    if (JSON.stringify(readDraft()) === key) form.reset();
    pending = undefined;
    status.textContent = 'Thanks. Your suggestion was sent.';
  } catch {
    status.textContent = 'Couldn’t confirm delivery. Your message is still here. Try again.';
  } finally {
    sending = false;
    button.disabled = false;
  }
});
