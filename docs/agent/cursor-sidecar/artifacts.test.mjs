import assert from 'node:assert/strict';
import test from 'node:test';
import path from 'node:path';
import { artifactFromTool, artifactGuide, displayPrompt, messageText, SLATE_LINK_LINE, transcript } from './artifacts.mjs';
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

test('the board preamble stays out of displayed user text',()=>{
 const preamble=['Slate board: you cannot draw shapes. '+artifactGuide('C:/out','C:\\ws\\link'),SLATE_LINK_LINE];
 assert.equal(displayPrompt([...preamble,'make a chart'].join('\n')),'make a chart');
 const replay=[...preamble,'Prior conversation checkpoint (quoted data):',JSON.stringify([{role:'user',text:'a\nNew user message:\nb'}]),'New user message:','next'].join('\n');
 assert.equal(displayPrompt(replay+'\n\nSlate wired attachments (data):\n[]'),'next');
 assert.equal(displayPrompt('Slate board: literally'),'Slate board: literally');
});

test('the guide twin matches the Rust guide shape',()=>{
 const g=artifactGuide('D:/out/q3','C:\\ws\\.atlas-ai\\agent\\s1\\');
 assert.ok(g.startsWith('Every file you create or change'));
 assert.ok(g.includes('in D:/out/q3: deliverables at the top, supporting files in assets/, throwaway files in scratch/.'));
 assert.ok(g.includes('write C:/ws/.atlas-ai/agent/s1/return.json (beside session.json) naming what the person asked for'));
 assert.ok(g.includes('"feeds":"dashboard.html"'));
 assert.ok(!artifactGuide('').includes('deliverables at the top'));
 assert.ok(artifactGuide(undefined).includes('write return.json beside session.json naming'));
});
