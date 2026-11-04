const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs'),vm=require('node:vm');
const context={URLSearchParams};vm.createContext(context);
vm.runInContext(fs.readFileSync(__dirname+'/studio-model.js','utf8'),context);
const M=context.StudioPortalModel;
test('one element registry migrates custom tabs without changing their data',()=>{
  const saved={tabs:[{id:'chat-train',kind:'blank',name:'duplicate'},null,{id:'element-kept',kind:'blank',name:'Kept',draft:{text:'retain'}},{id:'element-kept',kind:'blank',name:'duplicate'},{id:'studio',kind:'blank',name:'duplicate'}]};
  const before=JSON.stringify(saved),tabs=M.elements(saved);
  assert.equal(JSON.stringify(saved),before);assert.equal(tabs[0].id,'chat-train');assert.equal(tabs.at(-1).id,'studio');
  assert.equal(tabs.filter(t=>t.id==='element-kept').length,1);assert.equal(tabs.find(t=>t.id==='element-kept').draft.text,'retain');
  assert.equal(new Set(tabs.map(t=>t.id)).size,tabs.length);
});
test('home is default; old element URLs continue to select JS phase',()=>{
  const tabs=M.elements(null);assert.equal(M.route('',tabs).phase,null);
  for(const id of ['image-generator','frame','chat-train']){const r=M.route('?tab='+id,tabs);assert.equal(r.phase,'01');assert.equal(r.tab,id);}
  assert.equal(M.route('?phase=invalid&tab=frame',tabs).phase,null);
});
test('every stage accepts every element without a prerequisite',()=>{
  const tabs=M.elements({tabs:[{id:'element-custom',kind:'blank',name:'Custom'}]});
  for(const p of M.phases)for(const t of tabs){const r=M.route(`?phase=${p.id}&tab=${t.id}`,tabs);assert.equal(r.phase,p.id);assert.equal(r.tab,t.id);}
});
