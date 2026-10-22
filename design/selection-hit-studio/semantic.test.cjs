'use strict';
const {test} = require('node:test'), assert = require('node:assert/strict');
const G = require('./geometry.js'), E = require('./engine.js'), S = require('./semantic.js'), B = require('./source-baseline.json');
const view = {w: 920, h: 500, z: 1, cx: 0, cy: 0};
const scene = () => ({...G.preset('scale'), config: {...B.config, cornerHit: 7}, tinyStrategy: 'semantic', showChrome: true});

test('continuous prominence is monotone and follows wire→resize→rotate→move', () => {
  let previous = S.domains(0);
  for (let i = -400; i <= 1000; i++) {
    const size = Math.exp(i / 100), d = S.domains(size), next = S.domains(size * 1.0001);
    assert.ok(d.wire <= d.resize && d.resize <= d.rotate && d.rotate <= d.move);
    for (const key of ['wire','resize','rotate','move']) {
      assert.ok(d[key] >= previous[key]); assert.ok(next[key] - d[key] < 0.001);
    }
    previous = d;
  }
});
test('one-unit objects regain wire controls by zooming in, without an authored-size cutoff', () => {
  const state = scene(); state.nodes = [{id:'P',kind:'image',x:-0.5,y:-0.5,w:1,h:1,radius:0}]; state.selected=['P'];
  assert.ok(G.build(state,view).zones.every(n=>n.kind==='body'));
  const near = G.build(state,{...view,z:64});
  assert.equal(near.zones.filter(n=>n.kind==='wire').length,2);
  assert.equal(S.audit(state,{...view,z:64},G).failures.length,0);
});
test('the same projected size has the same hit radii across world scale and zoom', () => {
  const a = scene(), b = scene();
  a.nodes=[{id:'P',kind:'image',x:-12,y:-9,w:24,h:18,radius:0}]; a.selected=['P'];
  b.nodes=[{id:'P',kind:'image',x:-120,y:-90,w:240,h:180,radius:0}]; b.selected=['P'];
  const x=G.build(a,{...view,z:2}), y=G.build(b,{...view,z:0.2});
  for (const target of x.zones.filter(n=>n.shape.radius!==undefined)) {
    const same=y.zones.find(n=>n.id===target.id); assert.ok(same);
    assert.ok(Math.abs(target.shape.radius-same.shape.radius)<1e-9,target.id);
  }
});
test('all active targets retain a reachable patch across the full zoom ladder', () => {
  const state = scene();
  for (const z of [1/128,1/32,0.1,0.25,0.5,1,2,4,8,16,32,64,128]) {
    const result = S.audit(state,{...view,z},G);
    assert.deepEqual(result.failures,[],`zoom ${z}`);
    const layout=G.build(state,{...view,z}), visible=layout.cards.filter(n=>n.visible);
    assert.ok(result.active>=visible.length);
    for (const card of visible) assert.ok(layout.zones.some(n=>n.node===card.node.id&&n.kind==='body'));
  }
});
test('fully overlapping, thin rotated, and offscreen targets remain reachable in narrow viewports', () => {
  const state = scene(); state.nodes = Array.from({length:8},(_,i)=>({id:'N'+i,kind:'image',x:i<4?-50:300,y:i<4?-50:250,
    w:i%3===0?1:100,h:i%3===1?1:100,rotation:i*31,radius:0})); state.selected=state.nodes.map(n=>n.id);
  for (const z of [0.1,1,8,128]) {
    const result=S.audit(state,{...view,w:340,h:500,z},G);
    assert.deepEqual(result.failures,[],`zoom ${z}`);
  }
});
test('dormant controls have no protected patch and cannot win a hit', () => {
  const state=scene(), v={...view,z:1/128}, layout=G.build(state,v);
  assert.ok(layout.dormant.length>0); assert.ok(layout.zones.every(n=>n.kind==='body'));
  for (const item of layout.dormant) { assert.equal(item.guard,undefined); assert.notEqual(G.inspect(state,v,item.shape).winner?.id,item.id); }
});
test('a held resize retains its owner and visible patch when it crosses into move-only size', () => {
  const state=scene(); state.nodes=[{id:'P',kind:'image',x:-12,y:-9,w:24,h:18,radius:0}]; state.selected=['P'];
  const v={...view,z:2}, target=G.build(state,v).zones.find(n=>n.kind==='resize'&&n.handle==='s');
  const start=target.guard, d=E.begin(state,v,start); E.move(state,d,{x:start.x,y:start.y-40});
  assert.equal(state.nodes[0].h,1);
  const held=G.build(state,v).zones.find(n=>n.id===target.id);
  assert.ok(held?.locked); assert.equal(G.inspect(state,v,held.guard).winner.id,target.id);
  const result=E.finish(state,v,d); assert.equal(result.hit.id,target.id);
  assert.ok(!G.build(state,v).zones.some(n=>n.id===target.id));
});
test('cursor-anchored zoom preserves the world point and round-trips the camera', () => {
  const point={x:171,y:119}, original={...view,cx:23,cy:-42,z:0.7}, world=S.worldAt(point,original);
  const near=S.zoomAt(original,64,point), far=S.zoomAt(near,original.z,point);
  assert.ok(G.distance(world,S.worldAt(point,near))<1e-9);
  assert.ok(Math.abs(far.cx-original.cx)<1e-9); assert.ok(Math.abs(far.cy-original.cy)<1e-9);
});
test('a wire released on a displaced protected input connects behind an overlapping object', () => {
  const state=scene(); state.nodes=[{id:'A',kind:'image',x:-2,y:-0.5,w:1,h:1,radius:0},
    {id:'B',kind:'image',x:2,y:-0.5,w:1,h:1,radius:0},{id:'C',kind:'image',x:1.5,y:-1,w:2,h:2,radius:0}]; state.selected=['A'];
  const v={...view,z:64}, layout=G.build(state,v), from=layout.zones.find(n=>n.node==='A'&&n.kind==='wire'&&n.side==='out');
  const to=layout.zones.find(n=>n.node==='B'&&n.kind==='wire'&&n.side==='in');
  assert.ok(!G.bodyContains(state.nodes[1],G.screenRect(state.nodes[1],v),to.guard,v.z));
  const d=E.begin(state,v,from.guard); E.move(state,d,to.guard); E.finish(state,v,d);
  assert.equal(state.wires[0].from,'A'); assert.equal(state.wires[0].to,'B');
});
test('fully offscreen objects are dormant while partially visible objects keep reachable controls', () => {
  const state=scene(); state.nodes=[{id:'A',kind:'image',x:10000,y:10000,w:100,h:100,radius:0},
    {id:'B',kind:'image',x:450,y:-30,w:100,h:100,radius:0}]; state.selected=['B'];
  const layout=G.build(state,view); assert.ok(!layout.zones.some(n=>n.node==='A'));
  assert.ok(layout.zones.some(n=>n.node==='B'&&n.kind==='wire'));
  assert.equal(S.audit(state,view,G).failures.length,0);
});
test('free wire endpoints account for a zoomed and panned camera', () => {
  const state=scene(); state.nodes=[{id:'A',kind:'image',x:30,y:20,w:100,h:80,radius:0}]; state.selected=['A'];
  const v={...view,z:2,cx:70,cy:60}, port=G.build(state,v).zones.find(n=>n.kind==='wire'&&n.side==='out');
  const end={x:900,y:460}, d=E.begin(state,v,port.guard); E.move(state,d,end); E.finish(state,v,d);
  assert.deepEqual(state.wires[0].end,S.worldAt(end,v));
});
test('closing a popup preserves a visible displaced move target as the first drag owner', () => {
  const state=scene(); state.popup='Corners';
  const panel=G.build(state,view).zones.find(n=>n.kind==='popup').shape, middle=G.center(panel);
  state.nodes[3]={...state.nodes[3],x:middle.x-view.w/2-0.5,y:middle.y-view.h/2-0.5};
  const layout=G.build(state,view), target=layout.zones.find(n=>n.node==='D'&&n.kind==='body');
  assert.ok(target.displaced); assert.deepEqual(S.audit(state,view,G).failures,[]);
  const d=E.begin(state,view,target.guard); assert.equal(d.hit.node,'D'); assert.equal(state.popup,null);
  const old=state.nodes.find(n=>n.id==='D').x;
  E.move(state,d,{x:target.guard.x+20,y:target.guard.y}); E.finish(state,view,d);
  assert.equal(state.nodes.find(n=>n.id==='D').x,old+20);
});
test('capacity fallback gives all 480 possible node targets separate visible patches', () => {
  const v={...view,w:330,h:500}, rail={x:40,y:441,w:243,h:26};
  const zones=Array.from({length:480},(_,i)=>({id:'target'+i,kind:'resize',shape:{x:0,y:0,radius:24},unplaced:true}));
  S.reserveGrid(zones,[rail],v,G);
  for(let i=0;i<zones.length;i++) {
    const p=zones[i].guard; assert.ok(p&&!zones[i].unplaced);
    assert.ok(p.x-p.radius>=0&&p.y-p.radius>=0&&p.x+p.radius<=v.w&&p.y+p.radius<=v.h-S.policy.viewportBottom);
    assert.ok(p.x+p.radius<=rail.x||p.x-p.radius>=rail.x+rail.w||p.y+p.radius<=rail.y||p.y-p.radius>=rail.y+rail.h);
    for(let j=0;j<i;j++) assert.ok(G.distance(p,zones[j].guard)>=p.radius+zones[j].guard.radius+S.policy.gap-1e-8);
  }
});
