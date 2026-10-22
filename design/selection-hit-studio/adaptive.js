/* Sandbox proposal: continuous feature fields, cursor expansion, and honest
   silhouette occlusion. Reuses semantic prominence; never relocates targets to dots. */
(function (root) {
  'use strict';
  const S = typeof module !== 'undefined' ? require('./semantic.js') : root.HitSemantic;
  const policy = {cursorRange: 42, cursorGain: 0.65, cage: 72, coverGap: 25, moveCore: 9, sampleStep: 4,
    previewRadius: 66, previewWork: 70000, overlapVersion: 2, scoreTie: 1e-8, coincidentCorner: 0.01};
  const uiKinds = ['toolbar', 'popup', 'dimension', 'align'];
  const transformKinds = ['body', 'resize', 'crop', 'rotate', 'fillet'];
  function segment(p, a, b) {
    const dx = b.x - a.x, dy = b.y - a.y, t = Math.max(0, Math.min(1,
      ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy || 1)));
    return {x: a.x + dx * t, y: a.y + dy * t};
  }
  function nearest(zone, p, G) {
    const f = zone.field;
    if (f.type === 'segment') return segment(p, f.a, f.b);
    if (f.type === 'arc') {
      const angle = Math.atan2(p.y - f.center.y, p.x - f.center.x);
      const delta = Math.atan2(Math.sin(angle - f.bearing), Math.cos(angle - f.bearing));
      const a = f.bearing + Math.max(-Math.PI / 4, Math.min(Math.PI / 4, delta));
      return {x: f.center.x + Math.cos(a) * f.radius, y: f.center.y + Math.sin(a) * f.radius};
    }
    return zone.anchor;
  }
  function reach(zone, cursor, G) {
    if (!zone.field) return 0;
    const distance = cursor ? G.distance(cursor, nearest(zone, cursor, G)) : policy.cursorRange;
    return zone.field.reach * (1 + policy.cursorGain * (1 - S.smooth(0, policy.cursorRange, distance)));
  }
  function silhouette(card, p, view, G) { return G.bodyContains(card.node, card.r, p, view.z); }
  function fullyCovered(card, cards, view, G) {
    const index = cards.indexOf(card), corners = G.rectPoints(G.oriented(card.r, card.node, card.origin));
    return [...corners, card.origin].every(p => cards.slice(index + 1).some(front => silhouette(front, p, view, G)));
  }
  function controlRect(card, cards, state, view, G) {
    const {r, node, origin, domains: d} = card;
    const pad = Math.max(0, (policy.cage - Math.min(r.w, r.h)) / 2) * d.resize;
    let control = {x: r.x - pad, y: r.y - pad, w: r.w + 2 * pad, h: r.h + 2 * pad};
    // An explicitly selected buried object gets one coherent outside frame.
    // Ordinary rear objects never acquire floating handles over a foreground body.
    if (state.selected[0] === node.id && fullyCovered(card, cards, view, G)) {
      const local = p => G.rotatePoint(p, origin, -(node.rotation || 0));
      const own = G.boundsOf(G.oriented(r, node, origin));
      const blockers = cards.slice(cards.indexOf(card) + 1).filter(other => {
        const b = G.boundsOf(G.oriented(other.r, other.node, other.origin));
        return b.x <= own.x + own.w && b.x + b.w >= own.x && b.y <= own.y + own.h && b.y + b.h >= own.y;
      });
      const bounds = G.union([control, ...blockers.map(other => G.union(G.rectPoints(G.oriented(other.r, other.node, other.origin)).map(p => ({...local(p), w: 0, h: 0}))))]);
      control = {x: bounds.x - policy.coverGap, y: bounds.y - policy.coverGap,
        w: bounds.w + policy.coverGap * 2, h: bounds.h + policy.coverGap * 2};
      card.coverFrame = true;
    }
    return control;
  }
  function moveHeader(card, layout, view, G, state) {
    const r = card.control, width = Math.min(130, Math.max(70, r.w - 36)), height = 20;
    const anchors = [0,20,40].flatMap(gap=>[{x:r.x+r.w/2,y:r.y-13-gap},{x:r.x+r.w/2,y:r.y+r.h+13+gap},
      {x:r.x-width/2-3-gap,y:r.y+r.h/2},{x:r.x+r.w+width/2+3+gap,y:r.y+r.h/2}]);
    const blockers = [...layout.cards.filter(other => other.node.id !== card.node.id).map(other => G.boundsOf(G.oriented(other.r, other.node, other.origin))),
      ...layout.zones.filter(zone => uiKinds.includes(zone.kind)).map(zone => zone.shape)];
    const candidates = anchors.map(anchor => {
      const p = G.rotatePoint(anchor, card.origin, card.node.rotation || 0), shape = {x:p.x-width/2,y:p.y-height/2,w:width,h:height};
      const probes = G.points(shape).map(([,x,y])=>({x,y})).concat([p]);
      for(let y=shape.y+3;y<shape.y+height;y+=6) for(let x=shape.x+3;x<shape.x+width;x+=6) probes.push({x,y});
      const clear = probes.filter(point => {
        if (point.x<0 || point.x>view.w || point.y<0 || point.y>view.h-34 || blockers.some(b=>G.contains(b,point))) return false;
        const winner=resolve(layout,state,view,point,G,point,false).winner;
        return !winner || layout.cardMap.get(winner.node).layer<=layout.cardMap.get(card.node.id).layer;
      }).length;
      return {shape, clear:clear/probes.length};
    });
    return candidates.find(item=>item.clear===1)?.shape || candidates.sort((a,b)=>b.clear-a.clear)[0].shape;
  }
  function apply(layout, state, view, G) {
    const zones = layout.zones.filter(zone => uiKinds.includes(zone.kind)).map(zone => { const item = {...zone}; delete item.guard; return item; });
    for (const card of layout.cards) {
      if (!card.visible) continue;
      card.covered = fullyCovered(card, layout.cards, view, G);
      card.adaptive = true; card.control = controlRect(card, layout.cards, state, view, G);
      card.expanded = card.control.w > card.r.w + 1 || card.control.h > card.r.h + 1;
      const rotate = p => G.rotatePoint(p, card.origin, card.node.rotation || 0);
      const handles = new Map(G.points(card.control).map(([h, x, y]) => [h, rotate({x, y})]));
      for (const raw of layout.zones.filter(zone => zone.node === card.node.id && !uiKinds.includes(zone.kind))) {
        const zone = {...raw, adaptive: true, suppressed: card.covered && !card.coverFrame}; delete zone.guard;
        if (zone.kind === 'body') { zone.shape = zone.bodyRect; zones.push(zone); continue; }
        if (state.mode === 'crop' && !card.selected) continue;
        let anchor = handles.get(zone.handle) || zone.anchor;
        let field = {type: 'point', reach: zone.shape.radius};
        if (['resize', 'crop'].includes(zone.kind) && !zone.corner) {
          const ends = {n: ['nw','ne'], e: ['ne','se'], s: ['sw','se'], w: ['nw','sw']}[zone.handle];
          field = {type: 'segment', a: handles.get(ends[0]), b: handles.get(ends[1]), reach: zone.shape.radius};
        } else if (zone.kind === 'rotate') {
          const center = anchor, bearing = Math.atan2(center.y - card.origin.y, center.x - card.origin.x);
          const radius = Math.max(12, state.config.rotateOffset * card.domains.rotate);
          field = {type: 'arc', center, bearing, radius, reach: zone.shape.radius};
          anchor = {x: center.x + Math.cos(bearing) * radius, y: center.y + Math.sin(bearing) * radius};
        } else if (zone.kind === 'fillet') {
          anchor = rotate({x: card.control.x + Math.min(card.control.w / 3, Math.max(10, G.filletTravel(card.node) * view.z)), y: card.control.y});
          zone.detached = card.expanded;
        } else if (zone.kind === 'wire') {
          anchor = card.expanded ? rotate({x: zone.side === 'in' ? card.control.x : card.control.x + card.control.w,
            y: card.control.y + card.control.h * (zone.side === 'in' && card.node.kind !== 'generator' ? 0.5 : 0.25)}) : zone.anchor;
          const direction = G.rotatePoint({x: zone.side === 'in' ? -1 : 1, y: 0}, {x: 0, y: 0}, card.node.rotation || 0);
          field = {type: 'segment', a: anchor, b: {x: anchor.x + direction.x * field.reach * 1.25,
            y: anchor.y + direction.y * field.reach * 1.25}, reach: field.reach};
        }
        zone.anchor = anchor; zone.field = field; zone.shape = {...anchor, radius: field.reach}; zones.push(zone);
      }
    }
    const cardMap = new Map(layout.cards.map((card, i) => [card.node.id, {...card, layer: i, overlaps: new Set()}]));
    const cards = [...cardMap.values()];
    for (let i=0;i<cards.length;i++) for (let j=i+1;j<cards.length;j++) if (G.bodiesOverlap(cards[i],cards[j],view.z)) {
      cards[i].overlaps.add(cards[j].node.id); cards[j].overlaps.add(cards[i].node.id); cards[j].hasUnderlay = true;
    }
    for (const zone of zones.filter(zone=>zone.kind==='wire' && cardMap.get(zone.node).hasUnderlay)) {
      const host=cardMap.get(zone.node), anchor=G.wirePoint(host.node,zone.side,view,'adaptive',state.config);
      const delta={x:zone.field.b.x-zone.field.a.x,y:zone.field.b.y-zone.field.a.y};
      zone.anchor=anchor; zone.shape={...anchor,radius:zone.field.reach};
      zone.field={...zone.field,a:anchor,b:{x:anchor.x+delta.x,y:anchor.y+delta.y}};
    }
    const result={...layout,zones,adaptive:true,cardMap};
    for(const card of layout.cards.filter(card=>card.coverFrame)) {
      const shape=moveHeader(card,result,view,G,state);
      zones.push({id:card.node.id+':body:frame',node:card.node.id,kind:'body',shape,bodyRect:shape,
        frameHeader:true,adaptive:true,opacity:1,anchor:G.shapeCenter(shape)});
    }
    return result;
  }
  function clipCards(layout, zone) {
    const host = layout.cardMap.get(zone.node);
    return [...layout.cardMap.values()].filter(card => card.visible && card.node.id !== zone.node &&
      (card.layer > host.layer || !host.overlaps.has(card.node.id)));
  }
  function wireAllowed(layout, zone, point, view, G) {
    const host = layout.cardMap.get(zone.node);
    return !host.hasUnderlay || (point && G.bodyDistance(host,point,view.z) < -1e-7);
  }
  function prefer(a, b, layout, p, view, G) {
    if (!b) return true;
    if (a.zone.node === b.zone.node) return a.score < b.score || (a.score === b.score && a.zone.id < b.zone.id);
    const corners = a.zone.corner && b.zone.corner && G.distance(a.zone.anchor,b.zone.anchor) <= policy.coincidentCorner;
    if (corners || Math.abs(a.score-b.score) <= policy.scoreTie) {
      const distance = item => Math.max(0,G.bodyDistance(layout.cardMap.get(item.zone.node),p,view.z));
      const difference = distance(a)-distance(b);
      if (Math.abs(difference)>policy.scoreTie) return difference<0;
    }
    return a.score < b.score-policy.scoreTie || (Math.abs(a.score-b.score)<=policy.scoreTie && a.layer>b.layer);
  }
  function resolve(layout, state, view, p, G, cursor = p, collect = true) {
    const actual = [...layout.cards].reverse().find(card => card.visible && silhouette(card, p, view, G));
    const expanded = actual || [...layout.cards].reverse().find(card => card.visible && layout.zones.some(zone =>
      zone.node === card.node.id && zone.kind === 'body' && G.contains(zone.bodyRect, G.localPoint(p, zone.bodyRect))));
    const candidates = [], hosts = []; let ui = null;
    const local = expanded && G.rotatePoint(p, expanded.origin, -(expanded.node.rotation || 0));
    const inMoveCore = expanded && Math.abs(local.x - expanded.origin.x) < Math.min(policy.moveCore, expanded.r.w / 3) &&
      Math.abs(local.y - expanded.origin.y) < Math.min(policy.moveCore, expanded.r.h / 3);
    for (const zone of layout.zones) {
      if (!collect && zone.suppressed) continue;
      let score, inside;
      if (uiKinds.includes(zone.kind)) { inside = G.contains(zone.shape, p); score = zone.kind === 'popup' ? -3 : -2; }
      else if (zone.kind === 'body') { inside = G.contains(zone.bodyRect, G.localPoint(p, zone.bodyRect)); score = zone.frameHeader ? -1 : 1; }
      else {
        const radius = reach(zone, cursor, G), distance = G.distance(p, nearest(zone, p, G)); inside = distance <= radius;
        const edge = ['resize','crop'].includes(zone.kind) && !zone.corner;
        score = edge ? 0.65 + 0.25 * distance / radius : (zone.kind === 'rotate' ? 0.15 : zone.kind === 'wire' ? 0.02 : 0.06) + 0.6 * distance / radius;
      }
      if (!inside) continue;
      const host = layout.cardMap.get(zone.node), layer = host.layer;
      const foreign = expanded && expanded.node.id !== zone.node;
      const foreignBlocks = foreign && (layout.cardMap.get(expanded.node.id).layer > layer || !host.overlaps.has(expanded.node.id));
      const wireBlocked = zone.kind === 'wire' && (!wireAllowed(layout,zone,p,view,G) || !wireAllowed(layout,zone,cursor,view,G));
      const occluded = !uiKinds.includes(zone.kind) && (zone.suppressed || foreignBlocks || wireBlocked ||
        (inMoveCore && !foreign && zone.kind !== 'body'));
      const candidate = collect ? {...zone, score, occluded: Boolean(occluded)} : zone;
      if (collect) candidates.push(candidate);
      if (occluded) continue;
      const entry = {zone:candidate, score, layer};
      if (uiKinds.includes(zone.kind)) { if (!ui || score<ui.score) ui=entry; }
      else if (prefer(entry,hosts[layer],layout,p,view,G)) hosts[layer]=entry;
    }
    // Preserve every foreground transform region. A lower host cannot carve it
    // away with a nearer grip or body. Adjacent hosts use geometry distance.
    const retained = []; let best = null;
    for (let i=hosts.length-1;i>=0;i--) {
      const entry=hosts[i]; if (!entry) continue;
      if (retained.some(front=>transformKinds.includes(front.zone.kind) && layout.cardMap.get(front.zone.node).overlaps.has(entry.zone.node))) {
        if (collect) for (const candidate of candidates) if (candidate.node===entry.zone.node) candidate.occluded=true;
        continue;
      }
      retained.push(entry); if (prefer(entry,best,layout,p,view,G)) best=entry;
    }
    const winner = (ui || best)?.zone || null;
    if (collect) candidates.sort((a,b)=>Number(b.id===winner?.id)-Number(a.id===winner?.id) || Number(a.occluded)-Number(b.occluded) || a.score-b.score);
    return {winner, candidates, layout};
  }
  const inspect = (layout, state, view, p, G) => ({...resolve(layout, state, view, p, G), point: p});
  function audit(state, view, G) {
    const layout = G.build(state, view), counts = new Map(layout.zones.map(zone => [zone.id, 0])), total = counts.size;
    // Sample the actual winning area, not hand-picked reserve centres. Covered
    // targets are reported explicitly, never counted as a reachability pass.
    for (let y = 2; y < view.h - 30; y += policy.sampleStep) for (let x = 2; x < view.w; x += policy.sampleStep) {
      const winner = resolve(layout, state, view, {x, y}, G, {x, y}, false).winner;
      if (winner) counts.set(winner.id, counts.get(winner.id) + policy.sampleStep ** 2);
    }
    return {total, reachable: [...counts.values()].filter(n => n > 0).length,
      hidden: layout.zones.filter(zone => !counts.get(zone.id)).map(zone => ({id: zone.id, kind: zone.kind, node: zone.node})),
      areas: Object.fromEntries(counts), dormant: layout.dormant.length};
  }
  const api = {policy, apply, inspect, resolve, reach, nearest, audit, clipCards, wireAllowed};
  if (typeof module !== 'undefined') module.exports = api; else root.HitAdaptive = api;
})(typeof window === 'undefined' ? globalThis : window);
