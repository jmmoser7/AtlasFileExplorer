/* Text-box review surface. Shared agent controls and history are owned by the Studio runtime. */
(() => {
  const seed = 'The courtyard should provide a quiet place to pause within the working day. A clear path connects the entrance to the shared table, while the planting creates a softer edge beside the reading seats. Keep the centre open so that people can understand the space at a glance.\n\nThe northern edge has room for two small trees. Their canopies should create shade without blocking light to the adjacent rooms. Before fixing their positions, check the mature canopy, the available soil volume and the route used for maintenance.\n\nThe seating should support both a brief conversation and an hour of focused reading. Use a short permanent bench near the entrance, two movable chairs in the quieter corner and a table that can accommodate four people. Furniture should remain outside the main walking route, including when every seat is occupied.\n\nFor the next review, compare a morning visit, a shared lunch and a late-afternoon reading session. Each should feel comfortable without moving the planting or adding temporary barriers.';
  const initialize = node => {node.data ||= {}; if(typeof node.data.text !== 'string')node.data.text=seed; node.data.textRevision ||= 0;};
  function validSelection(node,selection=node.selection){
    return !!selection && Number.isInteger(selection.start) && Number.isInteger(selection.end) && selection.end>selection.start && selection.start>=0 && selection.end<=node.data.text.length && node.data.text.slice(selection.start,selection.end)===selection.text && (selection.revision==null||selection.revision===node.data.textRevision);
  }
  function rangeAt(editor,start,end){
    const walker=document.createTreeWalker(editor,NodeFilter.SHOW_TEXT);let offset=0,first=null,last=null,text;
    while((text=walker.nextNode())){const next=offset+text.length;if(first===null&&start<=next)first=[text,Math.max(0,start-offset)];if(end<=next){last=[text,Math.max(0,end-offset)];break;}offset=next;}
    if(!first||!last)return null;const range=document.createRange();range.setStart(...first);range.setEnd(...last);return range;
  }
  function concise(text){
    const cleaner=text.replace(/\bshould provide\b/gi,'provides').replace(/\bshould create\b/gi,'creates').replace(/\bshould remain\b/gi,'must remain').replace(/\bin order to\b/gi,'to').replace(/\bhas room for\b/gi,'can fit').replace(/\bat a glance\b/gi,'quickly');
    if(cleaner!==text)return cleaner;
    const sentences=text.match(/[^.!?]+[.!?]?/g)||[text];return sentences.length>1?sentences.slice(0,Math.max(1,sentences.length-1)).join('').trim():text.replace(/\bvery\s+|\breally\s+|\bsimply\s+/gi,'').trim();
  }
  function renderPreview({node,make,update,setSelection}){
    initialize(node);
    const root=make('section','text-preview');
    const sheet=make('div','text-preview-sheet'),editor=make('div','text-preview-editor');editor.contentEditable='true';editor.spellcheck=false;editor.setAttribute('role','textbox');editor.setAttribute('aria-label','Text document');editor.setAttribute('aria-multiline','true');editor.textContent=node.data.text;sheet.append(editor);root.append(sheet);
    const highlightKey='text-agent-'+String(node.id||'document').replace(/[^a-z0-9-]/gi,'');
    const highlightStyle=make('style');highlightStyle.textContent=`::highlight(${highlightKey}){background:color-mix(in srgb,var(--accent) 22%,transparent);color:inherit}`;root.append(highlightStyle);
    const paintSelection=()=>{if(!CSS.highlights||typeof Highlight==='undefined')return;CSS.highlights.delete(highlightKey);if(validSelection(node)){const range=rangeAt(editor,node.selection.start,node.selection.end);if(range)CSS.highlights.set(highlightKey,new Highlight(range));}};
    const capture=()=>{
      const selection=window.getSelection();if(!selection||selection.isCollapsed||!selection.rangeCount)return;
      const range=selection.getRangeAt(0);if(!editor.contains(range.startContainer)||!editor.contains(range.endContainer))return;
      const before=range.cloneRange();before.selectNodeContents(editor);before.setEnd(range.startContainer,range.startOffset);const start=before.toString().length,end=start+range.toString().length,text=node.data.text.slice(start,end);
      if(!text.trim()||start===end)return;
      setSelection({text,start,end,revision:node.data.textRevision});paintSelection();
    };
    editor.addEventListener('pointerup',capture);editor.addEventListener('keyup',e=>{if(e.key.startsWith('Arrow')||e.key==='Shift'||e.key==='Home'||e.key==='End')capture();});
    editor.addEventListener('input',()=>{node.data.text=editor.textContent;node.data.textRevision++;node.data.review=null;setSelection(null);CSS.highlights?.delete(highlightKey);update({render:false});});
    editor.addEventListener('blur',()=>update({render:false}));
    editor.addEventListener('paste',e=>{e.preventDefault();document.execCommand('insertText',false,e.clipboardData.getData('text/plain'));});
    editor.addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();document.execCommand('insertText',false,'\n');}});
    paintSelection();window.StudioScroll?.attach(editor,sheet);
    const review=node.data.review;
    if(review){
      const box=make('section','text-preview-review');box.setAttribute('aria-label',review.kind==='edit'?'Proposed text revision':'Passage note');
      const label=make('span','text-preview-review-label',review.kind==='edit'?'Proposed revision':'Passage note');box.append(label,make('p','text-preview-review-copy',review.text));
      const actions=make('div','text-preview-review-actions');
      if(review.kind==='edit'){
        const apply=make('button','',review.selection?'Replace highlighted text':'Replace document');apply.type='button';apply.dataset.hint='Apply this proposed wording to the text document. Existing discussion stays in the dialogue.';
        apply.onclick=()=>{if(review.selection&&!validSelection(node,review.selection)){node.data.review={kind:'answer',text:'The selected passage changed. Highlight it again before applying a replacement.'};update();return;}
          node.data.previousText=node.data.text;node.data.text=review.selection?node.data.text.slice(0,review.selection.start)+review.text+node.data.text.slice(review.selection.end):review.text;node.data.textRevision++;node.data.review=null;setSelection(null);update();};actions.append(apply);
      }
      const dismiss=make('button','','Dismiss');dismiss.type='button';dismiss.onclick=()=>{node.data.review=null;update();};actions.append(dismiss);box.append(actions);root.append(box);
    }
    return root;
  }
  function applyResult(node,prompt){
    initialize(node);if(node.selection&&!validSelection(node)){node.data.review={kind:'answer',text:'The highlighted passage has changed. Highlight it again before asking for a revision.'};return;}const selection=validSelection(node)?{...node.selection}:null,source=selection?selection.text:node.data.text;
    const wantsEdit=/\b(rewrite|rephrase|shorten|concise|simplify|improve|edit|clarify|tighter|clearer)\b/i.test(prompt);
    if(wantsEdit)node.data.review={kind:'edit',selection,text:concise(source)};
    else node.data.review={kind:'answer',text:selection?'This passage sets a design requirement. Its key point is '+source.split(/[.!?]/)[0].trim().replace(/^The /,'the ')+'. Check that the drawing and the next revision make that requirement easy to verify.':'The brief connects circulation, planting and seating. Review the walking route first, then check that shade and furniture support each of the three daily-use scenarios.'};
  }
  window.StudioElements ||= [];
  window.StudioElements.push({id:'text-agent',title:'Text Agent',width:650,height:390,initialPrompt:'Make the highlighted passage clearer and more concise.',initialTurns:[{role:'user',text:'Help me refine this courtyard brief while preserving its design intent.'},{role:'assistant',text:'Highlight a passage in the document and ask about it below. I can discuss the wording or propose a replacement for that passage.'}],stages:[{label:'Read selected passage',detail:'Capture the exact text range and its document revision.'},{label:'Review wording',detail:'Check the requested change against the surrounding brief.'},{label:'Prepare response',detail:'Return a passage note or a proposed edit. The document changes only when you apply an edit.'}],renderPreview,applyResult,tools:[{id:'undo-text-edit',label:'Undo text edit',hint:'Restore the previous document wording; conversation history is retained.',action(node){initialize(node);if(typeof node.data.previousText==='string'){const current=node.data.text;node.data.text=node.data.previousText;node.data.previousText=current;node.data.textRevision++;node.data.review=null;node.selection=null;}}}]});
})();
