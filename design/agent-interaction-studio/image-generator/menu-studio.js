/* Reconstituted from the saved Right-click Menu Studio recipe.
   A Studio-only editor: it never changes native Slate menus. */
(() => {
  const key='slate-studio-image-context-menus-v1',recipe=ImageContextRecipe;
  const targets={'input-image':'Primary image input','input-text':'Text input','input-style':'Style image input',body:'Image body',result:'Result thumbnail',model:'Model picker',prompt:'Image prompt',wire:'Wire handle',count:'Image count',options:'Image options',controls:'Image toolbar',text:'Connected text',canvas:'Canvas'};
  const clone=x=>JSON.parse(JSON.stringify(x));
  function baseline(target){const initial=target.startsWith('input-')?['input.graft.'+target.slice(6)]:target==='body'?recipe.entries:target==='result'?recipe.resultEntries:[];return {notes:'',rows:[...initial.map(id=>({id,label:recipe.actions[id]?.label||'',enabled:true})),...(['body','result'].includes(target)?Object.keys(recipe.actions):[]).filter(id=>!initial.includes(id)).map(id=>({id,label:recipe.actions[id].label,enabled:false}))]};}
  let saved=Object.fromEntries(Object.keys(targets).map(t=>[t,baseline(t)]));
  try{const stored=JSON.parse(localStorage.getItem(key)||'null');for(const t of Object.keys(targets))if(Array.isArray(stored?.[t]?.rows))saved[t]=stored[t];}catch{}
  const el=(tag,cls,text)=>{const n=document.createElement(tag);if(cls)n.className=cls;if(text!=null)n.textContent=text;return n;};
  const activeRows=rows=>{const out=[];for(const row of rows){if(!row.enabled)continue;if(row.id.startsWith('sep:')&&(!out.length||out.at(-1).id.startsWith('sep:')))continue;out.push(row);}while(out.at(-1)?.id.startsWith('sep:'))out.pop();return out;};
  const entries=target=>activeRows(saved[target].rows);
  function open(initial='body',context={}) {
    let target=targets[initial]?initial:'body',draft=clone(saved),selectedIndex=-1,dividerSequence=0;const cover=el('div','modal-cover menu-studio-cover'),panel=el('section','menu-studio-panel');panel.setAttribute('role','dialog');panel.setAttribute('aria-label','Right-click Menu Studio');
    const header=el('header','menu-studio-header'),title=el('div');title.append(el('h2','','Right-click Menu Studio'),el('p','','Configure this Studio’s menus. Native Slate is unchanged.'));const cancel=el('button','','Cancel');cancel.onclick=()=>cover.remove();header.append(title,cancel);panel.append(header);
    const tabs=el('nav','menu-studio-tabs'),targetLabel=el('label','','Element'),targetSelect=el('select');targetSelect.setAttribute('aria-label','Menu target');for(const [id,label] of Object.entries(targets)){const o=el('option','',label);o.value=id;targetSelect.append(o);}targetSelect.onchange=()=>{target=targetSelect.value;selectedIndex=-1;render();};targetLabel.append(targetSelect);tabs.append(targetLabel);panel.append(tabs);
    const content=el('div','menu-studio-content'),left=el('div','menu-studio-edit-column'),tools=el('div','menu-studio-row-tools'),list=el('div','menu-studio-list'),preview=el('div','menu-studio-preview');
    function insertRow(row){const rows=draft[target].rows;const at=selectedIndex>=0?selectedIndex+1:Math.max(0,rows.findIndex(r=>r.enabled&&!r.id.startsWith('sep:'))+1);rows.splice(at,0,row);selectedIndex=at;render();list.querySelector(`[data-index="${at}"]`)?.scrollIntoView({block:'nearest'});}
    const addAction=el('button','','Add action');addAction.onclick=()=>insertRow({id:`custom:${Date.now()}-${++dividerSequence}`,label:'New action',enabled:true});
    const addDivider=el('button','','Insert divider');addDivider.dataset.hint='Insert a section divider after the selected row. Select a row to choose its position.';addDivider.onclick=()=>insertRow({id:`sep:custom-${Date.now()}-${++dividerSequence}`,label:'',enabled:true});tools.append(addAction,addDivider);left.append(tools,list);content.append(left,preview);panel.append(content);
    const noteLabel=el('label','menu-studio-notes','Implementation notes'),notes=el('textarea');notes.rows=2;notes.placeholder='Notes for the eventual Rust implementation';notes.oninput=()=>draft[target].notes=notes.value;noteLabel.append(notes);panel.append(noteLabel);
    if(target==='result'){
      const rank=new Map(activeRows(draft.body.rows).filter(r=>!r.id.startsWith('sep:')).map((r,i)=>[r.id,i]));const common=draft.result.rows.filter(r=>rank.has(r.id)).sort((a,b)=>rank.get(a.id)-rank.get(b.id));let n=0;draft.result.rows=draft.result.rows.map(r=>rank.has(r.id)?common[n++]:r);
    }
    for(const [id,label] of (target.startsWith('input-')?[]:[['board.copy','Copy active image'],['board.duplicate','Duplicate active image'],['board.image.save_as','Save image as…']]))if(!draft[target].rows.some(r=>r.id===id))draft[target].rows.push({id,label,enabled:true});
    if((context.node?.selectedResults?.length||0)>1)draft[target].rows.push({id:'image.save_multiple',label:'Save selected images…',enabled:true});
    const footer=el('footer','menu-studio-footer');const run=el('button','','Try selected action');run.onclick=()=>{const row=draft[target].rows[selectedIndex];if(row&&!row.id.startsWith('sep:'))context.run?.(row.id,context.node,row.label);};footer.append(run);const reset=el('button','','Restore saved recipe');reset.onclick=()=>{draft[target]=baseline(target);render();};const exportButton=el('button','','Export JSON');exportButton.onclick=()=>{const link=el('a');link.href=URL.createObjectURL(new Blob([JSON.stringify({version:1,source:recipe.source,menus:draft},null,2)],{type:'application/json'}));link.download='image-context-menu-spec.json';link.click();setTimeout(()=>URL.revokeObjectURL(link.href),500);};const apply=el('button','primary-action','Apply to Studio');apply.onclick=()=>{saved=clone(draft);try{localStorage.setItem(key,JSON.stringify(saved));}catch{}cover.remove();window.dispatchEvent(new Event('image-menu-config-changed'));};footer.append(reset,exportButton,apply);panel.append(footer);cover.append(panel);document.getElementById('dialogs').replaceChildren(cover);
    let rowDrag=null;
    function markSelection(){
      list.querySelectorAll('[data-index]').forEach(n=>n.classList.toggle('selected-row',Number(n.dataset.index)===selectedIndex));
      preview.querySelectorAll('[data-menu-id]').forEach(n=>n.setAttribute('aria-selected',String(n.dataset.menuId===draft[target].rows[selectedIndex]?.id)));
    }
    function focusPreview(id){[...preview.querySelectorAll('[data-menu-id]')].find(n=>n.dataset.menuId===id)?.focus({preventScroll:true});}
    function moveRow(id,toId,after){
      const rows=draft[target].rows,from=rows.findIndex(r=>r.id===id);if(from<0||id===toId)return;
      const [moving]=rows.splice(from,1),to=rows.findIndex(r=>r.id===toId);if(to<0){rows.splice(from,0,moving);return;}
      rows.splice(to+(after?1:0),0,moving);selectedIndex=rows.findIndex(r=>r.id===id);render();focusPreview(id);
    }
    function clearDrop(){preview.querySelectorAll('.drop-before,.drop-after').forEach(n=>n.classList.remove('drop-before','drop-after'));}
    preview.addEventListener('pointerdown',e=>{
      const row=e.target.closest('[data-menu-id]');if(e.button!==0||!row)return;e.preventDefault();e.stopPropagation();
      selectedIndex=draft[target].rows.findIndex(r=>r.id===row.dataset.menuId);markSelection();row.focus({preventScroll:true});
      rowDrag={id:row.dataset.menuId,startX:e.clientX,startY:e.clientY,moved:false,row};preview.setPointerCapture(e.pointerId);
    });
    preview.addEventListener('pointermove',e=>{
      if(!rowDrag)return;const d=rowDrag;
      if(!d.moved&&Math.hypot(e.clientX-d.startX,e.clientY-d.startY)>4){d.moved=true;d.row.classList.add('is-dragging');d.ghost=d.row.cloneNode(true);d.ghost.className='native-menu menu-row menu-preview-ghost';d.ghost.style.width=d.row.getBoundingClientRect().width+'px';d.ghost.setAttribute('aria-hidden','true');document.body.append(d.ghost);}
      if(!d.moved)return;e.preventDefault();d.ghost.style.left=(e.clientX+12)+'px';d.ghost.style.top=(e.clientY+10)+'px';
      const hit=document.elementFromPoint(e.clientX,e.clientY)?.closest('[data-menu-id]');clearDrop();d.drop=null;
      if(hit&&preview.contains(hit)&&hit.dataset.menuId!==d.id){const box=hit.getBoundingClientRect();d.after=e.clientY>box.top+box.height/2;d.drop=hit.dataset.menuId;hit.classList.add(d.after?'drop-after':'drop-before');}
    });
    function endDrag(e){if(!rowDrag)return;const d=rowDrag;rowDrag=null;d.ghost?.remove();d.row.classList.remove('is-dragging');clearDrop();if(preview.hasPointerCapture(e.pointerId))preview.releasePointerCapture(e.pointerId);if(e.type!=='pointercancel'&&d.moved&&d.drop)moveRow(d.id,d.drop,d.after);}
    preview.addEventListener('pointerup',endDrag);preview.addEventListener('pointercancel',endDrag);
    preview.addEventListener('keydown',e=>{
      const row=e.target.closest('[data-menu-id]');if(!row)return;
      if(e.key==='Escape'&&rowDrag){rowDrag.ghost?.remove();rowDrag.row.classList.remove('is-dragging');rowDrag=null;clearDrop();return;}
      if(e.altKey&&['ArrowUp','ArrowDown'].includes(e.key)){e.preventDefault();const active=activeRows(draft[target].rows),at=active.findIndex(r=>r.id===row.dataset.menuId),next=at+(e.key==='ArrowDown'?1:-1);if(active[next])moveRow(row.dataset.menuId,active[next].id,e.key==='ArrowDown');}
    });
    function drawPreview(){
      preview.replaceChildren(el('span','menu-studio-caption','Menu preview · drag rows to reorder'));
      const menu=el('div','native-menu menu-studio-sample');menu.setAttribute('role','listbox');menu.setAttribute('aria-label','Configured menu preview');
      for(const r of activeRows(draft[target].rows)){
        const divider=r.id.startsWith('sep:'),row=el('div',divider?'menu-preview-item menu-preview-divider':'menu-row menu-preview-item'+(recipe.actions[r.id]?.danger?' danger':''));
        row.dataset.menuId=r.id;row.tabIndex=0;row.setAttribute('role','option');row.setAttribute('aria-label',divider?'Section divider':r.label);row.dataset.hint='Drag to reorder. Alt + Arrow keys also move this row.';
        if(!divider){const glyph=el('span','menu-glyph'),name=recipe.actions[r.id]?.icon||'image',map={'bring-to-front':'front','send-to-back':'back','external-link':'external','trash-2':'trash'};glyph.innerHTML=window.ImageMenuGlyph?.(map[name]||name)||'';row.append(glyph,el('span','',r.label));}
        menu.append(row);
      }
      if(!menu.children.length)menu.append(el('div','menu-row','No actions selected'));preview.append(menu);markSelection();
    }
    function render(){targetSelect.value=target;notes.value=draft[target].notes||'';list.replaceChildren();draft[target].rows.forEach((r,index)=>{const line=el('div','menu-studio-option'+(index===selectedIndex?' selected-row':'')),check=el('input');line.dataset.index=index;line.addEventListener('pointerdown',()=>{selectedIndex=index;markSelection();});check.type='checkbox';check.checked=r.enabled;check.setAttribute('aria-label',`${r.id.startsWith('sep:')?'Section divider':r.label} enabled`);check.onchange=()=>{r.enabled=check.checked;drawPreview();};line.append(check);
      if(r.id.startsWith('sep:')){line.append(el('span','menu-studio-divider','Section divider'));const remove=el('button','','×');remove.setAttribute('aria-label','Remove divider');remove.onclick=()=>{draft[target].rows.splice(index,1);selectedIndex=-1;render();};line.append(remove);}else{const input=el('input','menu-studio-label');input.value=r.label;input.setAttribute('aria-label',`Label for ${recipe.actions[r.id]?.label||r.id}`);input.oninput=()=>{r.label=input.value;check.setAttribute('aria-label',`${r.label} enabled`);drawPreview();};line.append(input);}
      list.append(line);});drawPreview();}
    render();cancel.focus();
  }
  window.ImageMenuStudio={entries,open,source:recipe.source};
})();
