// Run after an explicitly approved rig change:
// node crates/atlas-shell/src/agent_presence/conformance.mjs
// This executes the actual browser face engine against an SVG attribute double;
// it is deterministic geometry evidence, not a browser/rendering test.
import fs from 'node:fs';
import vm from 'node:vm';
const here=new URL('.',import.meta.url);
const rig=JSON.parse(fs.readFileSync(new URL('../../assets/agent-avatar.json',here),'utf8'));
function element(){return{
  namespaceURI:'http://www.w3.org/2000/svg',attrs:{},
  setAttribute(key,value){this.attrs[key]=value;},
  querySelector(selector){return this.children[selector];},
  set innerHTML(value){this.children={
    '[data-face="left"]':element(),'[data-face="right"]':element(),'[data-face="bridge"]':element()
  };}
};}
const context=vm.createContext({
  window:{AvatarRig:rig,matchMedia:()=>({matches:false})},
  performance:{now:()=>0},requestAnimationFrame:()=>1,cancelAnimationFrame(){}
});
vm.runInContext(fs.readFileSync(new URL('../../../../design/agent-avatar-2026-10-16/avatar-engine.js',here),'utf8'),context);
function points(path){
  const numbers=path.match(/-?\d+(?:\.\d+)?/g).map(Number);
  const out=[[numbers[0],numbers[1]]];
  for(let n=0;n<39;n++)out.push(numbers.slice(2+n*6+4,2+n*6+6));
  return out;
}
const records=[];
for(const expression of rig.expressions)for(const orientation of rig.orientations){
  const svg=element(),controller=context.window.AvatarFace.create(svg);
  controller.setTarget(expression.id,orientation.id,rig.presentationDefaults.settings,{duration:0});
  records.push({
    expression:expression.id,orientation:orientation.id,eyes:controller.getPose().geometry.eyes,
    contours:[points(svg.children['[data-face="left"]'].attrs.d),points(svg.children['[data-face="right"]'].attrs.d)],
    bridge:svg.children['[data-face="bridge"]'].attrs
  });
  controller.destroy();
}
fs.writeFileSync(new URL('avatar_conformance.json',here),'[\n'+records.map(record=>JSON.stringify(record)).join(',\n')+'\n]\n');
console.log(`Recorded ${records.length} approved browser poses for native conformance.`);
