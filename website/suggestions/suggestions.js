// GitHub owns submissions. This page prepares a draft for the visitor to review.
const form = document.querySelector('.suggestion-form');
form.addEventListener('submit', (event) => {
  event.preventDefault();
  const kind = document.querySelector('#kind').value;
  const title = document.querySelector('#title').value.trim();
  const body = document.querySelector('#body').value.trim();
  if (!title || !body) {
    const emptyField = document.querySelector(!title ? '#title' : '#body');
    emptyField.setCustomValidity('Please add a few words before continuing.');
    emptyField.reportValidity();
    emptyField.addEventListener('input', () => emptyField.setCustomValidity(''), { once: true });
    return;
  }
  const destination = new URL(form.action);
  destination.searchParams.set('title', `[${kind}] ${title}`);
  destination.searchParams.set('body', `## ${kind}\n\n${body}\n\n---\nSubmitted from the Slate suggestion box.`);
  window.location.assign(destination.href);
});
