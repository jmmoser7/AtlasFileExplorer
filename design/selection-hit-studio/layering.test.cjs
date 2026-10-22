'use strict';
const {test}=require('node:test'), assert=require('node:assert/strict');
const G=require('./geometry.js'),A=require('./adaptive.js'),E=require('./engine.js'),B=require('./source-baseline.json');
const view={w:920,h:550,z:1,cx:0,cy:0};
const node=(id,x,y,w=100,h=100)=>({id,kind:'image',x,y,w,h,radius:0});
const state=nodes=>({nodes,selected:[nodes.at(-1).id],wires:[],mode:'select',popup:null,config:{...B.config,cornerHit:7,rotateHit:9},tinyStrategy:'adaptive',showChrome:true});
const screen=(p,v=view)=>({x:v.w/2+(p.x-(v.cx||0))*v.z,y:v.h/2+(p.y-(v.cy||0))*v.z});
const hit=(s,p,v=view)=>G.inspect(s,v,p).winner;

test('stacking preserves every foreground transform region over a lower body',()=>{
  const front=node('B',-50,-50), alone=state([front]), stacked=state([node('A',-200,-180,400,360),front]);
  const original=G.build(alone,view), combined=G.build(stacked,view);
  for(const zone of original.zones.filter(n=>n.field&&n.kind!=='wire')) {
    const next=combined.zones.find(n=>n.id===zone.id); assert.deepEqual(next.field,zone.field); assert.deepEqual(next.anchor,zone.anchor);
  }
  const seen=new Set(); let probes=0;
  for(let y=-95;y<=95;y+=3) for(let x=-95;x<=95;x+=3) {
    const p=screen({x,y}), before=hit(alone,p);
    if(!before || !['resize','rotate','fillet','body'].includes(before.kind)) continue;
    assert.equal(hit(stacked,p)?.id,before.id,`foreground zone at ${x},${y}`); seen.add(before.kind); probes++;
  }
  assert.ok(probes>1000); assert.deepEqual([...seen].sort(),['body','fillet','resize','rotate']);
});
test('foreground rotation and scaling beat rear controls even over the rear move core',()=>{
  const s=state([node('A',-80,-80,160,160),node('B',8,8)]), l=G.build(s,view);
  for(const id of ['B:rotate:nw','B:resize:nw']) {
    const zone=l.zones.find(n=>n.id===id); assert.equal(hit(s,zone.anchor)?.id,id);
  }
  const rotate=l.zones.find(n=>n.id==='B:rotate:nw'), d=E.begin(s,view,rotate.anchor);
  E.move(s,d,{x:rotate.anchor.x+15,y:rotate.anchor.y-15}); const result=E.finish(s,view,d);
  assert.equal(result.hit.id,rotate.id); assert.notEqual(s.nodes[1].rotation,0); assert.deepEqual(s.nodes[0],result.before.nodes[0]);
});
test('overlapping foreground wire regions activate only strictly inside their visible host',()=>{
  const front=node('B',-50,-50), stacked=state([node('A',-200,-180,400,360),front]), alone=state([front]);
  const layout=G.build(stacked,view), wire=layout.zones.find(n=>n.id==='B:wire:out');
  const inside={x:wire.anchor.x-2,y:wire.anchor.y}, outside={x:wire.anchor.x+2,y:wire.anchor.y};
  assert.equal(hit(alone,outside)?.kind,'wire');
  assert.equal(hit(stacked,inside)?.id,wire.id);
  assert.notEqual(hit(stacked,wire.anchor)?.kind,'wire'); assert.notEqual(hit(stacked,outside)?.kind,'wire');
  assert.equal(hit(stacked,outside)?.node,'B'); assert.equal(hit(stacked,outside)?.kind,'resize');
  assert.ok(G.inspect(stacked,view,outside).candidates.some(n=>n.id===wire.id&&n.occluded));
  assert.equal(A.resolve(layout,stacked,view,inside,G,outside).winner?.kind,'resize');
});
test('compact overlapping foreground wires retain an interior region at their actual host edge',()=>{
  const s=state([node('A',-100,-100,200,200),node('B',-28,-28,56,56)]), l=G.build(s,view);
  const wire=l.zones.find(n=>n.id==='B:wire:out'); assert.ok(wire);
  const host=l.cards.find(n=>n.node.id==='B'); assert.equal(wire.anchor.x,host.r.x+host.r.w);
  assert.equal(hit(s,{x:wire.anchor.x-1,y:wire.anchor.y})?.id,wire.id);
});
test('matching adjacent corner zones choose the nearest host on both sides regardless of draw order',()=>{
  for(const reverse of [false,true]) {
    const nodes=[node('A',-100,0),node('B',0,0)], s=state(reverse?nodes.reverse():nodes), l=G.build(s,view);
    assert.equal(l.cardMap.get('A').overlaps.has('B'),false);
    for(const y of [-5,-2,2]) for(const x of [-3,-1,1,3]) {
      const winner=hit(s,screen({x,y})); assert.equal(winner?.node,x<0?'A':'B'); assert.equal(winner?.kind,'resize');
    }
    assert.equal(hit(s,screen({x:0,y:-3}))?.node,reverse?'A':'B');
  }
});
test('adjacent corner ownership follows rotated geometry at multiple zoom levels',()=>{
  const angle=33;
  for(const z of [0.5,1,2,4]) {
    // Keep corners coincident at every zoom; compact expanded cages instead
    // have distinct corners and are covered by the existing compact-frame tests.
    const v={...view,z}, nodes=[node('A',-240,0,240,240),node('B',0,0,240,240)].map(n=>{
      const p=G.rotatePoint({x:n.x+120,y:n.y+120},{x:0,y:0},angle);
      return {...n,x:p.x-120,y:p.y-120,rotation:angle};
    });
    const s=state(nodes), l=G.build(s,v); assert.equal(l.cardMap.get('A').overlaps.has('B'),false);
    assert.ok(G.distance(l.zones.find(n=>n.id==='A:resize:ne').anchor,l.zones.find(n=>n.id==='B:resize:nw').anchor)<1e-8);
    for(const x of [-2,2]) {
      const p=G.rotatePoint({x:x/z,y:-3/z},{x:0,y:0},angle), winner=hit(s,screen(p,v),v);
      assert.equal(winner?.node,x<0?'A':'B'); assert.equal(winner?.kind,'resize');
    }
  }
});
test('overlap classification excludes contact and rounded-corner empty space',()=>{
  const cards=nodes=>nodes.map(n=>({node:n,r:G.screenRect(n,view),origin:G.center(G.screenRect(n,view))}));
  let [a,b]=cards([node('A',0,0),node('B',100,0)]); assert.equal(G.bodiesOverlap(a,b,1),false);
  [a,b]=cards([node('A',0,0),node('B',99.9,0)]); assert.equal(G.bodiesOverlap(a,b,1),true);
  [a,b]=cards([{...node('A',0,0),radius:50},{...node('B',90,90),radius:50}]);
  assert.equal(G.bodiesOverlap(a,b,1),false);
  assert.ok(Math.abs(G.bodyDistance(a,screen({x:100,y:100}),1)-(Math.hypot(50,50)-50))<1e-8);
});
test('fast painted ownership and detailed input ownership agree for stacked and adjacent objects',()=>{
  for(const s of [state([node('A',-120,-100,240,200),node('B',-50,-50)]),state([node('A',-100,0),node('B',0,0)])]) {
    const layout=G.build(s,view);
    for(let y=-90;y<=110;y+=7) for(let x=-110;x<=110;x+=7) {
      const p=screen({x,y}); assert.equal(A.resolve(layout,s,view,p,G,p,false).winner?.id,hit(s,p)?.id);
    }
  }
});
