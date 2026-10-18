'use strict';
const DATA = JSON.parse(document.getElementById('evaluation-data').textContent);
const KEY = `slate-evaluation:${DATA.catalogHash}`;
const STATUS = ['untested', 'pass', 'fail', 'blocked'];
const LABEL = {untested: 'Untested', pass: 'Pass', fail: 'Fail', blocked: 'Blocked', partial: 'Partial / evidence needed'};
const SCOPES = {current: 'Current change', historical: 'Historical regression', 'known-open': 'Known open'};
const $ = id => document.getElementById(id);
const freshRun = () => ({id: crypto.randomUUID(), started: new Date().toISOString(), locked: false,
  meta: {tester: '', label: `Slate ${DATA.candidateBuild.sha256.slice(0, 8)} — ${new Date().toLocaleDateString()}`, binary: DATA.candidateBuild.path,
    identity: `${DATA.candidateBuild.version} / SHA-256 ${DATA.candidateBuild.sha256}`, context: '', environment: '', confirmed: false},
  results: {}, additional: ''});
let store = {runs: [freshRun()], active: '', handoff: DATA.candidateBuild.sha256};
store.active = store.runs[0].id;
let storageAvailable = true;
try { const saved = JSON.parse(localStorage.getItem(KEY)); if (saved) store = validateStore(saved); }
catch { storageAvailable = false; }
let bridgeReady = window.parent === window;
let filter = {scope: 'current', group: 'all', result: 'all', search: ''};
let current = () => store.runs.find(r => r.id === store.active);
function validateRun(raw) {
  if (!raw || typeof raw.id !== 'string' || !raw.meta || typeof raw.results !== 'object' || !raw.results) throw Error('Invalid saved run.');
  const run = freshRun();
  run.id = raw.id; run.started = typeof raw.started === 'string' ? raw.started : run.started;
  run.locked = raw.locked === true;
  for (const k of Object.keys(run.meta)) run.meta[k] = k === 'confirmed' ? raw.meta[k] === true : String(raw.meta[k] || '');
  run.additional = String(raw.additional || '');
  for (const item of DATA.cases) {
    const source = Object.hasOwn(raw.results, item.id) ? raw.results[item.id] : null;
    if (!source) continue;
    const result = {checks: {}, notes: String(source.notes || ''), evidence: String(source.evidence || '')};
    for (const check of item.checks) {
      const saved = source.checks && Object.hasOwn(source.checks, check.id) ? source.checks[check.id] : null;
      if (saved && STATUS.includes(saved.status)) result.checks[check.id] = {status: saved.status, at: String(saved.at || '')};
    }
    run.results[item.id] = result;
  }
  return run;
}
function validateStore(raw) {
  if (!Array.isArray(raw.runs) || raw.runs.length === 0) throw Error('No saved runs.');
  const runs = raw.runs.map(validateRun);
  if (new Set(runs.map(r => r.id)).size !== runs.length) throw Error('Duplicate run IDs.');
  let active = runs.some(r => r.id === raw.active) ? raw.active : runs[0].id;
  if (raw.handoff !== DATA.candidateBuild.sha256) {const next = freshRun(); runs.unshift(next); active = next.id;}
  return {runs, active, handoff: DATA.candidateBuild.sha256};
}
function persist() {
  try { localStorage.setItem(KEY, JSON.stringify(store)); storageAvailable = true; }
  catch { storageAvailable = false; }
  if (window.parent !== window) window.parent.postMessage({type: 'slate-evaluation-save', catalogHash: DATA.catalogHash, store}, '*');
  $('message').textContent = window.parent !== window ? 'Saved to the canvas sidecar. Export JSON for a portable copy.'
    : storageAvailable ? 'Saved in this browser. Export JSON for the next repair pass.' : 'Browser storage is unavailable. Use Save results JSON to keep your work.';
}
function resultFor(item, run = current()) { return run.results[item.id] || {checks: {}, notes: '', evidence: ''}; }
function verdict(item, run = current()) {
  const result = resultFor(item, run);
  const states = item.checks.map(c => result.checks[c.id]?.status || 'untested');
  if (states.includes('fail')) return 'fail';
  if (states.includes('blocked')) return 'blocked';
  if (states.every(s => s === 'untested')) return 'untested';
  const context = run.locked && run.meta.confirmed && run.meta.tester.trim() && run.meta.identity.trim() && run.meta.binary.trim() && run.meta.context.trim();
  return states.every(s => s === 'pass') && context && (result.notes.trim() || result.evidence.trim()) ? 'pass' : 'partial';
}
function reportVerdicts(run = current()) {
  return Object.keys(DATA.reports).map(id => {
    const cases = DATA.cases.filter(c => c.reports.includes(id));
    const states = cases.map(c => verdict(c, run));
    const status = states.includes('fail') ? 'fail' : states.every(s => s === 'pass') ? 'pass'
      : states.includes('blocked') ? 'blocked' : states.every(s => s === 'untested') ? 'untested' : 'partial';
    return {id, status, archived: DATA.reports[id].archived, cases: cases.map(c => c.id)};
  });
}
function node(tag, text, className) { const el = document.createElement(tag); if (text !== undefined) el.textContent = text; if (className) el.className = className; return el; }
function notify(text) { $('message').textContent = text; }
function fillContext() {
  const run = current();
  for (const [key, value] of Object.entries(run.meta)) {
    const field = $('run-form').elements.namedItem(key);
    if (key === 'confirmed') field.checked = value; else field.value = value;
    field.disabled = run.locked || !bridgeReady;
  }
  $('begin').hidden = run.locked;
  $('locked-context').hidden = !run.locked;
  $('run-label').textContent = run.locked ? `${run.meta.label} · ${run.meta.tester} · ${run.meta.identity}` : 'Not started';
  $('additional').value = run.additional;
  $('additional').disabled = !run.locked || !bridgeReady;
  $('saved-runs').replaceChildren(...store.runs.map(r => {const o = node('option', `${r.meta.label} — ${r.locked ? r.meta.tester : 'draft'}`); o.value = r.id; return o;}));
  $('saved-runs').value = run.id;
  for (const button of document.querySelectorAll('.toolbar button')) button.disabled = !bridgeReady;
  $('saved-runs').disabled = !bridgeReady;
  $('begin').disabled = !bridgeReady;
}
function totals() {
  const counts = {pass: 0, fail: 0, blocked: 0, partial: 0, untested: 0};
  for (const item of DATA.cases) counts[verdict(item)]++;
  $('totals').replaceChildren(...Object.entries(counts).map(([key, value]) => {
    const el = node('div', undefined, 'total'); el.append(node('strong', String(value)), node('span', LABEL[key])); return el;
  }));
  const reports = reportVerdicts();
  $('report-progress').textContent = `${DATA.cases.length} cases / ${reports.length} original reports. ${reports.filter(r => r.status === 'pass').length} reports pass every linked case; ${reports.filter(r => r.status === 'fail' && r.archived).length} historical reports are reopen candidates. Counts always cover the full run, including hidden filters.`;
}
function visible(item) {
  const scope = filter.scope === 'all' || item.scope === filter.scope;
  const group = filter.group === 'all' || item.group.toLowerCase() === filter.group;
  const status = filter.result === 'all' || verdict(item) === filter.result;
  const search = !filter.search || JSON.stringify([item, item.reports.map(id => DATA.reports[id].description)]).toLowerCase().includes(filter.search);
  return scope && group && status && search;
}
function applyResultFilter(item) {
  if (filter.result === 'all' || visible(item)) return;
  document.querySelector(`[data-case="${item.id}"]`)?.remove();
  $('visible-count').textContent = `${DATA.cases.filter(visible).length} cases shown. Mark each expected result independently.`;
}
function setResult(item, edit, filterNow = false) {
  if (!current().locked || !bridgeReady) return;
  const existing = resultFor(item); edit(existing); current().results[item.id] = existing;
  persist(); totals();
  const row = document.querySelector(`[data-case="${item.id}"]`);
  if (row) { row.dataset.verdict = verdict(item); row.querySelector('.verdict').textContent = LABEL[verdict(item)]; }
  if (filterNow) applyResultFilter(item);
}
function list(tag, values) { const el = node(tag); for (const text of values) el.append(node('li', text)); return el; }
function caseNode(item) {
  const result = resultFor(item);
  const box = node('details', undefined, 'case'); box.dataset.case = item.id; box.dataset.verdict = verdict(item);
  const summary = node('summary'); summary.append(node('span', item.title, 'title'), node('span', LABEL[verdict(item)], 'verdict')); box.append(summary);
  const body = node('div', undefined, 'case-body'); body.append(node('p', `${item.group} · ${SCOPES[item.scope]} · ${item.id}`, 'small scope'));
  const instructions = node('div', undefined, 'instructions');
  for (const [title, values] of [['Set up', item.setup], ['Do this', item.steps]]) {
    const part = node('div'); part.append(node('h3', title), list('ol', values)); instructions.append(part);
  }
  body.append(instructions);
  if (item.limitations) body.append(node('p', item.limitations, 'limit'));
  for (const check of item.checks) {
    const row = node('div', undefined, 'check'), label = node('label', check.expected);
    const select = node('select'); select.id = `${item.id}--${check.id}`; label.htmlFor = select.id;
    select.setAttribute('aria-label', `${item.title}: ${check.expected}`);
    for (const value of STATUS) { const o = node('option', LABEL[value]); o.value = value; select.append(o); }
    select.value = result.checks[check.id]?.status || 'untested'; select.disabled = !current().locked || !bridgeReady;
    select.addEventListener('change', () => setResult(item, r => {r.checks[check.id] = {status: select.value, at: new Date().toISOString()};}, true));
    row.append(label, select); body.append(row);
  }
  const notesLabel = node('label', 'Observed result / failure steps / blocked reason (required evidence for a pass)', 'observed');
  const notes = node('textarea'); notes.rows = 2; notes.value = result.notes; notes.disabled = !current().locked || !bridgeReady;
  notes.addEventListener('input', () => setResult(item, r => {r.notes = notes.value;})); notesLabel.append(notes); body.append(notesLabel);
  notes.addEventListener('blur', () => applyResultFilter(item));
  const evidenceLabel = node('label', 'Screenshot / recording / workbook reference (optional if your observation above is specific)');
  const evidence = node('input'); evidence.value = result.evidence; evidence.disabled = !current().locked || !bridgeReady;
  evidence.addEventListener('input', () => setResult(item, r => {r.evidence = evidence.value;})); evidenceLabel.append(evidence); body.append(evidenceLabel);
  evidence.addEventListener('blur', () => applyResultFilter(item));
  const source = node('details', undefined, 'source'); source.append(node('summary', `Original reports (${item.reports.length})`));
  for (const id of item.reports) {
    source.append(node('p', id), node('p', DATA.reports[id].description));
    if (DATA.reports[id].attachments.length) source.append(node('p', `Original attachments: ${DATA.reports[id].attachments.join(', ')}`));
    for (const reference of DATA.reports[id].referenceImages || []) {
      const view = node('details'); view.append(node('summary', `View original reference: ${reference.name}`));
      view.addEventListener('toggle', () => {
        if (view.open && !view.querySelector('img')) {const img = node('img'); img.src = reference.url; img.alt = `Original attachment for ${id}: ${reference.name}`; img.loading = 'lazy'; view.append(img);}
      });
      source.append(view);
    }
    source.append(node('p', DATA.reports[id].path));
  }
  body.append(source); box.append(body); return box;
}
function renderCases(openIds = []) {
  const items = DATA.cases.filter(visible);
  $('cases').replaceChildren(...items.map(item => {const el = caseNode(item); el.open = openIds.includes(item.id); return el;}));
  $('visible-count').textContent = `${items.length} cases shown. ${current().locked ? 'Mark each expected result independently.' : 'Browse the cases now; begin a run to record verdicts.'}`;
}
function render() { fillContext(); totals(); renderCases(); }
function exportObject() {
  return {schema: 'slate-feedback-evaluation/v1', catalogHash: DATA.catalogHash, evaluationId: DATA.evaluationId,
    exportedAt: new Date().toISOString(), run: current(), verdicts: DATA.cases.map(c => ({id: c.id, status: verdict(c)})),
    reports: reportVerdicts(), catalog: DATA.cases, definitions: 'Pass requires every expected result, confirmed running build, human reviewer and case evidence. Reports are not automatically resolved or archived.'};
}
function markdown() {
  const run = current(), lines = ['# Slate acceptance results', '', ...Object.entries(run.meta).map(([k, v]) => `- ${k}: ${v}`),
    `- Run ID: ${run.id}`, `- Started: ${run.started}`, `- Catalog: ${DATA.catalogHash}`, '', '## Report verdicts', ''];
  for (const report of reportVerdicts()) lines.push(`- ${LABEL[report.status]} — ${report.id}${report.archived && report.status === 'fail' ? ' — REOPEN CANDIDATE' : ''}`);
  for (const item of DATA.cases) {
    const r = resultFor(item);
    lines.push('', `## ${LABEL[verdict(item)]}: ${item.title}`, '', `Case: ${item.id} | ${SCOPES[item.scope]}`, '', 'Set up:', ...item.setup.map(s => `- ${s}`), '', 'Actions:', ...item.steps.map((s, i) => `${i + 1}. ${s}`), '', 'Expected results:');
    for (const check of item.checks) lines.push(`- ${LABEL[r.checks[check.id]?.status || 'untested']}: ${check.expected}`);
    lines.push('', `Observed: ${r.notes || '(not recorded)'}`, `Evidence: ${r.evidence || '(not recorded)'}`, `Reports: ${item.reports.join(', ')}`);
    if (item.limitations) lines.push(`Known limit: ${item.limitations}`);
  }
  lines.push('', '## Additional failures', '', run.additional || '(none recorded)', '', DATA.automatedEvidence);
  return lines.join('\n');
}
function download(text, extension, type) {
  const url = URL.createObjectURL(new Blob([text], {type})), a = node('a');
  a.href = url; a.download = `slate-evaluation-${current().id}.${extension}`; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 3000);
}
$('run-form').addEventListener('submit', event => {
  event.preventDefault(); if (!bridgeReady || current().locked || !$('run-form').reportValidity()) return;
  for (const key of Object.keys(current().meta)) {const f = $('run-form').elements.namedItem(key); current().meta[key] = key === 'confirmed' ? f.checked : f.value.trim();}
  current().locked = true; persist(); render(); $('run-details').open = false;
});
$('run-form').addEventListener('input', () => {
  if (current().locked || !bridgeReady) return;
  for (const key of Object.keys(current().meta)) {const f = $('run-form').elements.namedItem(key); current().meta[key] = key === 'confirmed' ? f.checked : f.value;}
  persist();
});
$('new-run').addEventListener('click', () => {const next = freshRun(); store.runs.unshift(next); store.active = next.id; persist(); render(); $('run-details').open = true;});
$('saved-runs').addEventListener('change', () => {store.active = $('saved-runs').value; persist(); render();});
for (const key of ['scope', 'group', 'result', 'search']) $(key).addEventListener(key === 'search' ? 'input' : 'change', () => {filter[key] = $(key).value.toLowerCase(); renderCases();});
$('additional').addEventListener('input', () => {current().additional = $('additional').value; persist();});
$('export-json').addEventListener('click', () => download(JSON.stringify(exportObject(), null, 2), 'json', 'application/json'));
$('export-md').addEventListener('click', () => download(markdown(), 'md', 'text/markdown'));
$('import-toggle').addEventListener('click', () => {$('import-panel').hidden = !$('import-panel').hidden;});
$('import-run').addEventListener('click', () => {
  try {
    const saved = JSON.parse($('import-text').value);
    if (saved.schema !== 'slate-feedback-evaluation/v1' || saved.catalogHash !== DATA.catalogHash) throw Error('This file uses a different checklist. Keep it as historical evidence and start a fresh run here.');
    const run = validateRun(saved.run); run.id = crypto.randomUUID(); store.runs.unshift(run); store.active = run.id;
    persist(); render(); $('import-panel').hidden = true; $('import-text').value = ''; notify('Restored as a separate run. Original saved runs are preserved.');
  } catch (error) {notify(`Could not restore: ${error.message}`);}
});
let beforePrint;
window.addEventListener('beforeprint', () => {
  beforePrint = {filter: {...filter}, open: [...document.querySelectorAll('.case[open]')].map(e => e.dataset.case)};
  filter = {scope: 'all', group: 'all', result: 'all', search: ''}; renderCases(DATA.cases.map(c => c.id));
});
window.addEventListener('afterprint', () => {if (beforePrint) {filter = beforePrint.filter; renderCases(beforePrint.open); beforePrint = null;}});
$('print').addEventListener('click', () => window.print());
window.addEventListener('message', event => {
  if (event.source !== window.parent || event.data?.type !== 'slate-evaluation-init' || event.data.catalogHash !== DATA.catalogHash) return;
  if (event.data.store) {try {store = validateStore(event.data.store);} catch {notify('Canvas saved state was invalid. Start a fresh run or restore your exported JSON.');}}
  for (const [key, value] of Object.entries(event.data.theme || {})) if (['bg', 'fg', 'muted', 'line', 'accent', 'surface'].includes(key) && typeof value === 'string') document.documentElement.style.setProperty(`--${key}`, value);
  bridgeReady = true; render();
});
for (const group of [...new Set(DATA.cases.map(c => c.group))].sort()) {const o = node('option', group); o.value = group.toLowerCase(); $('group').append(o);}
$('candidate').textContent = DATA.candidateBuild.available === false
  ? `Source candidate: ${DATA.candidateBuild.version}. Native binary delivery is pending. Confirm the actual running binary before beginning a run.`
  : `Prefilled candidate: ${DATA.candidateBuild.version}, modified ${DATA.candidateBuild.modified_utc}. This is an available binary, not proof of which build is running. Confirm or replace its identity.`;
$('automated-evidence').textContent = DATA.automatedEvidence;
$('document-date').textContent = `Slate / interactive acceptance document / ${DATA.createdLabel}`;
render();
if (window.parent !== window) window.parent.postMessage({type: 'slate-evaluation-ready', catalogHash: DATA.catalogHash}, '*');
else notify(storageAvailable ? 'All results begin untested. Begin a run after confirming the running build.' : 'Browser storage is unavailable. Export JSON to save your results.');
