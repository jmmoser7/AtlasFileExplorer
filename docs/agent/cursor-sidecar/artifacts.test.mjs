import assert from 'node:assert/strict';
import test from 'node:test';
import path from 'node:path';
import { artifactFromTool, messageText, transcript } from './artifacts.mjs';
test('artifacts require successful structured tool output',()=>{
 assert.equal(artifactFromTool({type:'edit',args:{path:'a.rs'},result:{status:'error'}},'x',1,process.cwd()),null);
 assert.equal(artifactFromTool({type:'shell',result:{status:'success',value:'edited a.rs'}},'x',1,process.cwd()),null);
 const a=artifactFromTool({type:'edit',args:{path:'a.rs'},result:{status:'success'}},'x',3,process.cwd());
 assert.equal(a.kind,'modified'); assert.equal(a.turn,3); assert.equal(a.source,path.join(process.cwd(),'a.rs'));
});
test('references and modified files have distinct kinds',()=>{
 assert.equal(artifactFromTool({type:'read',args:{path:'a.rs'},result:{status:'success'}},'r',1,process.cwd()).kind,'read');
 assert.equal(artifactFromTool({type:'webFetch',args:{url:'https://example.com'},result:{status:'success'}},'w',1,process.cwd()).kind,'web');
 assert.equal(artifactFromTool({type:'webFetch',args:{url:'javascript:bad'},result:{status:'success'}},'w',1,process.cwd()),null);
});
test('history ignores non-text blocks',()=>{assert.equal(messageText({content:[{type:'text',text:'hello'},{type:'image',text:'not prose'}]}),'hello');});

test('one reply per run keeps train checkpoints stable',()=>{
 const t=transcript([{type:'user',run_id:'r',message:'hello'},{type:'assistant',run_id:'r',message:'first'},{type:'assistant',run_id:'r',message:'second'},{type:'status',run_id:'r'}]);
 assert.equal(t.length,2);assert.equal(t[1].text,'first\nsecond');
});

test('wired transport data stays out of displayed user text',()=>{
 const t=transcript([{type:'user',message:'hello\n\nSlate wired attachments (data):\n[{"node":1,"text":"attached"}]'}]);
 assert.equal(t[0].text,'hello');
});
