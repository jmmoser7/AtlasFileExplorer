/* Representative sandbox paint, not Slate's native chrome interpreter. */
(function () {
  'use strict';
  const G = window.HitGeometry;
  const colors = {body: '#62b49d', resize: '#79a8ee', crop: '#79a8ee', rotate: '#f3a3b5', wire: '#d9ac68',
    fillet: '#79a8ee', toolbar: '#b3a0d4', popup: '#b3a0d4', dimension: '#b3a0d4', align: '#b3a0d4'};
  function box(ctx, r, radius = 0) {
    ctx.beginPath(); ctx.roundRect(r.x, r.y, Math.max(0, r.w), Math.max(0, r.h), Math.max(0, Math.min(radius, r.w / 2, r.h / 2)));
  }
  function text(ctx, value, x, y, color = '#c9cbd0', size = 12, align = 'left') {
    ctx.font = `${size}px Segoe UI, sans-serif`; ctx.fillStyle = color; ctx.textAlign = align; ctx.fillText(value, x, y);
  }
  function zonePath(ctx, zone) {
    const s = zone.shape; ctx.beginPath();
    if (s.radius !== undefined) {
      if (zone.kind === 'wire' && !zone.semantic) {
        const start = Math.atan2(zone.direction?.y || 0, zone.direction?.x ?? zone.outward) - Math.PI / 2;
        ctx.arc(s.x, s.y, zone.outer, start, start + Math.PI); ctx.closePath();
      } else ctx.arc(s.x, s.y, s.radius, 0, Math.PI * 2);
    } else {
      const p = G.rectPoints(s); ctx.moveTo(p[0].x, p[0].y); p.slice(1).forEach(q => ctx.lineTo(q.x, q.y)); ctx.closePath();
    }
  }
  function fieldPaint(ctx, zone, cursor) {
    const A = window.HitAdaptive, f = zone.field, radius = A.reach(zone, cursor, G);
    ctx.beginPath(); ctx.lineWidth = radius * 2; ctx.lineCap = 'round';
    if (f.type === 'segment') { ctx.moveTo(f.a.x, f.a.y); ctx.lineTo(f.b.x, f.b.y); ctx.stroke(); }
    else if (f.type === 'arc') { ctx.arc(f.center.x, f.center.y, f.radius, f.bearing - Math.PI / 4, f.bearing + Math.PI / 4); ctx.stroke(); }
    else { ctx.arc(zone.anchor.x, zone.anchor.y, radius, 0, Math.PI * 2); ctx.fill(); }
  }
  function silhouettePath(ctx, card, view) {
    ctx.save(); ctx.translate(card.origin.x,card.origin.y); ctx.rotate((card.node.rotation||0)*Math.PI/180); ctx.translate(-card.origin.x,-card.origin.y);
    const r=card.r, radius=Math.max(0,Math.min((card.node.radius||0)*view.z,r.w/2,r.h/2));
    ctx.roundRect(r.x,r.y,r.w,r.h,radius); ctx.restore();
  }
  function adaptivePaint(ctx, layout, state, view, hover) {
    const A = window.HitAdaptive, cursor = hover?.point;
    for (const zone of layout.zones.filter(zone => zone.field && !zone.suppressed)) {
      ctx.save();
      // Draw only the area that can actually receive this object's input.
      for (const card of A.clipCards(layout,zone)) {
        ctx.beginPath(); ctx.rect(0, 0, view.w, view.h);
        silhouettePath(ctx,card,view); ctx.clip('evenodd');
      }
      if (zone.kind==='wire' && layout.cardMap.get(zone.node).hasUnderlay) {
        ctx.beginPath(); silhouettePath(ctx,layout.cardMap.get(zone.node),view); ctx.clip();
      }
      if (state.zones && (zone.kind!=='wire' || A.wireAllowed(layout,zone,cursor,view,G))) {
        ctx.strokeStyle = ctx.fillStyle = colors[zone.kind] + '21'; fieldPaint(ctx, zone, cursor);
      }
      const p = zone.anchor; ctx.lineWidth = 1; ctx.lineCap = 'butt';
      if (zone.kind === 'wire') { ctx.beginPath(); ctx.arc(p.x, p.y, 2.5, 0, Math.PI * 2); ctx.fillStyle = '#eac78b'; ctx.fill(); }
      else if (zone.kind === 'fillet') { ctx.fillStyle = '#d7e7fb'; ctx.fillRect(p.x - 2, p.y - 2, 4, 4); }
      else if (zone.kind === 'rotate') text(ctx, '↻', p.x, p.y + 4, '#eab2c0', 13, 'center');
      else if (zone.corner) { ctx.fillStyle = '#d7e7fb'; ctx.fillRect(p.x - 1.5, p.y - 1.5, 3, 3); }
      ctx.restore();
    }
    for (const zone of layout.zones.filter(zone => zone.frameHeader)) {
      zonePath(ctx, zone); ctx.fillStyle = '#25413b'; ctx.fill();
      text(ctx, `${zone.node} · move selection`, zone.anchor.x, zone.anchor.y + 4, '#c0eadc', 10, 'center');
    }
    if (!state.zones || !cursor) return;
    // This is a local view of the exact winning regions with the current cursor,
    // including clipping and score boundaries; it is not a second hit model.
    const extent = A.policy.previewRadius, step = Math.max(4, Math.ceil(Math.sqrt(extent * extent * 4 * layout.zones.length / A.policy.previewWork)));
    for (let y = Math.max(0, cursor.y - extent); y < Math.min(view.h, cursor.y + extent); y += step)
      for (let x = Math.max(0, cursor.x - extent); x < Math.min(view.w, cursor.x + extent); x += step) {
        const distance = Math.hypot(x - cursor.x, y - cursor.y); if (distance > extent) continue;
        const winner = A.resolve(layout, state, view, {x: x + step / 2, y: y + step / 2}, G, cursor, false).winner;
        if (!winner || !colors[winner.kind]) continue;
        ctx.globalAlpha = (1 - distance / extent) * (winner.id === hover.winner?.id ? 0.48 : 0.23);
        ctx.fillStyle = colors[winner.kind]; ctx.fillRect(x, y, step, step);
      }
    ctx.globalAlpha = 1;
  }
  function draw(canvas, state, view, hover, drag, heat) {
    const ratio = window.devicePixelRatio || 1;
    const width = Math.round(view.w * ratio), height = Math.round(view.h * ratio);
    if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
    const ctx = canvas.getContext('2d'); ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, view.w, view.h); ctx.fillStyle = '#17191c'; ctx.fillRect(0, 0, view.w, view.h);
    ctx.fillStyle = '#303339';
    let pitch = 24 * view.z;
    while (pitch < 12) pitch *= 2; while (pitch > 72) pitch /= 2;
    for (let x = (view.w / 2 - (view.cx || 0) * view.z) % pitch; x < view.w; x += pitch)
      for (let y = (view.h / 2 - (view.cy || 0) * view.z) % pitch; y < view.h; y += pitch) ctx.fillRect(x, y, 1, 1);
    const layout = G.build(state, view), c = state.config, z = view.z, adaptive = state.tinyStrategy === 'adaptive', semantic = ['semantic', 'adaptive'].includes(state.tinyStrategy), paintZ = semantic ? 1 : z;
    for (const wire of state.wires) {
      const from = state.nodes.find(n => n.id === wire.from), to = state.nodes.find(n => n.id === wire.to);
      if (!from) continue;
      const start = G.wirePoint(from, wire.fromSide || 'out', view, state.tinyStrategy, c);
      const end = to ? G.wirePoint(to, 'in', view, state.tinyStrategy, c) :
        {x: view.w / 2 + (wire.end.x - (view.cx || 0)) * z, y: view.h / 2 + (wire.end.y - (view.cy || 0)) * z};
      curve(ctx, start, end);
    }
    if (drag?.started && drag.hit?.kind === 'wire') curve(ctx, drag.hit.shape, drag.current);
    for (const card of layout.cards) {
      if (semantic && !card.visible) continue;
      const {node, r, selected} = card;
      ctx.save(); ctx.translate(card.origin.x, card.origin.y); ctx.rotate((node.rotation || 0) * Math.PI / 180); ctx.translate(-card.origin.x, -card.origin.y);
      ctx.save(); box(ctx, r, (node.radius || 0) * z); ctx.clip();
      if (node.kind === 'generator') {
        ctx.fillStyle = '#262a30'; ctx.fillRect(r.x, r.y, r.w, r.h);
        if (r.w > 110 && r.h > 70) {
          text(ctx, 'Empty generator', r.x + r.w / 2, r.y + r.h / 2, '#aaafb8', 15 * paintZ, 'center');
          text(ctx, 'Body remains movable', r.x + r.w / 2, r.y + r.h / 2 + 24 * paintZ, '#777e88', 11 * paintZ, 'center');
        }
      } else {
        const inks = node.id === 'A' ? ['#596e73', '#85989b', '#a99b80', '#645d55'] : ['#76718a', '#a59bad', '#71837c', '#4f625c'];
        ctx.fillStyle = inks[0]; ctx.fillRect(r.x, r.y, r.w, r.h);
        for (let i = 0; i < 4; i++) { ctx.fillStyle = inks[i]; ctx.fillRect(r.x + (node.panX || 0) * z + (i % 2) * r.w / 2,
          r.y + (node.panY || 0) * z + Math.floor(i / 2) * r.h / 2, r.w / 2 + 1, r.h / 2 + 1); }
        if (r.w > 50 && r.h > 40) text(ctx, node.id, r.x + r.w / 2, r.y + r.h / 2 + 7 * paintZ, '#fafafa', 32 * paintZ, 'center');
      }
      ctx.restore();
      box(ctx, r, (node.radius || 0) * z); ctx.strokeStyle = '#a5a8ac'; ctx.lineWidth = 0.7 * paintZ; ctx.stroke();
      if (selected && !state.popup && state.mode !== 'crop') { ctx.strokeStyle = '#b8d4fa'; ctx.lineWidth = 1.5 * paintZ; ctx.stroke(); }
      ctx.restore();
      if (selected && card.expanded && state.showChrome && (!semantic || card.domains.resize > 0.12)) {
        zonePath(ctx, {shape: G.oriented(card.control, node, card.origin)}); ctx.strokeStyle = '#a9bfdc'; ctx.lineWidth = 1; ctx.setLineDash([4, 4]); ctx.stroke(); ctx.setLineDash([]);
      }
      const bounds = G.boundsOf(G.oriented(r, node, card.origin));
      if (state.labels) text(ctx, `${node.label || node.id} · ${Math.round(node.w)} × ${Math.round(node.h)}${node.rotation ? ' · ' + Math.round(node.rotation) + '°' : ''}`, Math.max(12, Math.min(view.w - 150, bounds.x)), Math.max(16, Math.min(view.h - 30, bounds.y - 9)), '#a5aab3', 11);
    }
    if (adaptive) adaptivePaint(ctx, layout, state, view, hover);
    if (state.zones && !adaptive) for (const zone of layout.zones) {
      if (zone.kind === 'popup') continue;
      ctx.globalAlpha = zone.opacity ?? 1;
      zonePath(ctx, zone); ctx.fillStyle = colors[zone.kind] + '0d'; ctx.strokeStyle = colors[zone.kind] + '60';
      ctx.lineWidth = 0.75; ctx.setLineDash([3, 3]); ctx.fill(); ctx.stroke();
      if (zone.kind === 'wire') { ctx.beginPath(); ctx.arc(zone.shape.x, zone.shape.y, zone.inner, 0, Math.PI * 2); ctx.stroke(); }
    }
    ctx.globalAlpha = 1;
    ctx.setLineDash([]);
    for (const strip of layout.chrome) {
      box(ctx, strip.r, strip.blister ? 12 * z : 0); ctx.fillStyle = strip.blister ? '#191c20' : '#17191c00'; ctx.fill();
    }
    for (const zone of layout.zones) {
      const r = zone.shape;
      if (zone.kind === 'wire' && !semantic) { ctx.beginPath(); ctx.arc(r.x, r.y, 3 * z, 0, Math.PI * 2); ctx.fillStyle = '#d5d9df'; ctx.fill(); }
      if (zone.kind === 'fillet' && !semantic) {
        const p = zone.grip;
        if (zone.detached) {
          ctx.beginPath(); ctx.moveTo(p.x, p.y); ctx.lineTo(zone.native.x, zone.native.y); ctx.strokeStyle = '#727d8c'; ctx.setLineDash([2, 3]); ctx.stroke(); ctx.setLineDash([]);
          ctx.fillStyle = '#29323e'; ctx.fillRect(p.x - 7, p.y - 7, 14, 14); text(ctx, 'R', p.x, p.y + 4, '#fafafa', 11, 'center');
        } else { ctx.fillStyle = '#fafafa'; ctx.fillRect(p.x - 1.6 * z, p.y - 1.6 * z, 3.2 * z, 3.2 * z); }
      }
      if (!semantic && zone.detached && r.radius !== undefined && ['resize', 'crop'].includes(zone.kind) && state.showChrome) { ctx.fillStyle = '#d7e7fb'; ctx.fillRect(r.x - 2, r.y - 2, 4, 4); }
      if (!semantic && zone.kind === 'rotate' && zone.detached && state.showChrome) text(ctx, '↻', r.x, r.y + 5, '#f3c5ce', 17, 'center');
      if (zone.kind === 'toolbar') {
        const strip = layout.chrome[0]; box(ctx, r, strip?.blister ? 3 * paintZ : 9 * paintZ);
        ctx.fillStyle = state.popup === zone.action ? '#3d4350' : '#272b31'; ctx.fill(); ctx.strokeStyle = '#565b64'; ctx.lineWidth = paintZ; ctx.stroke();
        const labels = {Fill: 'F', Stroke: 'S', Corners: 'C', Filter: '◐', Brush: 'B', Layers: 'L', Agent: 'A'};
        text(ctx, labels[zone.action] || zone.action, r.x + r.w / 2, r.y + r.h / 2 + 4 * paintZ, '#d4d8df', 12 * paintZ, 'center');
      }
      if (zone.kind === 'dimension') text(ctx, `${Math.round(state.nodes.find(n => n.id === zone.node).w)}`, r.x + r.w / 2, r.y + r.h * 0.72, '#d4d8df', 12 * paintZ, 'center');
      if (zone.kind === 'align') { box(ctx, r, 2); ctx.fillStyle = '#30353c'; ctx.fill(); text(ctx, {Left: 'L', Center: 'C', Right: 'R'}[zone.action], r.x + r.w / 2, r.y + 13, '#d4d8df', 11, 'center'); }
    }
    if (!semantic && state.mode === 'crop') for (const card of layout.cards.filter(n => n.selected)) {
      const r = card.control, scale = card.expanded ? 1 : Math.min(1, Math.min(r.w, r.h) / (4 * G.hitSize(c.cropArm + c.cropPad, z, c)));
      ctx.save(); ctx.translate(card.origin.x, card.origin.y); ctx.rotate((card.node.rotation || 0) * Math.PI / 180); ctx.translate(-card.origin.x, -card.origin.y);
      ctx.strokeStyle = '#f1f3f5'; ctx.lineWidth = c.cropBar * z * scale;
      for (const [h, x, y] of G.points(r)) {
        ctx.lineWidth = (h.length === 2 ? 2 : c.cropBar) * z * scale;
        ctx.beginPath();
        if (h.length === 2) { const dx = h.includes('w') ? 1 : -1, dy = h.includes('n') ? 1 : -1;
          ctx.moveTo(x + dx * c.cropArm * z * scale, y); ctx.lineTo(x, y); ctx.lineTo(x, y + dy * c.cropArm * z * scale);
        } else { const horizontal = h === 'n' || h === 's'; const half = c.cropBarLength * z * scale / 2;
          ctx.moveTo(x - (horizontal ? half : 0), y - (horizontal ? 0 : half)); ctx.lineTo(x + (horizontal ? half : 0), y + (horizontal ? 0 : half)); }
        ctx.stroke();
      }
      ctx.restore();
    }
    for (const point of heat || []) { ctx.fillStyle = '#e8ad6a99'; ctx.fillRect(point.x - 1, point.y - 1, 2, 2); }
    if (hover?.winner && state.zones && !adaptive) { zonePath(ctx, hover.winner); ctx.fillStyle = colors[hover.winner.kind] + '26'; ctx.strokeStyle = colors[hover.winner.kind]; ctx.lineWidth = 1.5; ctx.fill(); ctx.stroke(); }
    const popup = layout.zones.find(n => n.kind === 'popup');
    if (popup) {
      box(ctx, popup.shape, 8 * paintZ); ctx.fillStyle = '#292d34'; ctx.fill(); ctx.strokeStyle = '#707781'; ctx.lineWidth = paintZ; ctx.stroke();
      text(ctx, `${popup.action} · control layer`, popup.shape.x + 12 * paintZ, popup.shape.y + 24 * paintZ, '#e0e3e8', 12 * paintZ);
      text(ctx, popup.action === 'Corners' ? 'Drag here to adjust corner radius' : 'Representative panel · captures input', popup.shape.x + 12 * paintZ, popup.shape.y + 46 * paintZ, '#9ca4af', 10 * paintZ);
      text(ctx, 'Esc / outside press closes', popup.shape.x + 12 * paintZ, popup.shape.y + 62 * paintZ, '#9ca4af', 10 * paintZ);
    }
    if (semantic && !adaptive) for (const zone of layout.zones.filter(n => n.guard && !['popup', 'toolbar', 'dimension', 'align'].includes(n.kind))) {
      const p = zone.guard, color = colors[zone.kind];
      if (zone.displaced) { ctx.beginPath(); ctx.moveTo(p.x, p.y); ctx.lineTo(Math.max(0, Math.min(view.w, zone.anchor.x)), Math.max(0, Math.min(view.h, zone.anchor.y))); ctx.strokeStyle = color + '65'; ctx.lineWidth = 1; ctx.stroke(); }
      ctx.beginPath(); ctx.arc(p.x, p.y, p.radius, 0, Math.PI * 2); ctx.fillStyle = '#171c23'; ctx.fill(); ctx.strokeStyle = color; ctx.lineWidth = 1; ctx.stroke();
      const glyph = {body: '✥', rotate: '↻', resize: '·', crop: '⌟', fillet: 'R', wire: zone.side === 'in' ? 'I' : 'O'}[zone.kind];
      text(ctx, glyph, p.x, p.y + Math.min(4, p.radius * 0.6), color, Math.min(11, p.radius * 2), 'center');
      if (zone.displaced && zone.kind === 'body') text(ctx, zone.node, p.x + p.radius + 3, p.y + 3, color, 9);
    }
    const rotationPoint = drag?.hit.kind === 'rotate' ? drag.current : hover?.winner?.kind === 'rotate' ? hover.winner.shape : null;
    if (rotationPoint) text(ctx, '↻', rotationPoint.x + 13, rotationPoint.y - 9, '#ffffff', 22);
    if (drag?.started) text(ctx, `${drag.hit.kind.toUpperCase()} · press owner locked`, 16, view.h - 18, '#b8d4fa', 12);
    else text(ctx, adaptive ? 'Stacked: front wins · adjacent corners: nearer host · overlapping wires: inside' : semantic ? 'Protected-point comparison · wheel zoom' : state.tinyStrategy === 'expanded' ? 'Tiny-target proposal · expanded controls below 24 screen px' : 'Source profile · no native acceptance', 16, view.h - 18, '#7d858f', 11);
    return layout;
  }
  function curve(ctx, a, b) {
    ctx.beginPath(); ctx.moveTo(a.x, a.y); const gap = Math.max(50, Math.abs(b.x - a.x) * 0.5);
    ctx.bezierCurveTo(a.x + gap, a.y, b.x - gap, b.y, b.x, b.y); ctx.strokeStyle = '#9aa7b8'; ctx.lineWidth = 1.5; ctx.stroke();
  }
  window.HitRender = {draw, colors};
})();
