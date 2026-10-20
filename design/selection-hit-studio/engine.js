/* The sandbox's single gesture owner. Pointer events and deterministic replays
   both call this engine. This does not implement Slate's document journal. */
(function (root) {
  'use strict';
  const G = typeof module !== 'undefined' ? require('./geometry.js') : root.HitGeometry;
  const snapshot = state => G.clone({nodes: state.nodes, selected: state.selected, wires: state.wires});
  const restore = (state, scene) => Object.assign(state, G.clone(scene));
  const changed = (a, b) => JSON.stringify([a.nodes, a.wires]) !== JSON.stringify([b.nodes, b.wires]);
  function begin(state, view, point, shift = false) {
    const before = snapshot(state), popup = state.popup;
    let pick = G.inspect(state, view, point);
    if (state.popup && !['popup', 'toolbar'].includes(pick.winner?.kind)) {
      state.popup = null;
      if (!['semantic', 'adaptive'].includes(state.tinyStrategy) || !pick.winner) pick = G.inspect(state, view, point);
    }
    const hit = pick.winner || {node: null, kind: 'canvas', shape: {...point, w: 0, h: 0}};
    if (hit.kind === 'canvas' && !shift) state.selected = [];
    if (['body', 'resize', 'crop', 'fillet', 'wire', 'rotate'].includes(hit.kind)) {
      if (shift && hit.kind === 'body' && !state.selected.includes(hit.node)) state.selected = [...state.selected, hit.node];
      else if (!state.selected.includes(hit.node)) state.selected = [hit.node];
    }
    const host = state.nodes.find(n => n.id === hit.node), origin = host ? G.center(G.screenRect(host, view)) : point;
    return {hit: G.clone(hit), start: {...point}, current: {...point}, before, popup, origin,
      control: pick.layout.cards.find(card => card.node.id === hit.node)?.control,
      startAngle: Math.atan2(point.y - origin.y, point.x - origin.x),
      selected: [...state.selected], started: false, z: view.z,
      toggleOff: shift && hit.kind === 'body' && before.selected.includes(hit.node),
      contenders: pick.candidates.map(n => ({id: n.id, occluded: Boolean(n.occluded)}))};
  }
  function move(state, drag, point, modifiers = {}) {
    if (!drag) return;
    drag.current = {...point};
    if (!drag.started && G.distance(point, drag.start) <= state.config.dragThreshold) return;
    drag.started = true;
    if (['resize', 'crop', 'fillet', 'rotate'].includes(drag.hit.kind)) state.heldTarget = {id: drag.hit.id, point: {...point}};
    const dx = (point.x - drag.start.x) / drag.z, dy = (point.y - drag.start.y) / drag.z;
    const hit = drag.hit, original = drag.before.nodes.find(n => n.id === hit.node);
    if (!original) return;
    const a = -(original.rotation || 0) * Math.PI / 180;
    const localX = dx * Math.cos(a) - dy * Math.sin(a), localY = dx * Math.sin(a) + dy * Math.cos(a);
    if (hit.kind === 'body') {
      if (state.mode === 'crop') state.nodes = state.nodes.map(n => n.id === hit.node ?
        {...original, panX: (original.panX || 0) + localX, panY: (original.panY || 0) + localY} : n);
      else state.nodes = state.nodes.map(n => {
        const old = drag.before.nodes.find(item => item.id === n.id);
        return drag.selected.includes(n.id) ? {...old, x: old.x + dx, y: old.y + dy} : n;
      });
    } else if (['resize', 'crop'].includes(hit.kind)) {
      const resized = G.resize(original, hit.handle, dx, dy);
      state.nodes = state.nodes.map(n => n.id === hit.node ? resized : n);
      if (hit.detached) {
        const sx = hit.handle.includes('w') ? -1 : hit.handle.includes('e') ? 1 : 0;
        const sy = hit.handle.includes('n') ? -1 : hit.handle.includes('s') ? 1 : 0;
        const screenDelta = G.rotatePoint({x: sx * (resized.w - original.w) * drag.z,
          y: sy * (resized.h - original.h) * drag.z}, {x: 0, y: 0}, original.rotation || 0);
        state.heldControl = {node: hit.node, rect: G.resize({...drag.control, rotation: original.rotation || 0}, hit.handle, screenDelta.x, screenDelta.y)};
      }
    } else if (hit.kind === 'rotate') {
      const bearing = Math.atan2(point.y - drag.origin.y, point.x - drag.origin.x);
      let rotation = (original.rotation || 0) + (bearing - drag.startAngle) * 180 / Math.PI;
      const nearest = Math.round(rotation / 45) * 45;
      if (!modifiers.alt && Math.abs(rotation - nearest) <= 4) rotation = nearest;
      rotation = ((rotation + 180) % 360 + 360) % 360 - 180;
      state.nodes = state.nodes.map(n => n.id === hit.node ? {...original, rotation} : n);
    } else if (hit.kind === 'fillet' || (hit.kind === 'popup' && hit.action === 'Corners')) {
      const maximum = Math.min(original.w, original.h) / 2;
      const delta = hit.detached ? localX * drag.z / G.tiny.filletTravel * maximum : localX;
      let radius = Math.max(0, Math.min(maximum, (original.radius || 0) + delta));
      if (modifiers.shift) radius = Math.min(maximum, Math.round(radius));
      state.nodes = state.nodes.map(n => n.id === hit.node ? {...original,
        radius} : n);
      if (!hit.detached && hit.kind === 'fillet') state.heldFillet = {node: hit.node, travel: G.filletTravel(original) + localX};
    }
  }
  function finish(state, view, drag) {
    if (!drag) return null;
    const hit = drag.hit;
    delete state.heldFillet;
    delete state.heldControl;
    delete state.heldTarget;
    if (!drag.started && drag.toggleOff) state.selected = state.selected.filter(id => id !== hit.node);
    if (drag.started && hit.kind === 'wire') {
      const drop = G.inspect(state, view, drag.current), explicitPort = drop.winner?.kind === 'wire' ? drop.winner.node : null;
      const end = drop.layout.cards.slice().reverse().find(card => card.node.id !== hit.node &&
        (explicitPort ? card.node.id === explicitPort : G.bodyContains(card.node, card.r, drag.current, view.z)));
      if (hit.side === 'in' && end) state.wires.push({from: end.node.id, to: hit.node});
      else state.wires.push({from: hit.node, fromSide: hit.side, to: end?.node.id || null,
        end: {x: (view.cx || 0) + (drag.current.x - view.w / 2) / view.z, y: (view.cy || 0) + (drag.current.y - view.h / 2) / view.z}});
    }
    if (!drag.started && hit.kind === 'toolbar') state.popup = state.popup === hit.action ? null : hit.action;
    if (!drag.started && hit.kind === 'align' && state.selected.length > 1) {
      const nodes = state.nodes.filter(n => state.selected.includes(n.id));
      const bounds = new Map(nodes.map(n => [n.id, G.boundsOf(G.oriented(n, n, G.center(n)))]));
      const left = Math.min(...[...bounds.values()].map(r => r.x)), right = Math.max(...[...bounds.values()].map(r => r.x + r.w));
      state.nodes = state.nodes.map(n => {
        const b = bounds.get(n.id); if (!b) return n;
        const delta = hit.action === 'Left' ? left - b.x : hit.action === 'Right' ? right - b.x - b.w : (left + right) / 2 - b.x - b.w / 2;
        return {...n, x: n.x + delta};
      });
    }
    const after = snapshot(state);
    return {before: drag.before, after, changed: changed(drag.before, after),
      hit: drag.hit, start: drag.start, end: drag.current, started: drag.started,
      contenders: drag.contenders, mode: state.mode};
  }
  function cancel(state, drag) {
    if (!drag) return; restore(state, drag.before); state.popup = drag.popup; delete state.heldFillet; delete state.heldControl; delete state.heldTarget;
  }
  const api = {snapshot, restore, changed, begin, move, finish, cancel};
  if (typeof module !== 'undefined') module.exports = api;
  else root.HitEngine = api;
})(typeof window === 'undefined' ? globalThis : window);
