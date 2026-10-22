/* Proposed semantic zoom policy. Source and expanded profiles remain in geometry.js. */
(function (root) {
  'use strict';
  const policy = {wire: [36, 88], resize: [12, 48], rotate: [3, 16], minimumHit: 2, moveSize: 18,
    gap: 3, viewportInset: 10, viewportBottom: 34, zoomMin: 1 / 128, zoomMax: 128, wheelRate: 0.0015};
  const smooth = (low, high, value) => { const t = Math.max(0, Math.min(1, (value - low) / (high - low))); return t * t * (3 - 2 * t); };
  const domains = size => ({wire: smooth(...policy.wire, size), resize: smooth(...policy.resize, size), rotate: smooth(...policy.rotate, size), move: 1});
  const worldAt = (p, v) => ({x: (v.cx || 0) + (p.x - v.w / 2) / v.z, y: (v.cy || 0) + (p.y - v.h / 2) / v.z});
  function zoomAt(view, zoom, screen, world = worldAt(screen, view)) {
    const z = Math.max(policy.zoomMin, Math.min(policy.zoomMax, zoom));
    return {...view, z, cx: world.x - (screen.x - view.w / 2) / z, cy: world.y - (screen.y - view.h / 2) / z};
  }
  function free(point, radius, occupied, rects, view, G) {
    const pad = policy.viewportInset + radius;
    return point.x >= pad && point.y >= pad && point.x <= view.w - pad && point.y <= view.h - policy.viewportBottom - radius &&
      !occupied.some(p => (point.x - p.x) ** 2 + (point.y - p.y) ** 2 < (radius + p.radius + policy.gap) ** 2) &&
      !rects.some(r => point.x > r.x - radius - 2 && point.x < r.x + r.w + radius + 2 && point.y > r.y - radius - 2 && point.y < r.y + r.h + radius + 2);
  }
  function place(desired, radius, occupied, blockers, bodies, view, G) {
    const clamp = p => ({x: Math.max(policy.viewportInset + radius, Math.min(view.w - policy.viewportInset - radius, p.x)),
      y: Math.max(policy.viewportInset + radius, Math.min(view.h - policy.viewportBottom - radius, p.y))});
    const start = clamp(desired);
    if (free(start, radius, occupied, [...blockers, ...bodies], view, G)) return start;
    // Find nearby air first; the viewport fallback remains a visible overlay.
    for (let ring = 1; ring <= 12; ring++) for (let i = 0; i < 16; i++) {
      const angle = i * Math.PI / 8, p = clamp({x: start.x + Math.cos(angle) * ring * 14, y: start.y + Math.sin(angle) * ring * 14});
      if (free(p, radius, occupied, [...blockers, ...bodies], view, G)) return p;
    }
    const step = Math.max(8, radius * 2 + policy.gap);
    for (let y = policy.viewportInset + radius; y < view.h - policy.viewportBottom - radius; y += step)
      for (let x = policy.viewportInset + radius; x < view.w - policy.viewportInset - radius; x += step)
        if (free({x, y}, radius, occupied, blockers, view, G)) return {x, y};
    return null;
  }
  let cachedKey = '', cachedLayout;
  function reserveGrid(zones, blockers, view, G) {
    const slots = [], radius = 5, step = radius * 2 + policy.gap;
    for (let y = policy.viewportInset + radius; y <= view.h - policy.viewportBottom - radius; y += step)
      for (let x = policy.viewportInset + radius; x <= view.w - policy.viewportInset - radius; x += step)
        if (free({x, y}, radius, [], blockers, view, G)) slots.push({x, y});
    zones.forEach((zone, i) => {
      const p = slots[i]; if (!p) return;
      zone.guard = {...p, radius: Math.min(radius, zone.shape.radius)}; zone.displaced = true; delete zone.unplaced;
      if (zone.kind !== 'body') zone.shape = {...zone.shape, ...p};
    });
  }
  function apply(layout, state, view, G) {
    const key = JSON.stringify([state.tinyStrategy, state.nodes, state.selected, state.config, state.mode, state.popup, state.showChrome, state.heldTarget, state.heldControl, view]);
    if (key === cachedKey) return cachedLayout;
    const zones = [], occupied = [], blockers = [], cards = layout.cards, dormant = [], ui = [];
    const baseUi = layout.zones.filter(n => ['toolbar', 'dimension', 'align'].includes(n.kind));
    for (const card of cards) {
      const b = G.boundsOf(G.oriented(card.r, card.node, card.origin));
      card.visible = b.x + b.w >= 0 && b.x <= view.w && b.y + b.h >= 0 && b.y <= view.h;
    }
    const maximum = Math.max(0, ...cards.filter(n => n.selected && n.visible).map(n => domains(Math.min(n.r.w, n.r.h)).resize));
    // A compact screen-space rail keeps property controls inside the viewport at extreme zoom.
    if (state.showChrome && !state.heldTarget && maximum > 0.12) {
      const size = 26, gap = 5, width = baseUi.length * (size + gap) - gap;
      const start = Math.max(10, Math.min(view.w - width - 10, layout.bounds ? layout.bounds.x + layout.bounds.w / 2 - width / 2 : view.w / 2 - width / 2));
      baseUi.forEach((item, i) => {
        const shape = {x: start + i * (size + gap), y: view.h - 59, w: size, h: size};
        const zone = {...item, shape, opacity: maximum, semantic: true, guard: {...G.center(shape), radius: 4}};
        ui.push(zone); blockers.push(shape);
      });
    }
    if (state.popup && ui.length) {
      const source = layout.zones.find(n => n.kind === 'popup'), host = ui.find(n => n.action === state.popup) || ui[0];
      if (source) {
        const shape = {x: Math.max(10, Math.min(view.w - 250, host.shape.x)), y: view.h - 144, w: 240, h: 74};
        ui.push({...source, shape, semantic: true, guard: {...G.center(shape), radius: 4}}); blockers.push(shape);
      }
    }
    const candidates = [];
    function add(card, kind, p, radius, weight, data = {}) {
      const zone = {id: `${card.node.id}:${kind}:${data.handle || data.side || ''}`, node: card.node.id, kind,
        shape: {...p, radius}, opacity: weight, semantic: true, anchor: {...p}, ...data};
      if (state.heldTarget?.id === zone.id) { zone.locked = true; zone.shape.radius = Math.max(3, radius); zone.opacity = Math.max(0.3, weight); }
      if (!zone.locked && (radius < policy.minimumHit || weight <= 0)) { dormant.push(zone); return; }
      candidates.push(zone);
    }
    for (const card of cards) {
      const {node, r, origin} = card, size = Math.min(r.w, r.h), d = domains(size);
      card.domains = card.visible ? d : {wire: 0, resize: 0, rotate: 0, move: 0}; card.projectedSize = size;
      if (!card.visible) continue;
      card.expanded = size < 48; card.semantic = true;
      const c = state.config, resizeRadius = c.cornerHit * d.resize;
      const expansion = (1 - smooth(24, 72, size)) * 22 * d.resize;
      card.control = {x: r.x - expansion, y: r.y - expansion, w: r.w + expansion * 2, h: r.h + expansion * 2};
      add(card, 'body', origin, policy.moveSize / 2, 1, {bodyRect: G.oriented({x: origin.x - Math.max(policy.moveSize, r.w) / 2,
        y: origin.y - Math.max(policy.moveSize, r.h) / 2, w: Math.max(policy.moveSize, r.w), h: Math.max(policy.moveSize, r.h)}, node, origin)});
      if (!state.showChrome || state.popup) continue;
      const rotate = p => G.rotatePoint(p, origin, node.rotation || 0);
      for (const [handle, x, y] of G.points(card.control)) {
        add(card, state.mode === 'crop' ? 'crop' : 'resize', rotate({x, y}),
          handle.length === 2 ? resizeRadius : c.edgeHit * d.resize,
          d.resize, {handle, corner: handle.length === 2, detached: true});
        if (handle.length === 2 && state.mode !== 'crop' && node.kind !== 'generator') {
          const p = rotate({x, y}), length = Math.max(0.001, G.distance(p, origin));
          const offset = (Math.max(4, c.rotateOffset * 2 / 3) + c.rotateOffset * d.rotate) * d.rotate;
          add(card, 'rotate', {x: p.x + (p.x - origin.x) / length * offset, y: p.y + (p.y - origin.y) / length * offset},
            c.rotateHit * d.rotate, d.rotate, {handle, detached: true});
        }
      }
      if (state.mode === 'crop') continue;
      const native = rotate({x: r.x + G.filletTravel(node) * view.z, y: r.y});
      add(card, 'fillet', native, 9 * d.resize, d.resize, {native, detached: true});
      for (const side of ['in', 'out']) add(card, 'wire', G.wirePoint(node, side, view, 'semantic', c),
        c.wireInner * d.wire, d.wire, {side, inner: c.wireInner * d.wire, outer: c.wireInner * d.wire, outward: side === 'in' ? -1 : 1});
    }
    if (state.tinyStrategy === 'adaptive') {
      const A = typeof module !== 'undefined' ? require('./adaptive.js') : root.HitAdaptive;
      cachedKey = key; cachedLayout = A.apply({...layout, cards, zones: [...candidates, ...ui], chrome: [], dormant}, state, view, G);
      return cachedLayout;
    }
    // Legacy proposal: move reserves first, then the coarse-to-fine controls.
    const rank = {body: 0, rotate: 1, resize: 2, crop: 2, fillet: 3, wire: 4};
    candidates.sort((a, b) => Number(Boolean(b.locked)) - Number(Boolean(a.locked)) || rank[a.kind] - rank[b.kind] || state.nodes.findIndex(n => n.id === b.node) - state.nodes.findIndex(n => n.id === a.node));
    for (const zone of candidates) {
      const bodies = cards.filter(card => card.visible && card.node.id !== zone.node).map(card => G.boundsOf(G.oriented(card.r, card.node, card.origin)));
      const radius = Math.min(5, zone.shape.radius), desired = zone.locked ? state.heldTarget.point : zone.shape;
      const p = place(desired, radius, occupied, blockers, zone.locked ? [] : bodies, view, G);
      if (!p) { zone.unplaced = true; zones.push(zone); continue; }
      zone.guard = {...p, radius}; occupied.push(zone.guard);
      zone.displaced = G.distance(p, zone.anchor) > 1;
      if (zone.kind !== 'body') zone.shape = {...zone.shape, ...p};
      zones.push(zone);
    }
    // A fixed grid is the capacity fallback. At the supported 330×500 minimum
    // canvas, it has more slots than all 480 possible node targets plus the rail.
    if (zones.some(zone => zone.unplaced)) reserveGrid(zones, blockers, view, G);
    cachedKey = key; cachedLayout = {...layout, cards, zones: [...zones, ...ui], chrome: [], dormant};
    return cachedLayout;
  }
  function audit(state, view, G) {
    const layout = G.build(state, view), failures = [];
    for (const zone of layout.zones) {
      if (!zone.guard) { failures.push({id: zone.id, reason: 'No protected patch'}); continue; }
      const r = zone.guard.radius * 0.5;
      for (const [dx, dy] of [[0,0],[r,0],[-r,0],[0,r],[0,-r],[r/2,r/2],[-r/2,r/2],[r/2,-r/2],[-r/2,-r/2]]) {
        const point = {x: zone.guard.x + dx, y: zone.guard.y + dy}, winner = G.inspect(state, view, point).winner;
        if (point.x < 0 || point.y < 0 || point.x > view.w || point.y > view.h || winner?.id !== zone.id)
          failures.push({id: zone.id, point, winner: winner?.id || 'none'});
      }
    }
    return {active: layout.zones.length, dormant: layout.dormant.length, offscreen: layout.cards.filter(card => !card.visible).length, failures};
  }
  const api = {policy, smooth, domains, worldAt, zoomAt, apply, audit, reserveGrid};
  if (typeof module !== 'undefined') module.exports = api;
  else root.HitSemantic = api;
})(typeof window === 'undefined' ? globalThis : window);
