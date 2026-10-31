const assert=require('node:assert/strict'),L=require('./text-links.js');
const text='First. Shade trees. Last.',a={id:'a',start:7,end:18,text:'Shade trees',response:'Answer'};
assert.equal(L.valid(text,a),true);assert.equal(L.valid(text,{...a,end:19}),false);
assert.deepEqual(L.rebase([a],text,'New. '+text)[0],{...a,start:12,end:23});
assert.equal(L.rebase([a],text,text.replace('trees','flowers')).length,0,'editing the linked passage invalidates its answer anchor');
assert.equal(L.rebase([a],text,text+' More.')[0].start,7,'unrelated edits preserve the link');
const node={id:'n',revision:2,selection:a,data:{text}};const linked=L.answer(node,'Why?', 'For shade.');assert.equal(linked.question,'Why?');assert.equal(L.ranges(text,node.data.answers).length,1);
L.answer(node,'More detail?', 'New response');assert.equal(node.data.answers.length,1,'a repeated query replaces the link for that passage');
assert.equal(L.ranges(text,[a,{...a,start:8}]).length,1);console.log('Text answer anchors: exact ranges, edit rebasing, overlap and replacement passed.');
