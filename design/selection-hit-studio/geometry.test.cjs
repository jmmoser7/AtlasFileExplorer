'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const G = require('./geometry.js'), E = require('./engine.js');
const baseline = require('./source-baseline.json');
const view = {w: 1000, h: 700, z: 1};
const scene = (preset = 'single') => ({config: G.clone(baseline.config), ...G.preset(preset), showChrome: true});
const at = (x, y) => ({x: view.w / 2 + x, y: view.h / 2 + y});

test('baseline records all native owner hashes and intended screen slop', () => {
  assert.equal(baseline.sources.length, 18);
  assert.ok(baseline.sources.every(source => /^[a-f0-9]{64}$/.test(source.baselineSha256) && source.currentWorkingTreeMatchesBaseline));
  assert.equal(G.hitSize(12, 0.25, baseline.config), 8);
  assert.equal(G.hitSize(12, 2, baseline.config), 24);
});
test('foreground body occludes rear corner, edge, and wire candidates', () => {
  const s = scene('overlap');
  s.nodes[1] = {...s.nodes[1], x: 120, y: -170, w: 230, h: 300};
  for (const p of [at(160, -140), at(160, -110), at(160, 30)]) {
    const pick = G.inspect(s, view, p);
    assert.equal(pick.winner.node, 'B'); assert.equal(pick.winner.kind, 'body');
    assert.ok(pick.candidates.some(candidate => candidate.node === 'A' && candidate.occluded));
  }
});
test('frontmost corner owns its exposed corner above a rear body', () => {
  const s = scene('overlap'), pick = G.inspect(s, view, at(255, 110));
  assert.equal(pick.winner.node, 'B'); assert.equal(pick.winner.kind, 'resize'); assert.equal(pick.winner.handle, 'se');
});
test('tiny body stays movable when corner reach covers its center', () => {
  const s = scene('tiny'); s.nodes[0].w = 12; s.nodes[0].h = 10;
  const pick = G.inspect(s, view, at(-104, -15));
  assert.equal(pick.winner.node, 'A'); assert.equal(pick.winner.kind, 'body');
  assert.ok(pick.candidates.some(item => item.kind === 'resize'));
});
test('tiny crop preserves eight distinct handles and an available center over zoom', () => {
  for (const z of [0.25, 0.5, 1, 2]) {
    const s = scene('tiny'); s.mode = 'crop'; const v = {...view, z};
    const r = G.screenRect(s.nodes[0], v);
    for (const [handle, x, y] of G.points(r)) {
      const pick = G.inspect(s, v, {x, y});
      assert.equal(pick.winner.kind, 'crop'); assert.equal(pick.winner.handle, handle);
    }
    assert.equal(G.inspect(s, v, {x: r.x + r.w / 2, y: r.y + r.h / 2}).winner.kind, 'body');
    assert.ok(G.build(s, v).zones.every(item => !['wire', 'toolbar', 'dimension', 'fillet', 'align', 'resize'].includes(item.kind)));
  }
});
test('wire extension yields to resize while the actual port wins its center', () => {
  const s = scene(), port = at(160, -110);
  assert.equal(G.inspect(s, view, port).winner.kind, 'wire');
  assert.equal(G.inspect(s, view, at(160, -140)).winner.kind, 'resize');
  assert.equal(G.inspect(s, view, at(185, -110)).winner.kind, 'wire');
  const tiny = scene('tiny');
  assert.ok(G.build(tiny, view).zones.every(item => item.node !== 'A' || item.kind !== 'wire'));
});
test('one coalesced movement crossing a corner retains its body press owner', () => {
  const s = scene(), before = E.snapshot(s), drag = E.begin(s, view, at(0, -20));
  assert.equal(drag.hit.kind, 'body');
  E.move(s, drag, at(175, -145));
  const result = E.finish(s, view, drag);
  assert.ok(result.changed); assert.equal(result.hit.kind, 'body');
  assert.equal(s.nodes[0].x, before.nodes[0].x + 175);
  assert.equal(s.nodes[0].y, before.nodes[0].y - 125);
  assert.equal(s.nodes[0].w, 320); assert.equal(s.nodes[0].h, 240);
  E.restore(s, result.before); assert.deepEqual(E.snapshot(s), before);
  E.restore(s, result.after); assert.equal(s.nodes[0].x, 15);
});
test('sub-threshold press is a click; cancel restores a moved scene', () => {
  const s = scene(), before = E.snapshot(s), drag = E.begin(s, view, at(0, -20));
  E.move(s, drag, at(3, -17)); assert.equal(drag.started, false);
  assert.equal(E.finish(s, view, drag).changed, false);
  E.move(s, drag, at(40, 30)); E.cancel(s, drag); assert.deepEqual(E.snapshot(s), before);
});
test('empty generator moves on its first press and group move keeps spacing', () => {
  const s = scene('wire'), drag = E.begin(s, view, at(260, -30));
  assert.equal(drag.hit.kind, 'body'); E.move(s, drag, at(290, -10)); E.finish(s, view, drag);
  assert.equal(s.nodes[1].x, 200); assert.equal(s.nodes[1].y, -105);
  const group = scene('crowded'), original = E.snapshot(group), d = E.begin(group, view, at(0, -20));
  E.move(group, d, at(20, 0)); E.finish(group, view, d);
  for (const id of ['A', 'B']) assert.equal(group.nodes.find(n => n.id === id).x, original.nodes.find(n => n.id === id).x + 20);
  assert.equal(group.nodes[2].x, original.nodes[2].x);
});
test('crop body pans its test content without moving its viewport', () => {
  const s = scene('crop'), original = {...s.nodes[0]}, d = E.begin(s, view, at(0, -20));
  E.move(s, d, at(25, -5)); E.finish(s, view, d);
  assert.equal(s.nodes[0].x, original.x); assert.equal(s.nodes[0].w, original.w);
  assert.equal(s.nodes[0].panX, 25); assert.equal(s.nodes[0].panY, 15);
});
test('Shift-drag keeps the pressed member and group; Shift-click removes only on release', () => {
  const s = scene('crowded'), before = E.snapshot(s), d = E.begin(s, view, at(0, -20), true);
  assert.deepEqual(s.selected, ['A', 'B']);
  E.move(s, d, at(20, 0)); E.finish(s, view, d);
  for (const id of ['A', 'B']) assert.equal(s.nodes.find(n => n.id === id).x, before.nodes.find(n => n.id === id).x + 20);
  const single = scene(), click = E.begin(single, view, at(0, -20), true);
  assert.deepEqual(single.selected, ['A']); E.finish(single, view, click); assert.deepEqual(single.selected, []);
});
test('closing a popup preserves the next body drag; popup content captures input', () => {
  const s = scene(); s.popup = 'Fill';
  const popup = G.build(s, view).zones.find(n => n.kind === 'popup').shape;
  const p = {x: popup.x + 30, y: popup.y + 20}, d = E.begin(s, view, p);
  assert.equal(d.hit.kind, 'popup'); E.move(s, d, {x: p.x + 30, y: p.y});
  assert.equal(E.finish(s, view, d).changed, false);
  const body = E.begin(s, view, at(0, -20));
  assert.equal(s.popup, null); assert.equal(body.hit.kind, 'body');
  E.move(s, body, at(30, -20)); assert.ok(E.finish(s, view, body).changed);
});
test('resize preserves its opposite edge and has a finite minimum', () => {
  const node = scene().nodes[0], resized = G.resize(node, 'nw', 1000, 1000);
  assert.equal(resized.w, 1); assert.equal(resized.h, 1);
  assert.equal(resized.x + resized.w, node.x + node.w);
  assert.equal(resized.y + resized.h, node.y + node.h);
});
test('a missed target records its exact press without inventing a mutation', () => {
  const s = scene(), before = E.snapshot(s), point = at(-400, -280), d = E.begin(s, view, point);
  assert.equal(d.hit.kind, 'canvas'); assert.deepEqual(d.start, point);
  E.move(s, d, at(0, -20)); const result = E.finish(s, view, d);
  assert.equal(result.hit.kind, 'canvas'); assert.equal(result.changed, false);
  E.cancel(s, d); assert.deepEqual(E.snapshot(s), before);
});
test('rotation zones use native radial offset, resize precedence, and generator exclusion', () => {
  const s = scene(), layout = G.build(s, view), corner = at(-160, -140);
  const rotation = layout.zones.find(n => n.kind === 'rotate' && n.handle === 'nw');
  assert.ok(Math.abs(G.distance(rotation.shape, corner) - 18) < 1e-8);
  assert.equal(G.inspect(s, view, rotation.shape).winner.kind, 'rotate');
  assert.equal(G.inspect(s, view, corner).winner.kind, 'resize');
  assert.ok(G.build(scene('wire'), view).zones.every(n => n.node !== 'B' || n.kind !== 'rotate'));
});
test('rotation follows pointer bearing, snaps at 45 degrees, and can be cancelled or undone', () => {
  const s = scene(), before = E.snapshot(s), start = G.build(s, view).zones.find(n => n.kind === 'rotate').shape;
  const d = E.begin(s, view, start), end = G.rotatePoint(start, d.origin, 47);
  E.move(s, d, end); assert.equal(s.nodes[0].rotation, 45);
  E.move(s, d, end, {alt: true}); assert.ok(Math.abs(s.nodes[0].rotation - 47) < 1e-8);
  const result = E.finish(s, view, d); assert.ok(result.changed);
  E.restore(s, result.before); assert.deepEqual(E.snapshot(s), before);
  E.restore(s, result.after); assert.ok(Math.abs(s.nodes[0].rotation - 47) < 1e-8);
  E.cancel(s, d); assert.deepEqual(E.snapshot(s), before);
});
test('fillet sits on the top edge, follows radius, and drag uses rotated edge projection', () => {
  const s = scene(); s.nodes[0].rotation = 90;
  const r = G.screenRect(s.nodes[0], view), origin = G.center(r);
  let grip = G.build(s, view).zones.find(n => n.kind === 'fillet');
  assert.deepEqual(grip.grip, G.rotatePoint({x: r.x + 10, y: r.y}, origin, 90));
  assert.equal(G.inspect(s, view, grip.grip).winner.kind, 'fillet');
  const d = E.begin(s, view, grip.grip), end = {x: grip.grip.x, y: grip.grip.y + 30};
  E.move(s, d, end); assert.ok(Math.abs(s.nodes[0].radius - 30) < 1e-8);
  assert.ok(Math.abs(stateTravel(s) - 40) < 1e-8);
  E.finish(s, view, d); assert.equal(s.heldFillet, undefined);
  grip = G.build(s, view).zones.find(n => n.kind === 'fillet');
  assert.deepEqual(grip.grip, G.rotatePoint({x: r.x + 30, y: r.y}, origin, 90));
  function stateTravel(state) { return state.heldFillet.travel; }
});
test('rotated body, handles and opposite resize anchor use the same orientation', () => {
  const s = scene(); s.nodes[0].rotation = 45;
  const node = s.nodes[0], r = G.screenRect(node, view), origin = G.center(r);
  const corner = G.rotatePoint({x: r.x + r.w, y: r.y + r.h}, origin, 45);
  assert.equal(G.inspect(s, view, corner).winner.handle, 'se');
  assert.equal(G.inspect(s, view, origin).winner.kind, 'body');
  const worldOpposite = G.rotatePoint({x: node.x, y: node.y}, G.center(node), 45);
  const resized = G.resize(node, 'se', 40, 10);
  const afterOpposite = G.rotatePoint({x: resized.x, y: resized.y}, G.center(resized), 45);
  assert.ok(G.distance(worldOpposite, afterOpposite) < 1e-8);
});
test('expanded tiny strategy preserves body and every resize/crop/rotate/fillet target down to 1u', () => {
  for (const size of [[24,18],[16,12],[8,6],[4,3],[1,1],[1,200],[200,1]]) for (const z of [0.25,0.5,1,2]) for (const rotation of [0,31,90]) {
    const s = scene(); s.tinyStrategy = 'expanded';
    s.nodes[0] = {...s.nodes[0], w:size[0], h:size[1], x:-size[0]/2, y:-size[1]/2, rotation};
    const v = {...view,z}, layout = G.build(s,v), card = layout.cards[0];
    assert.equal(G.inspect(s,v,card.origin).winner.kind,'body');
    if (!card.expanded) continue;
    for (const target of layout.zones.filter(n => ['resize','rotate','fillet'].includes(n.kind))) {
      const winner = G.inspect(s,v,G.shapeCenter(target.shape)).winner;
      assert.equal(winner?.kind,target.kind,`size${size} z${z} rot${rotation}: ${target.kind} ${target.handle}`);
      if (target.handle) assert.equal(winner.handle,target.handle);
    }
    s.mode='crop';
    for (const target of G.build(s,v).zones.filter(n => n.kind==='crop')) {
      const winner = G.inspect(s,v,G.shapeCenter(target.shape)).winner;
      assert.equal(winner?.kind,'crop'); assert.equal(winner.handle,target.handle);
    }
  }
});
test('an expanded resize grip follows its pointer throughout the held gesture', () => {
  const s=scene(); s.tinyStrategy='expanded'; s.nodes[0]={...s.nodes[0],x:-0.5,y:-0.5,w:1,h:1};
  const target=G.build(s,view).zones.find(n=>n.kind==='resize'&&n.handle==='e'), p=target.shape;
  const d=E.begin(s,view,p), end={x:p.x+20,y:p.y}; E.move(s,d,end);
  const held=G.build(s,view).zones.find(n=>n.kind==='resize'&&n.handle==='e');
  assert.ok(G.distance(held.shape,end)<1e-8); assert.equal(s.nodes[0].w,21);
  E.finish(s,view,d); assert.equal(s.heldControl,undefined);
});
test('expanded controls yield when a neighbor painted body covers them', () => {
  const s=scene(); s.tinyStrategy='expanded'; s.nodes[0]={...s.nodes[0],x:-0.5,y:-0.5,w:1,h:1};
  const target=G.build(s,view).zones.find(n=>n.kind==='resize' && n.handle==='e').shape;
  s.nodes.push({id:'B',kind:'image',x:target.x-view.w/2-15,y:target.y-view.h/2-20,w:60,h:40,radius:0});
  const pick=G.inspect(s,view,target);
  assert.equal(pick.winner.node,'B'); assert.equal(pick.winner.kind,'body');
  assert.ok(pick.candidates.some(n=>n.node==='A'&&n.detached&&n.occluded));
});
test('expanded crop edge bars are pickable across their full painted length when rotated', () => {
  const s=scene(); s.tinyStrategy='expanded'; s.mode='crop';
  s.nodes[0]={...s.nodes[0],x:-2,y:-1.5,w:4,h:3,rotation:31};
  const v={...view,z:2}, card=G.build(s,v).cards[0], r=card.control;
  const p=G.rotatePoint({x:r.x+r.w,y:r.y+r.h/2+baseline.config.cropBarLength*v.z/2},card.origin,31);
  const winner=G.inspect(s,v,p).winner; assert.equal(winner?.kind,'crop'); assert.equal(winner.handle,'e');
});
test('an inward tiny resize clamps both the object and its rotated control cage at 1u', () => {
  const s=scene(); s.tinyStrategy='expanded'; s.nodes[0]={...s.nodes[0],x:-0.5,y:-0.5,w:1,h:1,rotation:31};
  const before=G.build(s,view), p=before.zones.find(n=>n.kind==='resize'&&n.handle==='e').shape;
  const delta=G.rotatePoint({x:-20,y:0},{x:0,y:0},31), d=E.begin(s,view,p);
  E.move(s,d,{x:p.x+delta.x,y:p.y+delta.y});
  const after=G.build(s,view); assert.equal(s.nodes[0].w,1);
  assert.ok(G.distance(after.zones.find(n=>n.kind==='resize'&&n.handle==='e').shape,p)<1e-8);
  assert.ok(G.distance(G.center(after.cards[0].control),after.cards[0].origin)<1e-8);
});
test('alignment compares painted rotated bounds instead of unrotated rectangle coordinates', () => {
  const s=scene('overlap'); s.selected=['A','B'];
  s.nodes[0]={...s.nodes[0],x:-150,y:-50,w:100,h:100};
  s.nodes[1]={...s.nodes[1],x:30,y:-50,w:100,h:100,rotation:45};
  const target=G.build(s,view).zones.find(n=>n.kind==='align'&&n.action==='Left');
  const d=E.begin(s,view,G.shapeCenter(target.shape)); E.finish(s,view,d);
  const [a,b]=s.nodes.map(n=>G.boundsOf(G.oriented(n,n,G.center(n))));
  assert.ok(Math.abs(a.x-b.x)<1e-8);
});
