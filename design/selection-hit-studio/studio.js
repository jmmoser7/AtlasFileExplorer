(function () {
  'use strict';
  const G = window.HitGeometry, E = window.HitEngine, B = window.HitBaseline, S = window.HitSemantic, A = window.HitAdaptive;
  const scaled = () => ['semantic', 'adaptive'].includes(state.tinyStrategy);
  const $ = id => document.getElementById(id), canvas = $('stage');
  const qa = new URLSearchParams(window.location.search).has('qa');
  const storageKey = 'slate-selection-studio-v1' + (qa ? '-qa' : '');
  if (qa) document.title += ' · QA session';
  const knobs = {'corner-hit': 'cornerHit', 'edge-hit': 'edgeHit', 'hit-slop': 'hitSlop',
    'drag-threshold': 'dragThreshold', 'strip-gap': 'stripGap', 'bottom-gap': 'bottomGap', 'rotate-hit': 'rotateHit', 'rotate-offset': 'rotateOffset'};
  const state = {config: G.clone(B.config), ...G.preset('scale'), tinyStrategy: 'adaptive', showChrome: true, zones: true, labels: true};
  const view = {w: 800, h: 600, z: 1, cx: 0, cy: 0};
  $('preset').value = 'scale';
  let zoomFrame = null, zoomTarget = 1, zoomAnchor = null, zoomTime = 0;
  let drag = null, pointerId = null, hover = null, point = null, heat = [], feedback = [], records = [];
  let undo = [], redo = [], last = null, pendingSave = null;
  let ladderKey = ''; const ladderRows = new Map();
  const say = message => { $('notice').textContent = message; };
  function clearScan() { heat = []; $('scan-results').textContent = ''; }
  const label = hit => hit && hit.kind !== 'canvas' ? `${hit.node} · ${hit.kind}${hit.handle ? ' ' + hit.handle.toUpperCase() : ''}${hit.action ? ' / ' + hit.action : ''}${hit.side ? ' ' + hit.side : ''}` : 'Canvas';
  const diff = () => Object.fromEntries(Object.entries(state.config).filter(([key, value]) => value !== B.config[key]));
  function syncDimensions() {
    const selected = state.nodes.find(n => n.id === state.selected[0]);
    for (const [axis, key] of [['width', 'w'], ['height', 'h'], ['angle', 'rotation'], ['radius', 'radius']]) {
      $(axis).disabled = !selected; $(axis).value = selected ? Math.round((selected[key] || 0) * 100) / 100 : '';
    }
  }
  function draw() {
    hover = point ? G.inspect(state, view, point) : null;
    const layout = window.HitRender.draw(canvas, state, view, hover, drag, heat); updateLadder(layout);
    $('target').textContent = label(drag?.hit || hover?.winner);
    $('contenders').textContent = hover ? hover.candidates.map(n => `${label(n)}${n.occluded ? ' (clipped / move core)' : ''}`).join('; ') || 'None' : 'Move over the canvas';
    const kind = drag?.hit.kind || hover?.winner?.kind;
    canvas.style.cursor = kind === 'body' ? (drag?.started ? 'grabbing' : 'grab') :
      ['resize', 'crop'].includes(kind) ? `${drag?.hit.handle || hover.winner.handle}-resize` :
      ['toolbar', 'dimension', 'align', 'fillet', 'popup'].includes(kind) ? 'pointer' : ['wire', 'rotate'].includes(kind) ? 'crosshair' : 'default';
    $('gesture').textContent = drag ? `${drag.started ? 'Dragging' : 'Pressed'} · ${label(drag.hit)} · owner locked` :
      last ? `${last.started ? 'Completed drag' : 'Click'} · ${label(last.hit)}` : 'Idle';
    if (drag?.started) syncDimensions();
  }
  function updateLadder(layout) {
    const holder = $('scale-ladder'); holder.hidden = !scaled(); if (holder.hidden) return;
    const key = JSON.stringify(state.nodes.map(n => [n.id, n.label]));
    if (key !== ladderKey) {
      ladderKey = key; ladderRows.clear(); holder.replaceChildren();
      const header = document.createElement('div'); header.className = 'ladder-row ladder-header';
      for (const value of ['Screen size', 'Wire', 'Resize', 'Turn', 'Move']) { const label = document.createElement('span'); label.textContent = value; header.append(label); }
      holder.append(header);
      for (const node of state.nodes) {
        const row = document.createElement('div'); row.className = 'ladder-row'; const title = document.createElement('span'); row.append(title);
        const bars = {};
        for (const domain of ['wire', 'resize', 'rotate', 'move']) { const bar = document.createElement('span'); bar.className = 'domain-bar ' + domain; row.append(bar); bars[domain] = bar; }
        ladderRows.set(node.id, {title, bars}); holder.append(row);
      }
    }
    for (const card of layout.cards) {
      const row = ladderRows.get(card.node.id); row.title.textContent = card.visible ? `${card.node.label || card.node.id} ${card.projectedSize.toFixed(1)}px` : `${card.node.label || card.node.id} offscreen`;
      for (const [domain, bar] of Object.entries(row.bars)) {
        const p = card.domains[domain], active = layout.zones.some(n => !n.suppressed && n.node === card.node.id && (domain === 'move' ? n.kind === 'body' : domain === 'resize' ? ['resize','crop','fillet'].includes(n.kind) : n.kind === domain));
        bar.style.setProperty('--weight', p); bar.classList.toggle('dormant', !active);
        bar.title = `${domain}: ${Math.round(p * 100)}% prominence · ${active ? 'active' : card.covered ? 'covered; select in Objects' : 'dormant'}`;
        bar.setAttribute('aria-label', bar.title);
      }
    }
  }
  function refresh() {
    $('objects').replaceChildren();
    for (const node of [...state.nodes].reverse()) {
      const button = document.createElement('button'); button.type = 'button';
      button.textContent = `${node.id} · ${node.label || node.kind}${state.nodes.at(-1).id === node.id ? ' · front' : ''}`;
      button.setAttribute('aria-pressed', String(state.selected.includes(node.id)));
      button.onclick = event => {
        cancel(); last = null; state.selected = event.shiftKey ? state.selected.includes(node.id) ? state.selected.filter(id => id !== node.id) : [...state.selected, node.id] : [node.id];
        state.popup = null; refresh(); save();
      };
      $('objects').append(button);
    }
    syncDimensions();
    $('mode').value = state.mode;
    $('tiny-strategy').value = state.tinyStrategy;
    $('undo').disabled = !undo.length; $('redo').disabled = !redo.length;
    $('front').disabled = $('back').disabled = !state.selected.length;
    $('center-view').disabled = !state.selected.length;
    $('feedback-count').textContent = feedback.length;
    for (const [id, key] of Object.entries(knobs)) { $(id).value = state.config[key]; $(id + '-value').value = state.config[key]; }
    for (const id of ['hit-slop', 'strip-gap', 'bottom-gap']) {
      $(id).disabled = scaled();
      $(id).title = $(id).disabled ? 'Source/expanded profiles only; zoom proposals use screen-space hit geometry and a property rail.' : '';
    }
    const count = Object.keys(diff()).length + (state.tinyStrategy !== 'source' ? 1 : 0);
    $('proposal-status').textContent = count ? `${count} proposed change${count === 1 ? '' : 's'}` : 'Source baseline';
    $('proposal-status').classList.toggle('changed', count > 0);
    draw();
  }
  function save() {
    clearTimeout(pendingSave);
    pendingSave = setTimeout(() => {
      try { localStorage.setItem(storageKey, JSON.stringify(payload())); }
      catch { say('Local saving is unavailable. Export checks to keep your work.'); }
    }, 200);
  }
  function commit(before, after = E.snapshot(state)) {
    last = null;
    if (!E.changed(before, after)) return;
    undo.push({before: G.clone(before), after: G.clone(after)}); undo = undo.slice(-100); redo = []; clearScan();
  }
  function cancel() {
    stopZoom();
    if (drag) E.cancel(state, drag);
    drag = null;
    if (pointerId !== null && canvas.hasPointerCapture(pointerId)) canvas.releasePointerCapture(pointerId);
    pointerId = null;
  }
  function coordinates(event) {
    const r = canvas.getBoundingClientRect(); return {x: event.clientX - r.left, y: event.clientY - r.top};
  }
  canvas.addEventListener('pointerdown', event => {
    if (event.button !== 0 || pointerId !== null) return;
    event.preventDefault(); stopZoom(); canvas.focus(); point = coordinates(event);
    drag = E.begin(state, view, point, event.shiftKey); pointerId = event.pointerId;
    canvas.setPointerCapture(pointerId); clearScan(); refresh();
  });
  canvas.addEventListener('pointermove', event => {
    if (pointerId !== null && event.pointerId !== pointerId) return;
    point = coordinates(event); E.move(state, drag, point, {alt: event.altKey, shift: event.shiftKey}); draw();
  });
  canvas.addEventListener('pointerup', event => {
    if (event.pointerId !== pointerId) return;
    point = coordinates(event); E.move(state, drag, point, {alt: event.altKey, shift: event.shiftKey});
    const result = E.finish(state, view, drag);
    last = null;
    if (result) {
      commit(result.before, result.after);
      last = {...result, testedConfig: G.clone(state.config), tinyStrategy: state.tinyStrategy,
        testedSemanticPolicy: scaled() ? G.clone(S.policy) : undefined,
        testedAdaptivePolicy: state.tinyStrategy === 'adaptive' ? G.clone(A.policy) : undefined,
        zoom: view.z, view: G.clone(view), sourceRevision: B.revision, at: new Date().toISOString()};
      records.push(G.clone(last)); records = records.slice(-100);
    }
    const focusDimension = !drag?.started && drag?.hit.kind === 'dimension';
    drag = null; canvas.releasePointerCapture(pointerId); pointerId = null;
    refresh(); save();
    if (focusDimension) { $('width').focus(); $('width').select(); }
  });
  canvas.addEventListener('pointercancel', () => { cancel(); refresh(); say('Gesture cancelled; scene restored.'); });
  canvas.addEventListener('lostpointercapture', () => { if (drag) { cancel(); refresh(); } });
  canvas.addEventListener('pointerleave', () => { if (!drag) { point = null; draw(); } });
  window.addEventListener('blur', () => { if (drag) { cancel(); refresh(); } });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape') { cancel(); state.popup = null; refresh(); return; }
    if (event.target.closest('input,textarea,select')) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z') {
      event.preventDefault(); history(event.shiftKey);
    }
  });
  function history(forward) {
    cancel(); const item = (forward ? redo : undo).pop(); if (!item) return;
    E.restore(state, forward ? item.after : item.before); (forward ? undo : redo).push(item);
    state.popup = null; clearScan(); last = null; refresh(); save();
  }
  $('undo').onclick = () => history(false); $('redo').onclick = () => history(true);
  function resetScene() {
    cancel(); Object.assign(state, G.preset($('preset').value)); undo = []; redo = []; last = null; clearScan(); point = null;
    fit(); refresh(); save(); say('Scene reset. Proposed hit settings and recorded checks are retained.');
  }
  function fit() {
    const bounds = G.union(state.nodes.map(n => ({x: n.x, y: n.y, w: n.w, h: n.h})));
    const extentX = Math.max(Math.abs(bounds.x), Math.abs(bounds.x + bounds.w)) * 2 + 100;
    const extentY = Math.max(Math.abs(bounds.y), Math.abs(bounds.y + bounds.h)) * 2 + 180;
    view.cx = 0; view.cy = 0;
    view.z = Math.max(S.policy.zoomMin, Math.min(1, (view.w - 24) / extentX, (view.h - 24) / extentY));
    view.z = Math.max(S.policy.zoomMin, Math.round(view.z * 10000) / 10000); updateZoom();
  }
  function updateZoom() { $('zoom').value = Math.log2(view.z) * 100; $('zoom-value').value = `${Number((view.z * 100).toFixed(2))}%`; }
  function stopZoom() { if (zoomFrame !== null) cancelAnimationFrame(zoomFrame); zoomFrame = null; zoomTarget = view.z; }
  function animateZoom(time) {
    const alpha = 1 - Math.exp(-Math.min(64, time - zoomTime) / 65); zoomTime = time;
    let next = Math.exp(Math.log(view.z) + (Math.log(zoomTarget) - Math.log(view.z)) * alpha);
    const done = Math.abs(Math.log(zoomTarget / next)) < 0.0005; if (done) next = zoomTarget;
    Object.assign(view, S.zoomAt(view, next, zoomAnchor.screen, zoomAnchor.world)); updateZoom(); draw();
    if (done) { zoomFrame = null; save(); } else zoomFrame = requestAnimationFrame(animateZoom);
  }
  canvas.addEventListener('wheel', event => {
    event.preventDefault(); if (pointerId !== null) return;
    const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? view.h : 1);
    if (!Number.isFinite(delta) || !delta) return;
    point = coordinates(event); last = null; clearScan();
    if (zoomFrame === null) zoomTarget = view.z;
    zoomTarget = Math.max(S.policy.zoomMin, Math.min(S.policy.zoomMax, zoomTarget * Math.exp(-Math.max(-2000, Math.min(2000, delta)) * S.policy.wheelRate)));
    zoomAnchor = {screen: {...point}, world: S.worldAt(point, view)};
    if (zoomFrame === null) { zoomTime = performance.now(); zoomFrame = requestAnimationFrame(animateZoom); }
  }, {passive: false});
  $('reset').onclick = resetScene; $('preset').onchange = resetScene;
  $('center-view').onclick = () => {
    cancel(); const selected = state.nodes.filter(n => state.selected.includes(n.id)); if (!selected.length) return;
    const bounds = G.union(selected.map(n => G.boundsOf(G.oriented(n, n, G.center(n)))));
    view.cx = bounds.x + bounds.w / 2; view.cy = bounds.y + bounds.h / 2; last = null; clearScan(); draw(); save();
  };
  $('mode').onchange = () => {
    cancel(); last = null; state.mode = $('mode').value; state.selected = state.selected.slice(0, 1); state.popup = null;
    clearScan(); refresh(); save(); say(state.mode === 'crop' ? 'Crop proxy: handles resize the viewport; body drags pan its test image.' : 'Selection mode.');
  };
  $('zoom').oninput = () => { cancel(); last = null; view.z = 2 ** (Number($('zoom').value) / 100); updateZoom(); clearScan(); draw(); save(); };
  $('tiny-strategy').onchange = () => {
    cancel(); last = null; state.tinyStrategy = $('tiny-strategy').value; clearScan(); refresh(); save();
    say(state.tinyStrategy === 'semantic' ? 'Semantic zoom: fine controls yield to moving as projected size shrinks. Outlined patches remain reachable; leaders carry no input.' : state.tinyStrategy === 'expanded' ? 'Previous proposal: separate controls outside tiny objects.' : 'Slate source profile: original overlapping target geometry.');
  };
  for (const [id, property] of [['zones', 'zones'], ['labels', 'labels'], ['show-chrome', 'showChrome']])
    $(id).onchange = () => { cancel(); last = null; state[property] = $(id).checked; clearScan(); draw(); };
  for (const [id, key] of Object.entries(knobs)) $(id).oninput = () => {
    cancel(); last = null; state.config[key] = Number($(id).value); clearScan(); refresh(); save();
  };
  $('baseline').onclick = () => { cancel(); last = null; state.config = G.clone(B.config); state.tinyStrategy = 'source'; clearScan(); refresh(); save(); say('Restored source configuration from 2087cc17 / preview.175.'); };
  for (const [id, key] of [['width', 'w'], ['height', 'h'], ['angle', 'rotation'], ['radius', 'radius']]) $(id).onchange = () => {
    const value = Number($(id).value); if (!Number.isFinite(value) || value < Number($(id).min) || value > Number($(id).max)) { refresh(); return; }
    cancel(); const before = E.snapshot(state);
    state.nodes = state.nodes.map(n => state.selected.includes(n.id) ? {...n, [key]: key === 'radius' ? Math.min(value, n.w / 2, n.h / 2) : value} : n);
    commit(before); clearScan(); refresh(); save();
  };
  $('add').onclick = () => {
    if (state.nodes.length >= 30) { say('The sandbox supports up to 30 objects.'); return; }
    cancel(); const before = E.snapshot(state); let i = 1; while (state.nodes.some(n => n.id === 'N' + i)) i++;
    const node = {id: 'N' + i, kind: 'image', x: -60 + (i % 4) * 30, y: -30 + (i % 3) * 30, w: 160, h: 120, radius: 0};
    state.nodes.push(node); state.selected = [node.id]; commit(before); refresh(); save();
  };
  for (const [id, front] of [['front', true], ['back', false]]) $(id).onclick = () => {
    cancel(); const before = E.snapshot(state), picked = state.nodes.filter(n => state.selected.includes(n.id));
    const rest = state.nodes.filter(n => !state.selected.includes(n.id)); state.nodes = front ? [...rest, ...picked] : [...picked, ...rest];
    commit(before); clearScan(); refresh(); save();
  };
  $('scan').onclick = () => {
    if (state.tinyStrategy === 'adaptive') {
      clearScan(); const result = A.audit(state, view, G);
      $('scan-results').textContent = `${result.reachable}/${result.total} targets have sampled winning area (4px grid). ${result.hidden.length} covered, out of view, or below sampling resolution: ${result.hidden.map(n => n.id).join(', ') || 'none'}. ${result.dormant} fine controls dormant. This reports occlusion; it is not a pass verdict. Select a buried object in Objects for its outer frame.`;
      draw(); return;
    }
    if (state.tinyStrategy === 'semantic') {
      const result = S.audit(state, view, G); clearScan(); heat = result.failures.filter(n => n.point).map(n => n.point);
      $('scan-results').textContent = `${result.active} active targets, nine probes each: ${result.failures.length} blocked probes. ${result.dormant} fine targets dormant; ${result.offscreen} objects offscreen. Prototype check; native acceptance is separate.`;
      draw(); return;
    }
    clearScan(); let sampled = 0, occluded = 0;
    for (let y = 12; y < view.h - 12; y += 14) for (let x = 12; x < view.w - 12; x += 14) {
      const pick = G.inspect(state, view, {x, y}); sampled++;
      if (pick.candidates.length > 1) { heat.push({x, y}); if (pick.candidates.some(n => n.occluded)) occluded++; }
    }
    $('scan-results').textContent = `${heat.length} overlapping samples / ${sampled}; ${occluded} include an occluded target. Amber dots show candidate overlap, including intentional overlap. This is a model scan, not a pass/fail result.`;
    draw();
  };
  for (const [id, verdict] of [['mark-pass', 'works-as-expected'], ['mark-fail', 'needs-change']]) $(id).onclick = () => {
    if (feedback.length >= 500) { say('Export your 500 recorded checks before starting another session.'); return; }
    feedback.push({verdict, note: $('note').value.trim().slice(0, 10000), at: new Date().toISOString(), baseline: B.revision,
      config: G.clone(state.config), scene: E.snapshot(state), mode: state.mode, zoom: view.z, view: G.clone(view), preset: $('preset').value,
      display: {zones: state.zones, labels: state.labels, chrome: state.showChrome}, tinyStrategy: state.tinyStrategy,
      semanticPolicy: scaled() ? G.clone(S.policy) : undefined,
      adaptivePolicy: state.tinyStrategy === 'adaptive' ? G.clone(A.policy) : undefined,
      gesture: G.clone(last), pointer: point ? G.clone(point) : null});
    $('note').value = ''; refresh(); save(); say(`Recorded check ${feedback.length}. Export checks to share it with the next agent.`);
  };
  function payload() {
    return {schema: 'slate-selection-hit-studio/v1', baseline: {revision: B.revision, tree: B.tree, build: B.build, config: B.config, sources: B.sources},
      proposal: {config: G.clone(state.config), changes: diff(), tinyStrategy: state.tinyStrategy, tinyPolicy: G.clone(G.tiny), semanticPolicy: G.clone(S.policy), semanticVersion: 1, adaptivePolicy: G.clone(A.policy), adaptiveVersion: 2, tinyConfidence: 55}, scene: E.snapshot(state), mode: state.mode,
      preset: $('preset').value, zoom: view.z, camera: {cx: view.cx, cy: view.cy}, feedback: G.clone(feedback), gestures: G.clone(records),
      limitations: B.simplifications, confidence: B.confidence, implementationBrief: {
        status: 'Sandbox proposal; user approval and native validation still required.',
        values: [...Object.entries(diff()).map(([key, value]) => `${key}: ${B.config[key]} → ${value}; owner: ${B.parameterSources[key] || 'see source snapshot'}`),
          ...(state.tinyStrategy === 'adaptive' ? ['PROPOSED: semantic zoom + continuous cursor-expanded regions. Stacked hosts preserve foreground move/resize/rotation priority; overlapping foreground wires require host-interior hover. Adjacent coincident corners prefer the nearer host silhouette, then front order at exact equality. Buried selections expose an outside frame.'] : state.tinyStrategy === 'semantic' ? ['PREVIOUS PROPOSAL: smooth zoom prominence with displaced protected points. Retained for comparison after overlap feedback.'] : state.tinyStrategy === 'expanded' ? ['PROPOSED: at ≤24 screen px, separate resize/rotate/fillet controls on a minimum 72 px cage; minimum 18 px move target.'] : [])],
        unresolved: feedback.filter(item => item.verdict === 'needs-change').map(item => item.note || 'Review recorded gesture'),
        next: 'Refine existing Rust owners; preserve press ownership and one undo per gesture. Run native interactive acceptance after integration.'}};
  }
  $('export').onclick = () => {
    const json = JSON.stringify(payload(), null, 2); $('transfer').value = json; $('transfer-panel').open = true;
    const url = URL.createObjectURL(new Blob([json], {type: 'application/json'}));
    const link = document.createElement('a'); link.href = url; link.download = 'slate-selection-checks.json'; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000); say('Exported configuration, scene, observations, and source references. JSON is also available below.');
  };
  $('import').onclick = () => { $('transfer-panel').open = true; $('transfer').focus(); };
  function validate(value) {
    if (value?.schema !== 'slate-selection-hit-studio/v1') throw Error('Unsupported studio JSON schema.');
    const config = G.clone(B.config);
    for (const [id, key] of Object.entries(knobs)) {
      const n = value.proposal?.config?.[key] ?? (key.startsWith('rotate') ? B.config[key] : undefined);
      if (!Number.isFinite(n) || n < Number($(id).min) || n > Number($(id).max)) throw Error(`Invalid ${key}.`);
      config[key] = n;
    }
    if (!Array.isArray(value.scene?.nodes) || !value.scene.nodes.length || value.scene.nodes.length > 30) throw Error('Expected 1–30 scene objects.');
    const ids = new Set();
    const nodes = value.scene.nodes.map(node => {
      if (typeof node.id !== 'string' || !/^[A-Za-z0-9_-]{1,24}$/.test(node.id) || ids.has(node.id)) throw Error('Invalid or duplicate object ID.');
      ids.add(node.id);
      if (!['image', 'shape', 'generator'].includes(node.kind)) throw Error('Invalid object kind.');
      const next = {id: node.id, kind: node.kind};
      if (typeof node.label === 'string') next.label = node.label.slice(0, 40);
      for (const key of ['x', 'y', 'w', 'h', 'radius', 'panX', 'panY', 'rotation']) {
        const n = node[key] ?? 0;
        if (!Number.isFinite(n) || Math.abs(n) > 10000 || (['w', 'h'].includes(key) && (n < 1 || n > 4000)) || (key === 'radius' && n < 0) || (key === 'rotation' && Math.abs(n) > 180)) throw Error(`Invalid object ${key}.`);
        next[key] = n;
      }
      return next;
    });
    if (!Array.isArray(value.scene.selected) || value.scene.selected.some(id => !ids.has(id))) throw Error('Invalid selection.');
    const selected = [...new Set(value.scene.selected)];
    if (!['select', 'crop'].includes(value.mode) || (value.mode === 'crop' && selected.length > 1)) throw Error('Invalid mode or crop selection.');
    if (!Array.isArray(value.scene.wires) || value.scene.wires.length > 200) throw Error('Invalid wires.');
    const wires = value.scene.wires.map(wire => {
      if (!ids.has(wire.from) || (wire.to !== null && !ids.has(wire.to))) throw Error('Invalid wire endpoint.');
      if (wire.to === null && (!Number.isFinite(wire.end?.x) || !Number.isFinite(wire.end?.y) || Math.abs(wire.end.x) > 10000 || Math.abs(wire.end.y) > 10000)) throw Error('Invalid free wire endpoint.');
      return {from: wire.from, to: wire.to, fromSide: wire.fromSide === 'in' ? 'in' : 'out', end: wire.to === null ? {x: wire.end.x, y: wire.end.y} : undefined};
    });
    if (!Number.isFinite(value.zoom) || value.zoom < S.policy.zoomMin || value.zoom > S.policy.zoomMax) throw Error('Invalid zoom.');
    const camera = {cx: value.camera?.cx ?? 0, cy: value.camera?.cy ?? 0};
    if (Object.values(camera).some(n => !Number.isFinite(n) || Math.abs(n) > 1e7)) throw Error('Invalid camera.');
    if (!Array.isArray(value.feedback) || value.feedback.length > 500 || value.feedback.some(item =>
      !item || !['works-as-expected', 'needs-change'].includes(item.verdict) || typeof item.note !== 'string' || item.note.length > 10000)) throw Error('Invalid recorded checks.');
    const tinyStrategy = value.proposal?.tinyStrategy ?? 'source';
    if (!['source', 'expanded', 'semantic', 'adaptive'].includes(tinyStrategy)) throw Error('Invalid tiny-target strategy.');
    return {config, nodes, selected, wires, mode: value.mode, zoom: value.zoom, tinyStrategy, camera};
  }
  function apply(value) {
    const {zoom, camera, ...clean} = validate(value); cancel();
    Object.assign(state, clean, {popup: null}); view.z = zoom; view.cx = camera.cx; view.cy = camera.cy; updateZoom();
    $('preset').value = ['single', 'overlap', 'overlap-lab', 'adjacent', 'tiny', 'crop', 'wire', 'crowded', 'scale'].includes(value.preset) ? value.preset : 'single';
    feedback = Array.isArray(value.feedback) ? value.feedback.slice(-500) : [];
    records = Array.isArray(value.gestures) ? value.gestures.slice(-100) : [];
    undo = []; redo = []; last = null; point = null; clearScan();
    return value.baseline?.revision !== B.revision;
  }
  $('apply-import').onclick = () => {
    try {
      if ($('transfer').value.length > 5000000) throw Error('JSON is too large (limit 5 MB).');
      const mismatch = apply(JSON.parse($('transfer').value)); refresh(); save();
      say(mismatch ? 'Imported as a proposal from a different source revision. Native parity is unverified.' : 'Imported checks and proposed configuration.');
    } catch (error) { say(`Import rejected: ${error.message}`); }
  };
  Object.defineProperty(window, 'HitStudioSnapshot', {get: () => G.clone({state, view, last, feedback, undoCount: undo.length, redoCount: redo.length})});
  new ResizeObserver(() => {
    cancel(); const r = canvas.getBoundingClientRect();
    if (r.width !== view.w || r.height !== view.h) last = null;
    view.w = r.width; view.h = r.height; clearScan(); draw();
  }).observe(canvas);
  const r = canvas.getBoundingClientRect(); view.w = r.width; view.h = r.height;
  try {
    const saved = localStorage.getItem(storageKey);
    if (saved) {
      const previous = JSON.parse(saved), upgraded = previous.proposal && !previous.proposal.adaptiveVersion;
      if (upgraded && previous.proposal.tinyStrategy !== 'source') previous.proposal.tinyStrategy = 'adaptive';
      const mismatch = apply(previous); say(upgraded ? 'Kept your scene and tuning. Adaptive regions replace the protected-point proposal. Try Overlap torture; previous rules remain available for comparison.' : mismatch ? 'Restored an older proposal. Compare its source revision before integration.' : 'Restored your previous sandbox session.');
    }
    else fit();
  } catch { say('Previous session could not be restored. Source configuration is ready.'); }
  refresh();
})();
