import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {boardLinkGuide,boardLinkPrompt} from './board-link.mjs';
const req={id:'r',prompt:'Organize this board',collaborator:{session_id:'s',grant_id:'g',workspace_id:'w'}};
test('only an explicit collaborator receives shared bridge instructions',()=>{
  assert.equal(boardLinkGuide({prompt:'hello'}),'');
  const prompt=boardLinkPrompt(req,'C:/quoted "folder"');
  assert.ok(prompt.startsWith(req.prompt));
  assert.ok(prompt.includes('C:/quoted \\"folder\\"'));
  assert.ok(prompt.includes('"request_id":"r"'));
  assert.ok(prompt.includes('"offset":0,"limit":32'));
  assert.ok(prompt.includes('next_offset'));
  assert.ok(!prompt.includes('place.json'));
  const privateValues=[
    'PRIVATE-AMBIENT-SELECTION',
    'PRIVATE-AMBIENT-VIEWPORT',
    'PRIVATE-AMBIENT-REVISION',
    'PRIVATE-AMBIENT-BEACON',
    'PRIVATE-AMBIENT-INPUT-CONTEXT',
  ];
  const ambient={
    ...req,
    selection:[privateValues[0]],
    viewport:{description:privateValues[1]},
    revision:privateValues[2],
    beacon:{note:privateValues[3]},
    inputs:{context:[{node:1,text:privateValues[4],images:[]}],wired:[]},
  };
  assert.equal(boardLinkPrompt(ambient,'C:/quoted "folder"'),prompt);
  for(const value of privateValues)assert.ok(!prompt.includes(value));
  const profile=readFileSync(new URL('./board-link-v1.md',import.meta.url),'utf8');
  assert.ok(profile.startsWith('You are a scoped Slate board collaborator.'));
  assert.ok(!/\{\{[a-z_]+\}\}/.test(prompt));
  assert.ok(boardLinkGuide(req,'C:/literal-{{session}}').includes('C:/literal-{{session}}'));
});
test('malformed and incompatible bindings are refused',()=>{
  assert.throws(()=>boardLinkGuide({...req,oneshot:true}));
  assert.throws(()=>boardLinkGuide({...req,id:'../r'}));
  assert.throws(()=>boardLinkGuide({...req,collaborator:{...req.collaborator,grant_id:''}}));
});
