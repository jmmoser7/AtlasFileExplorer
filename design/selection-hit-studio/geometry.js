/* Sandbox-only approximation. Native owners and limits: source-baseline.json.
   Never import this model into Slate; refine the existing Rust owners later. */
(function (root) {
  'use strict';
  const clone = value => JSON.parse(JSON.stringify(value));
  const contains = (r, p) => p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;
  const distance = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);
  const points = r => [
    ['nw', r.x, r.y], ['n', r.x + r.w / 2, r.y], ['ne', r.x + r.w, r.y],
    ['e', r.x + r.w, r.y + r.h / 2], ['se', r.x + r.w, r.y + r.h],
    ['s', r.x + r.w / 2, r.y + r.h], ['sw', r.x, r.y + r.h], ['w', r.x, r.y + r.h / 2]
  ];
  const hitSize = (n, z, c) => Math.max(n * z, c.hitSlop);
  const tiny = {threshold: 24, cage: 72, body: 18, spacing: 24, filletTravel: 48};
  function rotatePoint(p, origin, degrees) {
    const a = degrees * Math.PI / 180, c = Math.cos(a), s = Math.sin(a), x = p.x - origin.x, y = p.y - origin.y;
    return {x: origin.x + x * c - y * s, y: origin.y + x * s + y * c};
  }
  const center = r => ({x: r.x + r.w / 2, y: r.y + r.h / 2});
  const oriented = (r, node, origin) => ({...r, angle: node.rotation || 0, origin});
  const localPoint = (p, shape) => shape.angle ? rotatePoint(p, shape.origin || center(shape), -shape.angle) : p;
  const shapeCenter = shape => shape.radius !== undefined ? {x: shape.x, y: shape.y} :
    rotatePoint(center(shape), shape.origin || center(shape), shape.angle || 0);
  const rectPoints = r => [points(r)[0], points(r)[2], points(r)[4], points(r)[6]].map(([, x, y]) =>
    rotatePoint({x, y}, r.origin || center(r), r.angle || 0));
  const boundsOf = r => union(rectPoints(r).map(p => ({...p, w: 0, h: 0})));
  const filletTravel = node => Math.max(Math.min(node.radius || 0, Math.min(node.w, node.h) / 2), Math.min(10, Math.min(node.w, node.h) / 2));
  function screenRect(node, view) {
    return {x: view.w / 2 + (node.x - (view.cx || 0)) * view.z, y: view.h / 2 + (node.y - (view.cy || 0)) * view.z,
      w: node.w * view.z, h: node.h * view.z};
  }
  function wirePoint(node, side, view, strategy, config) {
    const r = screenRect(node, view), height = ['semantic', 'adaptive'].includes(strategy) ? r.h * 0.25 : config.wireTop * view.z;
    return rotatePoint({x: side === 'in' ? r.x : r.x + r.w,
      y: side === 'out' || node.kind === 'generator' ? r.y + height : r.y + r.h / 2}, center(r), node.rotation || 0);
  }
  function union(rects) {
    if (!rects.length) return null;
    const x = Math.min(...rects.map(r => r.x)), y = Math.min(...rects.map(r => r.y));
    return {x, y, w: Math.max(...rects.map(r => r.x + r.w)) - x,
      h: Math.max(...rects.map(r => r.y + r.h)) - y};
  }
  function bodyContains(node, r, p, z) {
    p = localPoint(p, oriented(r, node, center(r)));
    if (!contains(r, p)) return false;
    const radius = Math.min((node.radius || 0) * z, r.w / 2, r.h / 2);
    const x = Math.max(r.x + radius, Math.min(p.x, r.x + r.w - radius));
    const y = Math.max(r.y + radius, Math.min(p.y, r.y + r.h - radius));
    return Math.hypot(p.x - x, p.y - y) <= radius + 0.001;
  }
  function bodyDistance(card, p, z) {
    const q = rotatePoint(p, card.origin, -(card.node.rotation || 0)), r = card.r;
    const radius = Math.max(0, Math.min((card.node.radius || 0) * z, r.w / 2, r.h / 2));
    const x = Math.abs(q.x - card.origin.x) - r.w / 2 + radius;
    const y = Math.abs(q.y - card.origin.y) - r.h / 2 + radius;
    return Math.hypot(Math.max(0, x), Math.max(0, y)) + Math.min(Math.max(x, y), 0) - radius;
  }
  function bodyOutline(card, z) {
    const r = card.r, radius = Math.max(0, Math.min((card.node.radius || 0) * z, r.w / 2, r.h / 2));
    if (!radius) return rectPoints(oriented(r, card.node, card.origin));
    const outline = [];
    for (const [x, y, start] of [[r.x + r.w - radius,r.y + radius,-90], [r.x + r.w - radius,r.y + r.h - radius,0],
      [r.x + radius,r.y + r.h - radius,90], [r.x + radius,r.y + radius,180]])
      for (let i = 0; i <= 8; i++) {
        const a = (start + i * 90 / 8) * Math.PI / 180;
        outline.push(rotatePoint({x:x + Math.cos(a) * radius,y:y + Math.sin(a) * radius},card.origin,card.node.rotation || 0));
      }
    return outline;
  }
  function bodiesOverlap(a, b, z) {
    const first = bodyOutline(a, z), second = bodyOutline(b, z);
    // Strict positive area: touching edges/corners are adjacency, not stacking.
    for (const polygon of [first, second]) for (let i = 0; i < polygon.length; i++) {
      const p = polygon[i], q = polygon[(i + 1) % polygon.length], axis = {x:q.y-p.y,y:p.x-q.x};
      const length = Math.hypot(axis.x,axis.y); if (length < 1e-10) continue;
      const project = points => points.map(point => (point.x*axis.x+point.y*axis.y)/length);
      const x = project(first), y = project(second);
      if (Math.min(Math.max(...x),Math.max(...y))-Math.max(Math.min(...x),Math.min(...y)) <= 1e-7) return false;
    }
    return true;
  }
  function build(state, view) {
    const c = state.config, z = view.z, zones = [], cards = [], chrome = [];
    const zone = (node, kind, shape, data = {}) => {
      const item = {id: node.id + ':' + kind + ':' + (data.handle || data.action || ''),
        node: node.id, kind, shape, ...data};
      zones.push(item); return item;
    };
    for (const node of state.nodes) {
      const r = screenRect(node, view), selected = state.selected.includes(node.id), origin = center(r);
      const expanded = state.tinyStrategy === 'expanded' && selected && (Math.min(r.w, r.h) <= tiny.threshold || state.heldControl?.node === node.id);
      const cage = Math.max(tiny.cage, 6 * Math.max(hitSize(c.cornerHit, z, c), hitSize(c.edgeHit, z, c)));
      const width = Math.max(cage, r.w + tiny.spacing * 2), height = Math.max(cage, r.h + tiny.spacing * 2);
      const control = expanded ? state.heldControl?.node === node.id ? state.heldControl.rect :
        {x: origin.x - width / 2, y: origin.y - height / 2, w: width, h: height} : r;
      const body = state.tinyStrategy === 'expanded' && Math.min(r.w, r.h) <= tiny.threshold ?
        {x: origin.x - Math.max(tiny.body, r.w) / 2, y: origin.y - Math.max(tiny.body, r.h) / 2, w: Math.max(tiny.body, r.w), h: Math.max(tiny.body, r.h)} : r;
      const card = {node, r, selected, control, expanded, origin}; cards.push(card);
      zone(node, 'body', oriented(body, node, origin), {expandedBody: body !== r, detached: body !== r});
      if (state.mode === 'crop' && !selected) continue;
      const crop = state.mode === 'crop' && selected;
      const cropScale = expanded ? 1 : Math.min(1, Math.min(r.w, r.h) / (4 * hitSize(c.cropArm + c.cropPad, z, c)));
      const corner = crop ? hitSize(c.cropArm + c.cropPad, z, c) * cropScale : hitSize(c.cornerHit, z, c);
      const edge = crop ? hitSize(c.cropBar / 2 + c.cropPad, z, c) * cropScale : hitSize(c.edgeHit, z, c);
      for (const [handle, x, y] of points(control)) {
        const p = rotatePoint({x, y}, origin, node.rotation || 0);
        if (handle.length === 2 || (expanded && !crop)) zone(node, crop ? 'crop' : 'resize', {...p, radius: handle.length === 2 ? corner : edge}, {handle, corner: handle.length === 2, detached: expanded});
      }
      const side = crop ? control : r;
      if (!expanded || crop) for (const [handle, x, y, w, h] of [
        ['n', side.x, side.y - edge, side.w, edge * 2], ['e', side.x + side.w - edge, side.y, edge * 2, side.h],
        ['s', side.x, side.y + side.h - edge, side.w, edge * 2], ['w', side.x - edge, side.y, edge * 2, side.h]
      ]) zone(node, crop ? 'crop' : 'resize', oriented({x, y, w, h}, node, origin), {handle, corner: false, detached: expanded});
      if (state.mode === 'crop') continue;
      if (node.kind !== 'generator') for (const [handle, x, y] of points(control).filter(([h]) => h.length === 2)) {
        const p = rotatePoint({x, y}, origin, node.rotation || 0), length = distance(p, origin);
        const offset = expanded ? Math.max(tiny.spacing, (c.rotateOffset || 18) * z, corner + hitSize(c.rotateHit || 12, z, c) + 4) : (c.rotateOffset || 18) * z;
        zone(node, 'rotate', {x: p.x + (p.x - origin.x) / length * offset,
          y: p.y + (p.y - origin.y) / length * offset, radius: hitSize(c.rotateHit || 12, z, c)}, {handle, detached: expanded});
      }
      if (selected && state.showChrome && !state.popup && (expanded || 4 * z >= 1.5)) {
        const travel = state.heldFillet?.node === node.id ? state.heldFillet.travel : filletTravel(node);
        const native = rotatePoint({x: r.x + travel * z, y: r.y}, origin, node.rotation || 0);
        const grip = expanded ? rotatePoint({x: control.x - tiny.spacing, y: origin.y}, origin, node.rotation || 0) : native;
        const half = Math.max(4 * z + c.hitSlop / 2, c.hitSlop);
        zone(node, 'fillet', {x: grip.x - half, y: grip.y - half, w: half * 2, h: half * 2}, {grip, native, detached: expanded});
      }
      const inner = z >= 1 ? c.wireInner : Math.min(14, c.wireInner * Math.pow(z, -0.35));
      const eligible = Math.min(r.w, r.h) > 4 * inner && node.h >= c.wireTop + 14 && 3 * z >= 1.5;
      if (state.showChrome && eligible) {
        for (const side of ['in', 'out']) {
          const x = side === 'in' ? r.x : r.x + r.w;
          const y = side === 'out' || node.kind === 'generator' ? r.y + c.wireTop * z : r.y + r.h / 2;
          const a = (node.rotation || 0) * Math.PI / 180, sign = side === 'in' ? -1 : 1;
          zone(node, 'wire', {...rotatePoint({x, y}, origin, node.rotation || 0), radius: inner},
            {side, inner, outer: inner * c.wireOuterFactor, outward: sign, direction: {x: Math.cos(a) * sign, y: Math.sin(a) * sign}});
        }
      }
    }
    const selected = cards.filter(n => n.selected), bounds = union(selected.map(card => {
      let b = boundsOf(oriented(card.control, card.node, card.origin));
      if (card.expanded) b = union([b, ...zones.filter(item => item.node === card.node.id && item.detached).map(item =>
        item.shape.radius !== undefined ? {x: item.shape.x - item.shape.radius, y: item.shape.y - item.shape.radius, w: item.shape.radius * 2, h: item.shape.radius * 2} : item.shape)]);
      return b;
    }));
    if (bounds && state.showChrome && state.mode !== 'crop') {
      const host = selected[0].node, count = selected.length === 1 && host.kind !== 'shape' ? 7 : 3;
      const natural = (count * 48 + 2 * c.blisterShoulder) * z;
      const blister = selected.length === 1 && host.kind !== 'shape' && natural <= bounds.w &&
        natural * c.blisterHeight * z <= bounds.w * bounds.h * 0.2;
      const captions = count === 7 ? ['Fill', 'Stroke', 'Corners', 'Filter', 'Brush', 'Layers', 'Agent'] : ['Fill', 'Stroke', 'Corners'];
      const width = blister ? natural : (count * c.buttonSize + (count - 1) * c.buttonGap) * z;
      const strip = {x: bounds.x + bounds.w / 2 - width / 2,
        y: blister ? bounds.y + bounds.h - 20 * z : bounds.y + bounds.h + c.stripGap * z,
        w: width, h: (blister ? c.blisterHeight : c.buttonSize) * z};
      chrome.push({kind: 'strip', r: strip, blister});
      for (let i = 0; i < count; i++) {
        const size = blister ? Math.min(31.2 * z, ((width - 2 * 84 * z) / count) * 0.82) : c.buttonSize * z;
        const x = blister ? strip.x + 84 * z + (i + 0.5) * (width - 168 * z) / count - size / 2 : strip.x + i * (c.buttonSize + c.buttonGap) * z;
        zone(host, 'toolbar', {x, y: strip.y + (strip.h - size) / 2, w: size, h: size}, {action: captions[i]});
      }
      if (!state.popup) {
        zone(host, 'dimension', {x: bounds.x + bounds.w / 2 - 27 * z, y: bounds.y + bounds.h + c.bottomGap * z - 9 * z, w: 54 * z, h: 18 * z}, {action: 'Width'});
        if (selected.length > 1) {
          const y = bounds.y + bounds.h + Math.max(20, c.bottomGap * z + 7 + 10 * z) + 16;
          for (const [i, action] of ['Left', 'Center', 'Right'].entries()) zone(host, 'align',
            {x: bounds.x + bounds.w / 2 + (i - 1) * 16 - 9, y: y - 9, w: 18, h: 18}, {action});
        }
      }
      if (state.popup) zone(host, 'popup', {x: strip.x, y: strip.y + strip.h + 7 * z, w: 240 * z, h: 74 * z}, {action: state.popup});
    }
    const layout = {cards, zones, chrome, bounds};
    if (['semantic', 'adaptive'].includes(state.tinyStrategy)) {
      const S = typeof module !== 'undefined' ? require('./semantic.js') : root.HitSemantic;
      return S.apply(layout, state, view, api);
    }
    return layout;
  }
  function zoneContains(zone, p) {
    if (zone.guard && distance(zone.guard, p) <= zone.guard.radius) return true;
    if (zone.bodyRect) return contains(zone.bodyRect, localPoint(p, zone.bodyRect));
    if (zone.kind === 'wire') {
      const d = distance(zone.shape, p);
      const dir = zone.direction || {x: zone.outward, y: 0};
      return d <= zone.inner || (d <= zone.outer && (p.x - zone.shape.x) * dir.x + (p.y - zone.shape.y) * dir.y >= 0);
    }
    return zone.shape.radius !== undefined ? distance(zone.shape, p) <= zone.shape.radius : contains(zone.shape, localPoint(p, zone.shape));
  }
  function inspect(state, view, p) {
    const layout = build(state, view);
    if (state.tinyStrategy === 'adaptive') {
      const A = typeof module !== 'undefined' ? require('./adaptive.js') : root.HitAdaptive;
      return A.inspect(layout, state, view, p, api);
    }
    const all = layout.zones.filter(zone => zoneContains(zone, p) && (zone.kind !== 'body' ||
      (zone.bodyRect || zone.expandedBody || bodyContains(state.nodes.find(n => n.id === zone.node), zone.shape, p, view.z))));
    const topBody = [...layout.cards].reverse().find(card => bodyContains(card.node, card.r, p, view.z));
    const candidate = all.map(zone => ({...zone, occluded: Boolean(!(zone.guard && distance(zone.guard, p) <= zone.guard.radius) && topBody && topBody.node.id !== zone.node &&
      (zone.detached || (!['popup', 'toolbar', 'dimension', 'align'].includes(zone.kind) &&
      state.nodes.findIndex(n => n.id === zone.node) < state.nodes.findIndex(n => n.id === topBody.node.id))))}));
    let winner = candidate.find(n => n.kind === 'popup');
    if (!winner && state.tinyStrategy === 'semantic') winner = candidate.find(n => n.guard && distance(n.guard, p) <= n.guard.radius);
    if (!winner && state.mode === 'crop') {
      const id = state.selected[0];
      winner = candidate.filter(n => n.node === id && n.kind === 'crop').sort((a, b) => Number(b.corner) - Number(a.corner) || distance(a.shape, p) - distance(b.shape, p))[0];
      if (!winner && (!topBody || topBody.node.id === id)) winner = candidate.find(n => n.node === id && n.kind === 'body' && !n.occluded);
    }
    if (!winner && state.mode !== 'crop') {
      winner = candidate.find(n => ['toolbar', 'dimension', 'align'].includes(n.kind));
      if (!winner) for (const card of [...layout.cards].reverse()) {
        const choices = candidate.filter(n => n.node === card.node.id && !n.occluded);
        const body = choices.find(n => n.kind === 'body');
        if (body && (card.expanded || Math.min(card.r.w, card.r.h) <= 2 * hitSize(state.config.cornerHit, view.z, state.config))) { winner = body; break; }
        winner = choices.find(n => n.kind === 'wire' && distance(n.shape, p) <= n.inner) ||
          choices.find(n => n.kind === 'fillet') || choices.find(n => n.kind === 'resize' && n.corner) ||
          choices.find(n => n.kind === 'resize' && !n.corner) || choices.find(n => n.kind === 'rotate') || choices.find(n => n.kind === 'wire') || body;
        if (winner) break;
      }
    }
    return {winner: winner || null, candidates: candidate, layout};
  }
  function preset(name) {
    const a = {id: 'A', kind: 'image', x: -160, y: -140, w: 320, h: 240, radius: 0};
    let nodes = [a], selected = ['A'], mode = 'select';
    if (name === 'overlap') nodes.push({id: 'B', kind: 'image', x: 25, y: -65, w: 230, h: 175, radius: 0});
    if (name === 'adjacent') nodes = [{...a,label:'Left',x:-180,y:-60,w:180,h:120},
      {...a,id:'B',label:'Right',x:0,y:-60,w:180,h:120}];
    if (name === 'tiny') { nodes = [{...a, x: -110, y: -20, w: 24, h: 18}, {...a, id: 'B', x: 5, y: 10, w: 44, h: 44},
      {...a, id: 'C', x: 115, y: 10, w: 8, h: 6}, {...a, id: 'D', x: 195, y: 10, w: 4, h: 3}, {...a, id: 'E', x: -200, y: 10, w: 1, h: 1}]; }
    if (name === 'crop') { nodes = [{...a, x: -280, y: -190, w: 560, h: 320}]; mode = 'crop'; }
    if (name === 'wire') nodes.push({...a, id: 'B', kind: 'generator', x: 170, y: -125, w: 220, h: 195});
    if (name === 'crowded') { nodes = [a, {...a, id: 'B', x: -90, y: 105, w: 180, h: 90}, {...a, id: 'C', x: 80, y: 120, w: 115, h: 80}]; selected = ['A', 'B']; }
    if (name === 'scale') {
      nodes = [{...a, label: 'Normal', x: -120, y: -200, w: 240, h: 112},
        {...a, id: 'B', label: 'Small', x: -48, y: -35, w: 96, h: 48},
        {...a, id: 'C', label: 'Extra small', x: -12, y: 65, w: 24, h: 18},
        {...a, id: 'D', label: 'Point', x: -0.5, y: 135, w: 1, h: 1}]; selected = ['A', 'B', 'C', 'D'];
    }
    if (name === 'overlap-lab') {
      nodes = [{...a, label: 'Rear', x: -200, y: -180, w: 240, h: 140},
        {...a, id: 'B', label: 'Front', x: -80, y: -140, w: 220, h: 150},
        {...a, id: 'C', label: 'Tiny rear', x: -90, y: 80, w: 24, h: 18},
        {...a, id: 'D', label: 'Tiny front', x: -70, y: 87, w: 24, h: 18},
        {...a, id: 'E', label: 'Buried', x: 75, y: 65, w: 70, h: 55},
        {...a, id: 'F', label: 'Coincident front', x: 75, y: 65, w: 70, h: 55}];
    }
    return {nodes, selected, mode, wires: [], popup: null};
  }
  function resize(node, handle, dx, dy) {
    const result = {...node};
    const a = -(node.rotation || 0) * Math.PI / 180, localX = dx * Math.cos(a) - dy * Math.sin(a), localY = dx * Math.sin(a) + dy * Math.cos(a);
    const sx = handle.includes('e') ? 1 : handle.includes('w') ? -1 : 0, sy = handle.includes('s') ? 1 : handle.includes('n') ? -1 : 0;
    const dw = sx ? Math.max(1 - node.w, localX * sx) : 0, dh = sy ? Math.max(1 - node.h, localY * sy) : 0;
    const shift = rotatePoint({x: sx * dw / 2, y: sy * dh / 2}, {x: 0, y: 0}, node.rotation || 0);
    result.w += dw; result.h += dh;
    result.x += shift.x - dw / 2; result.y += shift.y - dh / 2;
    return result;
  }
  const api = {clone, contains, distance, points, hitSize, screenRect, union, build, inspect, preset, resize,
    tiny, rotatePoint, center, oriented, shapeCenter, rectPoints, boundsOf, filletTravel, bodyContains, wirePoint, localPoint,
    bodyDistance, bodyOutline, bodiesOverlap};
  if (typeof module !== 'undefined') module.exports = api;
  else root.HitGeometry = api;
})(typeof window === 'undefined' ? globalThis : window);
