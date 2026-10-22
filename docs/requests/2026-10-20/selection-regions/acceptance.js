(function (root) {
  'use strict';
  const DATA = root.SelectionAcceptanceCatalog;
  const SCHEMA = 'slate-selection-regions-acceptance/v1';
  const KEY = `${SCHEMA}:${DATA.id}:${DATA.revision}`;
  const STATUS = ['untested', 'pass', 'fail', 'blocked'];
  const LABEL = {untested: 'Untested', pass: 'Pass', fail: 'Fail', blocked: 'Blocked'};
  const now = () => new Date().toISOString();
  const uid = () => root.crypto?.randomUUID?.() || `run-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const text = value => typeof value === 'string' ? value.slice(0, 50000) : '';
  function freshRun() {
    return {id: uid(), createdAt: now(), locked: false, meta: {evaluator: '', evaluatorType: 'user',
      label: `Slate selection · ${new Date().toLocaleDateString()}`, version: '', binaryPath: '', sha256: '',
      binaryModified: '', sourceRevision: '', workbook: '', environment: '', identityConfirmed: false},
    results: Object.fromEntries(DATA.cases.map(item => [item.id, {checks: Object.fromEntries(item.checks.map(check =>
      [check.id, {status: 'untested', at: ''}])), notes: '', evidence: '', updatedAt: ''}])), additional: ''};
  }
  function freshStore() { const run = freshRun(); return {catalogId: DATA.id, revision: DATA.revision, active: run.id, runs: [run]}; }
  function validIdentity(run) {
    const m = run.meta;
    return run.locked && m.identityConfirmed && /^[a-fA-F0-9]{64}$/.test(m.sha256.trim()) &&
      ['evaluator', 'version', 'binaryPath', 'workbook', 'environment'].every(key => m[key].trim());
  }
  function summary(item, run) {
    const result = run.results[item.id], values = item.checks.map(check => result.checks[check.id].status);
    const checked = values.filter(status => status !== 'untested').length;
    let status = values.includes('fail') ? 'fail' : values.includes('blocked') ? 'blocked' : 'untested';
    let reason = `${checked}/${values.length} expected results checked`;
    if (values.every(value => value === 'pass')) {
      if (!validIdentity(run)) reason = 'Expected results marked Pass; confirmed executable identity still needed';
      else if (!result.notes.trim() && !result.evidence.trim()) reason = 'Expected results marked Pass; observation or evidence still needed';
      else { status = 'pass'; reason = `${values.length}/${values.length} expected results passed`; }
    }
    return {status, checked, reason};
  }
  function validateRun(raw) {
    if (!raw || typeof raw.id !== 'string' || !raw.meta || !raw.results || typeof raw.results !== 'object') throw Error('Invalid run record.');
    const run = freshRun(); run.id = text(raw.id); run.createdAt = text(raw.createdAt) || run.createdAt; run.locked = raw.locked === true;
    if (!run.id.trim()) throw Error('Run ID is empty.');
    for (const key of ['beganAt', 'restoredAt', 'restoredFromRunId']) if (typeof raw[key] === 'string') run[key] = text(raw[key]);
    for (const key of Object.keys(run.meta)) run.meta[key] = key === 'identityConfirmed' ? raw.meta[key] === true : text(raw.meta[key]);
    if (!['user', 'native-evaluator'].includes(run.meta.evaluatorType)) throw Error('Invalid evaluator type.');
    run.additional = text(raw.additional);
    for (const item of DATA.cases) {
      const source = raw.results[item.id]; if (!source || typeof source !== 'object') continue;
      const result = run.results[item.id]; result.notes = text(source.notes); result.evidence = text(source.evidence); result.updatedAt = text(source.updatedAt);
      for (const check of item.checks) {
        const saved = source.checks?.[check.id]; if (!saved) continue;
        if (!STATUS.includes(saved.status)) throw Error(`Unknown result for ${check.id}.`);
        result.checks[check.id] = {status: saved.status, at: text(saved.at)};
      }
    }
    return run;
  }
  function validateStore(raw) {
    if (!raw || raw.catalogId !== DATA.id || raw.revision !== DATA.revision) throw Error('This record belongs to a different checklist revision.');
    if (!Array.isArray(raw.runs) || raw.runs.length < 1 || raw.runs.length > 100) throw Error('Expected 1–100 saved runs.');
    const runs = raw.runs.map(validateRun);
    if (new Set(runs.map(run => run.id)).size !== runs.length) throw Error('Duplicate run IDs.');
    return {catalogId: DATA.id, revision: DATA.revision, runs, active: runs.some(run => run.id === raw.active) ? raw.active : runs[0].id,
      recoveredStorage: text(raw.recoveredStorage)};
  }
  function exported(store) { return {schema: SCHEMA, exportedAt: now(), catalog: DATA, store}; }
  function restoreExport(raw) {
    if (!raw || raw.schema !== SCHEMA || JSON.stringify(raw.catalog) !== JSON.stringify(DATA)) throw Error('Checklist content/revision does not match this page. Keep the original file with its original checklist.');
    return validateStore(raw.store);
  }
  if (typeof module !== 'undefined') module.exports = {freshRun, freshStore, validIdentity, summary, validateRun, validateStore, exported, restoreExport, SCHEMA, KEY};
  if (typeof document === 'undefined') return;

  const $ = id => document.getElementById(id);
  const node = (tag, content, className) => { const e = document.createElement(tag); if (content !== undefined) e.textContent = content; if (className) e.className = className; return e; };
  let store = freshStore(), storageAvailable = true, initialMessage = 'New run: all native results are Untested.';
  let storedText = '';
  try {
    storedText = root.localStorage.getItem(KEY) || '';
    if (storedText) { store = validateStore(JSON.parse(storedText)); initialMessage = 'Restored this browser’s saved runs. Confirm the saved build before continuing; use a new run for a different build.'; }
  } catch (error) {
    storageAvailable = false; store.recoveredStorage = storedText;
    initialMessage = `Saved results could not be loaded: ${error.message} Export preserves recovered raw text if available.`;
  }
  const current = () => store.runs.find(run => run.id === store.active);
  function notify(message) { $('message').textContent = message; }
  function persist() {
    try { root.localStorage.setItem(KEY, JSON.stringify(store)); storageAvailable = true; }
    catch { storageAvailable = false; }
    notify(storageAvailable ? 'Saved in this browser. Export JSON to keep a portable copy.' : 'Browser storage is unavailable. Export JSON now to keep your work.');
  }
  function readForm() {
    for (const key of Object.keys(current().meta)) {
      const field = $('context-form').elements.namedItem(key);
      current().meta[key] = key === 'identityConfirmed' ? field.checked : field.value;
    }
  }
  function fillContext() {
    const run = current();
    for (const [key, value] of Object.entries(run.meta)) {
      const field = $('context-form').elements.namedItem(key);
      if (key === 'identityConfirmed') field.checked = value; else field.value = value;
      field.disabled = run.locked;
    }
    $('begin').hidden = run.locked; $('context-locked').hidden = !run.locked;
    $('context-label').textContent = run.locked ? `${run.meta.label} · ${run.meta.evaluator} · ${run.meta.sha256.slice(0, 12)}` : 'Not started';
    $('context-panel').open = !run.locked;
    $('saved-runs').replaceChildren(...store.runs.map(saved => {
      const option = node('option', `${saved.meta.label} · ${saved.locked ? saved.meta.evaluator : 'draft'}`); option.value = saved.id; return option;
    }));
    $('saved-runs').value = run.id;
    $('additional').value = run.additional; $('additional').disabled = !run.locked;
  }
  function updateProgress() {
    const counts = Object.fromEntries(STATUS.map(status => [status, 0]));
    for (const item of DATA.cases) {
      const state = summary(item, current()); counts[state.status]++;
      const section = document.getElementById(`case-${item.id}`); if (!section) continue;
      section.dataset.verdict = state.status;
      section.querySelector('.verdict').textContent = LABEL[state.status];
      section.querySelector('.progress').textContent = state.reason;
      section.querySelector('.observed-at').textContent = current().results[item.id].updatedAt ? `Last recorded: ${current().results[item.id].updatedAt}` : 'No native observation recorded';
      const filter = $('filter').value;
      section.hidden = (filter === 'open' && state.status === 'pass') || (['current', 'regression'].includes(filter) && item.category !== filter);
    }
    $('totals').replaceChildren(...STATUS.map(status => {
      const total = node('div', undefined, 'total'); total.append(node('strong', String(counts[status])), node('span', LABEL[status])); return total;
    }));
  }
  function listBlock(title, items, ordered = false) {
    const section = node('section'); section.append(node('h3', title));
    const list = node(ordered ? 'ol' : 'ul'); list.append(...items.map(item => node('li', item))); section.append(list); return section;
  }
  function observationField(item, field, label, rows) {
    const wrapper = node('label', label), input = node('textarea'); input.rows = rows;
    input.value = current().results[item.id][field]; input.disabled = !current().locked;
    input.addEventListener('input', () => {
      const result = current().results[item.id]; result[field] = input.value; result.updatedAt = now(); persist(); updateProgress();
    });
    wrapper.append(input); return wrapper;
  }
  function renderCases() {
    const sections = DATA.cases.map((item, index) => {
      const section = node('details', undefined, 'case'); section.id = `case-${item.id}`; section.open = index === 0;
      const heading = node('summary'); heading.append(node('span', `${item.id} · ${item.title}`, 'title'), node('span', 'Untested', 'verdict')); section.append(heading);
      const body = node('div', undefined, 'case-body'); body.append(node('p', item.concern));
      body.append(node('p', item.category === 'current' ? 'Current change · agreed native behavior' : 'Adjacent native regression · existing behavior must survive', 'small'));
      const instructions = node('div', undefined, 'instructions'); instructions.append(listBlock('Setup', item.setup), listBlock('Actions in native Slate', item.actions, true)); body.append(instructions);
      body.append(node('h3', 'Observable expected results'));
      for (const check of item.checks) {
        const row = node('div', undefined, 'check'), label = node('label', `${check.id} · ${check.text}`), select = node('select');
        select.id = `result-${check.id}`; label.htmlFor = select.id; select.setAttribute('aria-label', `${check.id} result`);
        select.append(...STATUS.map(status => { const option = node('option', LABEL[status]); option.value = status; return option; }));
        select.value = current().results[item.id].checks[check.id].status; select.disabled = !current().locked;
        select.addEventListener('change', () => {
          const result = current().results[item.id]; result.checks[check.id] = {status: select.value, at: now()}; result.updatedAt = now(); persist(); updateProgress();
        });
        row.append(label, select); body.append(row);
      }
      const meta = node('div', undefined, 'case-meta'); meta.append(node('span', '', 'progress'), node('span', '', 'observed-at')); body.append(meta);
      const observations = node('div', undefined, 'observation'); observations.append(
        observationField(item, 'notes', 'Actual behavior / unmet clause IDs / zooms and variants tested / feel', 3),
        observationField(item, 'evidence', 'Evidence references — screenshot/video/log filenames or paths', 2));
      body.append(observations); section.append(body); return section;
    });
    $('cases').replaceChildren(...sections); updateProgress();
  }
  function refresh() { fillContext(); renderCases(); }
  $('context-form').addEventListener('input', () => { if (!current().locked) { readForm(); persist(); } });
  $('context-form').addEventListener('submit', event => { event.preventDefault(); readForm(); current().locked = true; current().beganAt = now(); persist(); refresh(); });
  $('new-run').addEventListener('click', () => { const run = freshRun(); store.runs.unshift(run); store.active = run.id; persist(); refresh(); });
  $('saved-runs').addEventListener('change', () => { store.active = $('saved-runs').value; persist(); refresh(); });
  $('additional').addEventListener('input', () => { current().additional = $('additional').value; persist(); });
  $('filter').addEventListener('change', updateProgress);
  $('expand').addEventListener('click', () => { for (const section of document.querySelectorAll('.case:not([hidden])')) section.open = true; });
  $('restore-toggle').addEventListener('click', () => { $('restore-panel').hidden = !$('restore-panel').hidden; });
  $('restore').addEventListener('click', () => {
    try {
      const source = $('restore-text').value; if (source.length > 2_000_000) throw Error('Results file is too large.');
      const restored = restoreExport(JSON.parse(source));
      const runs = restored.runs.map(run => ({...run, id: uid(), restoredFromRunId: run.id, restoredAt: now()}));
      store.runs.unshift(...runs); store.active = runs[0].id; persist(); refresh();
      notify(`Added ${runs.length} restored run(s). Existing runs were preserved; check the selected build identity.`);
    } catch (error) { notify(`Nothing restored: ${error.message}`); }
  });
  $('export').addEventListener('click', () => {
    const output = JSON.stringify(exported(store), null, 2); $('export-text').value = output; $('export-panel').hidden = false;
    try {
      const url = URL.createObjectURL(new Blob([output], {type: 'application/json'})), link = node('a');
      link.href = url; link.download = `slate-selection-results-${new Date().toISOString().slice(0, 10)}.json`; link.click();
      root.setTimeout(() => URL.revokeObjectURL(url), 1000);
      notify('Export prepared. If the download did not open, copy the portable JSON shown above. It includes the checklist, exact-build context, all runs and results.');
    } catch { notify('Download is unavailable. Copy the portable JSON shown above to preserve the complete record.'); }
  });
  $('print').addEventListener('click', () => { for (const section of document.querySelectorAll('details')) section.open = true; root.print(); });
  refresh(); notify(initialMessage);
})(typeof window === 'undefined' ? globalThis : window);
