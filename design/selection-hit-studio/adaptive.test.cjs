'use strict';
const {test} = require('node:test'), assert = require('node:assert/strict');
const G = require('./geometry.js'), E = require('./engine.js'), A = require('./adaptive.js'), B = require('./source-baseline.json');
const view = {w:920,h:550,z:1,cx:0,cy:0};
const scene = (preset='single') => ({...G.preset(preset),config:{...B.config,cornerHit:7,rotateHit:9},tinyStrategy:'adaptive',showChrome:true});
const hit = (state,p,v=view) => G.inspect(state,v,p).winner;

test('continuous edge bands resize from positions far from a midpoint handle',()=>{
  const state=scene(), card=G.build(state,view).cards[0];
  for(const fraction of [0.2,0.35,0.65,0.8]) {
    const p={x:card.r.x+card.r.w*fraction,y:card.r.y+card.r.h+3};
    assert.equal(hit(state,p)?.id,'A:resize:s');
    const d=E.begin(state,view,p); E.move(state,d,{x:p.x,y:p.y+21}); const result=E.finish(state,view,d);
    assert.equal(result.hit.kind,'resize'); assert.equal(state.nodes[0].h,result.before.nodes[0].h+21);
    E.restore(state,result.before);
  }
});
test('foreground interiors never start hidden rear transforms, wires, or moves',()=>{
  const state=scene('overlap'), layout=G.build(state,view), front=layout.cards[1];
  let checked=0;
  for(let y=front.r.y+12;y<front.r.y+front.r.h-12;y+=9) for(let x=front.r.x+12;x<front.r.x+front.r.w-12;x+=9) {
    assert.equal(hit(state,{x,y})?.node,'B'); checked++;
  }
  assert.ok(checked>100);
  const hiddenCorner=G.points(layout.cards[0].r).find(([h])=>h==='se');
  const pick=G.inspect(state,view,{x:hiddenCorner[1],y:hiddenCorner[2]});
  assert.equal(pick.winner.node,'B'); assert.ok(pick.candidates.some(n=>n.node==='A'&&n.occluded));
});
test('exposed rear edge remains usable beside an overlapping front object',()=>{
  const state=scene('overlap'), card=G.build(state,view).cards[0], p={x:card.r.x+55,y:card.r.y+card.r.h};
  assert.equal(hit(state,p)?.id,'A:resize:s');
  const d=E.begin(state,view,p); E.move(state,d,{x:p.x,y:p.y+30}); const result=E.finish(state,view,d);
  assert.equal(state.nodes[0].h,270); assert.deepEqual(state.nodes[1],result.before.nodes[1]);
});
test('cursor proximity expands regions while competing feature ownership stays deterministic',()=>{
  const state=scene(), layout=G.build(state,view), corner=layout.zones.find(n=>n.id==='A:resize:se');
  const away=A.reach(corner,{x:0,y:0},G), near=A.reach(corner,corner.anchor,G);
  assert.equal(away,7); assert.ok(near>away*1.5);
  const p={x:corner.anchor.x+8,y:corner.anchor.y+1};
  assert.equal(A.resolve(layout,state,view,p,G,p).winner.id,corner.id);
  assert.notEqual(A.resolve(layout,state,view,p,G,{x:0,y:0}).winner?.id,corner.id);
  assert.equal(hit(state,{x:corner.anchor.x+2,y:corner.anchor.y+2}).id,corner.id);
  assert.equal(hit(state,layout.cards[0].origin).kind,'body');
});
test('small-object frames blend with zoom without converting targets to guards',()=>{
  const state=scene(); state.nodes=[{id:'P',kind:'image',x:-18,y:-15,w:36,h:30,radius:0}]; state.selected=['P'];
  const low=G.build(state,view), high=G.build(state,{...view,z:1.001});
  assert.ok(low.cards[0].control.w>low.cards[0].r.w);
  assert.ok(Math.abs(high.cards[0].control.w-low.cards[0].control.w)<0.2);
  assert.ok(low.zones.every(zone=>!zone.guard&&!zone.displaced));
  assert.equal(hit(state,low.cards[0].origin).kind,'body');
});
test('explicitly selecting a buried node reveals one outside frame and a move header',()=>{
  const state=scene(); const node={id:'A',kind:'image',x:-60,y:-45,w:120,h:90,radius:0};
  state.nodes=[node,{...node,id:'B'}]; state.selected=['A'];
  let layout=G.build(state,view), frame=layout.cards.find(card=>card.node.id==='A');
  assert.ok(frame.coverFrame); assert.ok(frame.control.w>frame.r.w);
  const header=layout.zones.find(zone=>zone.frameHeader), p=header.anchor;
  assert.equal(hit(state,p).id,header.id);
  const d=E.begin(state,view,p); E.move(state,d,{x:p.x+30,y:p.y+10}); const result=E.finish(state,view,d);
  assert.equal(state.nodes[0].x,node.x+30); assert.deepEqual(state.nodes[1],result.before.nodes[1]);
  E.restore(state,result.before); state.selected=['B']; layout=G.build(state,view);
  assert.ok(!layout.cards.some(card=>card.coverFrame));
  assert.equal(hit(state,{x:view.w/2,y:view.h/2}).node,'B');
});
test('all control domains of a selected coincident node retain actual winning area',()=>{
  const state=scene(), node={id:'A',kind:'image',x:-60,y:-45,w:120,h:90,radius:0};
  state.nodes=[node,{...node,id:'B'}];
  const layout=G.build(state,view), targets=layout.zones.filter(zone=>zone.node==='A'&&zone.field);
  for(const zone of targets) {
    let wins=0;
    const cores=zone.field.type==='segment' ? Array.from({length:9},(_,i)=>({
      x:zone.field.a.x+(zone.field.b.x-zone.field.a.x)*i/8,
      y:zone.field.a.y+(zone.field.b.y-zone.field.a.y)*i/8})) : [zone.anchor];
    for(const core of cores) for(let y=-8;y<=8;y+=2) for(let x=-8;x<=8;x+=2)
      if(hit(state,{x:core.x+x,y:core.y+y})?.id===zone.id) wins++;
    assert.ok(wins>=3,`${zone.id}: ${wins} winning probes`);
  }
});
test('the buried-object move header avoids neighboring silhouettes as well as its covering object',()=>{
  const state=scene('overlap-lab'); state.selected=['E'];
  const v={...view,h:498,z:0.8778}, layout=G.build(state,v), header=layout.zones.find(n=>n.frameHeader);
  for(const [,x,y] of G.points(header.shape)) assert.equal(hit(state,{x,y},v)?.id,header.id);
  assert.equal(hit(state,header.anchor,v)?.id,header.id);
});
test('rotation remains usable over a continuous arc, not just a point handle',()=>{
  const state=scene(), zone=G.build(state,view).zones.find(zone=>zone.id==='A:rotate:se');
  for(const delta of [-0.5,-0.2,0.2,0.5]) {
    const f=zone.field, p={x:f.center.x+Math.cos(f.bearing+delta)*f.radius,y:f.center.y+Math.sin(f.bearing+delta)*f.radius};
    assert.equal(hit(state,p)?.id,zone.id);
  }
});
test('wire outward regions connect and tiny objects regain wires at close zoom',()=>{
  const state=scene('wire'), layout=G.build(state,view), out=layout.zones.find(n=>n.node==='A'&&n.kind==='wire'&&n.side==='out');
  const input=layout.zones.find(n=>n.node==='B'&&n.kind==='wire'&&n.side==='in');
  const d=E.begin(state,view,out.anchor); assert.equal(d.hit.kind,'wire'); assert.equal(d.hit.node,'A');
  E.move(state,d,input.anchor); E.finish(state,view,d); assert.equal(state.wires[0].to,'B');
  state.nodes=[{id:'P',kind:'image',x:-0.5,y:-0.5,w:1,h:1,radius:0}]; state.selected=['P'];
  assert.ok(G.build(state,{...view,z:1/128}).zones.every(n=>n.kind==='body'));
  const near=G.build(state,{...view,z:128}); assert.equal(near.zones.filter(n=>n.kind==='wire').length,2);
});
test('press owner survives new overlap, movement across targets, and zoom-domain fade',()=>{
  const state=scene('overlap'), layout=G.build(state,view), p={x:layout.cards[1].r.x+100,y:layout.cards[1].r.y+95};
  const d=E.begin(state,view,p); assert.equal(d.hit.kind,'body'); assert.equal(d.hit.node,'B');
  E.move(state,d,{x:p.x-100,y:p.y-100}); const result=E.finish(state,view,d);
  assert.equal(result.hit.node,'B'); assert.deepEqual(state.nodes[0],result.before.nodes[0]);
  E.restore(state,result.before); assert.deepEqual(state.nodes,result.before.nodes);
});
test('rotated edge bands follow their full side and fillet stays on the top edge',()=>{
  const state=scene(); state.nodes[0].rotation=33;
  const layout=G.build(state,view), card=layout.cards[0], edge=layout.zones.find(n=>n.id==='A:resize:s');
  const p={x:edge.field.a.x*0.7+edge.field.b.x*0.3,y:edge.field.a.y*0.7+edge.field.b.y*0.3};
  assert.equal(hit(state,p)?.id,edge.id);
  const fillet=layout.zones.find(n=>n.kind==='fillet'), local=G.rotatePoint(fillet.anchor,card.origin,-33);
  assert.ok(Math.abs(local.y-card.r.y)<1e-9); assert.equal(hit(state,fillet.anchor)?.kind,'fillet');
});
test('adaptive audit reports genuinely covered targets instead of claiming all resolved',()=>{
  const state=scene(), node={id:'A',kind:'image',x:-30,y:-20,w:60,h:40,radius:0};
  state.nodes=[node,{...node,id:'B'}]; state.selected=['B'];
  const result=A.audit(state,{...view,w:350,h:500},G);
  assert.ok(result.hidden.some(zone=>zone.node==='A'));
  assert.ok(result.reachable<result.total); assert.ok(result.areas['B:body:']>100);
});
