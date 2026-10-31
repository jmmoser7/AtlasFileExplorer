const assert=require('node:assert/strict');
const {layout,bezier,ports,notch,resultGrid,portRadius,portOffset}=require('./layout.js');
// Native captured fixture geometry, including the exact 504/552 flat-edge thresholds.
for(const [w,h,connected,attached] of [[220,260,false,false],[360,360,false,false],[680,300,false,true],[680,300,true,true],[519,300,false,false],[520,300,false,true],[567,300,true,false],[568,300,true,true],[680,30,false,false]]) {
  const r=layout(w,h,connected,true,8);
  assert.equal(r.attached,attached,`${w}×${h} / connected=${connected}`);
  assert.equal(r.panelWidth,attached?w:420);
  assert.equal(r.panelY,attached?h:h+52);
  assert.equal(r.buttons.length,connected?8:7);
  const fields=['model','more','history','count','play'];
  for(let i=0;i<fields.length;i++) {
    const a=r.controls[fields[i]];
    assert.ok(a[0]>=8 && a[0]+a[2]<=r.panelWidth-8);
    for(let j=i+1;j<fields.length;j++) {
      const b=r.controls[fields[j]];
      assert.ok(a[0]+a[2]<=b[0]||b[0]+b[2]<=a[0]||a[1]+a[3]<=b[1]||b[1]+b[3]<=a[1],`${fields[i]} overlaps ${fields[j]}`);
    }
  }
}
assert.deepEqual(layout(680,300,false,true).controls.prompt,[8,28,664,32]);
assert.equal(bezier([0,0],[100,80]),'M0,0 C60,0 40,80 100,80');
const compact=layout(220,260,false,true,8);
assert.equal(compact.controls.history[1]+13,compact.controls.model[1]+12);
assert.ok(compact.controls.history[2]>=64);
console.log('Image fixture geometry and shared header datum: passed');

const u=portOffset/portRadius,outside=.5+(Math.asin(u)+u*Math.sqrt(1-u*u))/Math.PI;
assert.ok(Math.abs(outside-.65)<.0001,'wire handle should put 65% of its area outside');
assert.equal(ports(220)[0][0],-portOffset);assert.equal(ports(220)[3][0],220+portOffset);
assert.ok(notch(220,30).includes('C'));assert.ok(notch(220,30).includes('A7.4,7.4'));
const bond=resultGrid(220,12);assert.equal(bond.columns,8);assert.equal(bond.rows,2);assert.equal(bond.width,220);assert.equal(bond.height,52);assert.deepEqual(bond.positions[0],{x:0,y:0});assert.deepEqual(bond.positions[8],{x:12,y:28});assert.deepEqual(resultGrid(220,24).positions[16],{x:0,y:56});
assert.equal(layout(220,260,false,true,12).controls.more[0],layout(220,260,false,true,12).controls.model[0]+layout(220,260,false,true,12).controls.model[2]+1);
