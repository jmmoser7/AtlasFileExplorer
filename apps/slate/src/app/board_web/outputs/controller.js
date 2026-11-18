(saved => {
  const element = document.querySelector('script#slate-outputs[data-slate-controller]');
  if (!element) return;
  const schema = element.dataset.slateController;
  const key = Symbol.for('slate.dashboard.controller.v1');
  let runtime = element[key];
  if (!runtime) {
    runtime = { revision: undefined, offered: new Set(), latest: null, value: null };
    element[key] = runtime;
    element.addEventListener('slate-controller-edit', event => {
      if (runtime.revision === undefined) return;
      let value;
      try { value = JSON.stringify(event.detail); } catch { return; }
      if (typeof value !== 'string' || value.length > 8192
          || new TextEncoder().encode(value).length > 8192
          || value === (runtime.latest?.value ?? runtime.value)) return;
      const state = { schema, revision: crypto.randomUUID(), value };
      // All unacknowledged gestures share their last saved base. When Slate
      // acknowledges one, carry any newer gesture forward to that revision.
      if (runtime.offered.size >= 64) runtime.offered.delete(runtime.offered.values().next().value);
      runtime.offered.add(state.revision);
      runtime.latest = state;
      element.dataset.slateEdit = JSON.stringify({base: runtime.revision, state});
    });
  }
  const revision = saved?.revision ?? null;
  if (runtime.revision === revision) return;
  const ownEdit = runtime.offered.has(revision);
  runtime.revision = revision;
  runtime.value = saved?.schema === schema ? saved.value : null;
  if (ownEdit && runtime.latest && runtime.latest.revision !== revision) {
    element.dataset.slateEdit = JSON.stringify({base: revision, state: runtime.latest});
    return;
  }
  runtime.offered.clear();
  runtime.latest = null;
  delete element.dataset.slateEdit;
  if (!ownEdit) {
    const value = runtime.value === null ? null : JSON.parse(runtime.value);
    element.dispatchEvent(new CustomEvent('slate-controller-restore', { detail: value }));
  }
  element.dataset.slateReady = '';
})(JSON.parse(