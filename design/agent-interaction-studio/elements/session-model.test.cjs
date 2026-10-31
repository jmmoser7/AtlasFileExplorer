const assert=require('node:assert/strict');
const M=require('./session-model.js');
const config={width:700,height:400,initialPrompt:'Refine this',initialTurns:[{role:'assistant',text:'Ready'}]};
const parent=M.create(config);parent.data={nested:{value:1}};parent.grafted={style:true};parent.versions=[{old:true}];parent.turns.push({role:'user',text:'Original'});
const child=M.branch(parent,'child');assert.equal(child.inherited.length,2);assert.equal(child.turns.length,0);child.data.nested.value=2;assert.equal(parent.data.nested.value,1);
assert.equal(child.open,false,'a downstream window must not open menus on creation');assert.equal(child.selected,false);assert.equal(child.blank,true);
assert.deepEqual(child.grafted,{},'a new child must not inherit graft flags for unconnected inputs');assert.deepEqual(child.versions,[],'a blank child starts its own result history');
const selected={text:'selected',start:1,end:9,revision:3};assert.equal(M.begin(child,'Explain it',selected),true);selected.text='changed';assert.equal(child.turns[0].selection.text,'selected');assert.equal(M.begin(child,'Duplicate run'),false);
M.finish(child,'Done');assert.equal(child.revision,1);assert.equal(child.running,false);assert.equal(child.turns.length,2);
M.cut(child);assert.equal(child.inherited.length,0);assert.equal(child.preserve,false);assert.equal(child.turns.length,2);assert.equal(M.contextTurns(child).length,0,'cut retains display transcript but clears active context');assert.equal(child.archived.length,4);
child.preserve=true;assert.equal(M.branch(child,'fresh').inherited.length,0,'re-enabling preservation must not resurrect cut history');M.begin(child,'Start fresh');M.finish(child,'New reply');assert.deepEqual(M.contextTurns(child).map(t=>t.text),['Start fresh','New reply']);assert.equal(M.branch(child,'new-context').inherited.length,2);
parent.preserve=false;const independent=M.branch(parent,'independent');assert.equal(independent.inherited.length,0);assert.equal(independent.parent,null,'no history wire without preservation');assert.equal(M.begin(parent,' '),false);
console.log('Element sessions: independent branch snapshots, selected-text context, run guard and history cuts passed.');

const original={data:{variant:5,nested:{safe:true}}};const alternatives=M.options(original,3,candidate=>{candidate.data.variant++;candidate.data.nested.safe=false;});assert.deepEqual(alternatives.map(d=>d.variant),[6,7,8]);assert.equal(original.data.nested.safe,true);assert.notEqual(alternatives[0].nested,alternatives[1].nested);
