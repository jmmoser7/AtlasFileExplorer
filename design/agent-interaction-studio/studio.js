/* The existing shell is the sole route/tab owner; legacy tools remain intact. */
(() => {
  'use strict';
  const $=id=>document.getElementById(id),model=StudioPortalModel,key='slate-design-studio-tabs-v1';
  let saved;try{saved=JSON.parse(localStorage.getItem(key)||'null');}catch{}
  for(const phase of model.phases){const option=document.createElement('option');option.value=phase.id;option.textContent=`${phase.id} · ${phase.name}`;$('phase-switch').append(option);}
  let tabs=model.elements(saved),state=model.route(location.search,tabs),ready=false;
  const button=(text,cls,click)=>{const b=document.createElement('button');b.type='button';b.className=cls;b.textContent=text;b.addEventListener('click',click);return b;};
  const save=()=>{try{localStorage.setItem(key,JSON.stringify({...(saved||{}),tabs}));}catch{}};
  function navigate(phase,tab=state.tab,replace=false){
    state={phase,tab:tabs.some(t=>t.id===tab)?tab:'chat-train'};
    const url=new URL(location.href);url.searchParams.delete('phase');url.searchParams.delete('tab');
    if(phase){url.searchParams.set('phase',phase);url.searchParams.set('tab',state.tab);}
    history[replace?'replaceState':'pushState'](null,'',url);
    if(phase)try{localStorage.setItem('slate-studio-last-route-v1',JSON.stringify(state));}catch{}
    paint();
  }
  function activate(id){if(ready&&tabs.some(t=>t.id===id))navigate(state.phase||'01',id);}
  window.StudioShell={get activeId(){return state.phase==='01'?state.tab:'studio-portal-inactive';},get tabs(){return tabs.map(t=>({...t}));},get route(){return {...state};},activate,navigate,openDocument};
  const optionsButton=$('studio-options-button'),optionsMenu=$('studio-options-menu');
  const optionItems=()=>[...optionsMenu.querySelectorAll('button')].filter(b=>!b.hidden&&!b.disabled);
  optionsMenu.querySelectorAll('button').forEach(b=>b.setAttribute('role','menuitem'));
  function closeOptions(focus=false){optionsMenu.hidden=true;optionsButton.setAttribute('aria-expanded','false');if(focus)optionsButton.focus();}
  function openOptions(last=false){optionsMenu.hidden=false;optionsButton.setAttribute('aria-expanded','true');const items=optionItems();items[last?items.length-1:0]?.focus();}
  optionsButton.addEventListener('click',()=>optionsMenu.hidden?openOptions():closeOptions());
  optionsButton.addEventListener('keydown',e=>{if(['ArrowDown','ArrowUp'].includes(e.key)){e.preventDefault();openOptions(e.key==='ArrowUp');}});
  optionsMenu.addEventListener('keydown',e=>{
    const items=optionItems(),index=items.indexOf(document.activeElement);
    if(['ArrowDown','ArrowUp','Home','End'].includes(e.key)){e.preventDefault();e.stopPropagation();const next=e.key==='Home'?0:e.key==='End'?items.length-1:(index+(e.key==='ArrowDown'?1:items.length-1))%items.length;items[next]?.focus();}
    else if(e.key==='Escape'){e.preventDefault();e.stopPropagation();closeOptions(true);}
    else if(e.key==='Tab')closeOptions();
  });
  optionsMenu.addEventListener('click',e=>{if(e.target.closest('button'))closeOptions();});
  document.addEventListener('pointerdown',e=>{if(!e.target.closest('.studio-options'))closeOptions();},true);
  window.addEventListener('keydown',e=>{if(e.key==='Escape'&&!optionsMenu.hidden){e.preventDefault();closeOptions(true);}});
  const header=document.querySelector('.studio-bar');let headerHeight=0;
  function measureHeader(){
    const height=Math.ceil(header.getBoundingClientRect().height);
    if(height!==headerHeight){headerHeight=height;document.documentElement.style.setProperty('--studio-header-height',height+'px');}
    // Frames already measures its own toolbar; account for a moved (not resized) toolbar too.
    const frameToolbar=$('frame-studio-controls');
    if(frameToolbar&&!frameToolbar.hidden)document.documentElement.style.setProperty('--frame-canvas-top',Math.ceil(frameToolbar.getBoundingClientRect().bottom)+'px');
  }
  new ResizeObserver(measureHeader).observe(header);
  function clearLegacy(){
    closeOptions();
    window.ChatAgentPicker?.close();
    ['canvas','image-generator-canvas','image-studio-bottom','studio-empty'].forEach(id=>$(id).hidden=true);
    document.querySelectorAll('[data-element-studio],.studio-bottom').forEach(el=>el.hidden=true);
    $('menus').replaceChildren();$('dialogs').replaceChildren();$('hint').hidden=true;
    $('notes-panel').hidden=true;$('notes-button').setAttribute('aria-expanded','false');
  }
  function paint(){
    clearLegacy();const phase=model.phases.find(p=>p.id===state.phase),tab=tabs.find(t=>t.id===state.tab);
    document.body.dataset.studioPhase=state.phase||'home';
    $('portal-home').hidden=!!phase;$('portal-phase').hidden=!phase;
    $('phase-breadcrumb').textContent=phase?`${phase.id} · ${phase.name}`:'Build studio';
    $('phase-switch').hidden=!phase;$('phase-switch').value=state.phase||'00';
    $('studio-tab-row').hidden=!phase;$('studio-tabs').hidden=!phase;$('new-element').hidden=!phase;
    $('phase-instructions').hidden=!phase;$('debrief-button').hidden=!phase;$('notes-button').hidden=state.phase!=='01'||tab.id!=='chat-train';
    $('reference-button').hidden=state.phase!=='01'||tab.id!=='chat-train';
    renderTabs();
    const legacy=state.phase==='01'&&['chat-train','image-generator','element'].includes(tab.kind);
    if(legacy){
      $('portal-phase').hidden=true;
      if(tab.kind==='chat-train'){$('canvas').hidden=false;document.querySelector('.studio-bottom:not(#image-studio-bottom):not([data-element-studio])').hidden=false;}
      if(tab.kind==='image-generator'){$('image-generator-canvas').hidden=false;$('image-studio-bottom').hidden=false;}
      document.querySelectorAll('[data-element-studio]').forEach(el=>el.hidden=el.dataset.elementStudio!==tab.id);
    }
    // Inactive ID deliberately closes old tool menus and global input owners.
    window.dispatchEvent(new CustomEvent('studio-element-changed',{detail:{id:legacy?tab.id:'studio-portal-inactive',phase:state.phase,elementId:state.tab}}));
    if(ready)window.StudioPortalView.render({state,phase,tab,tabs,navigate,openDocument});
    requestAnimationFrame(measureHeader);
  }
  function renderTabs(){
    $('studio-tabs').replaceChildren();
    for(const tab of tabs){
      const b=button(tab.name,'studio-tab',()=>activate(tab.id));b.setAttribute('role','tab');b.setAttribute('aria-selected',String(tab.id===state.tab));b.tabIndex=tab.id===state.tab?0:-1;b.title=tab.name;
      if(tab.kind!=='blank')$('studio-tabs').append(b);
      else{const slot=document.createElement('div');slot.className='studio-tab-slot';const close=button('×','studio-tab-close',()=>{tabs=tabs.filter(t=>t.id!==tab.id);save();navigate(state.phase,state.tab===tab.id?'chat-train':state.tab);});close.setAttribute('aria-label',`Close ${tab.name} tab`);slot.append(b,close);$('studio-tabs').append(slot);}
    }
  }
  function modal(title){
    const cover=document.createElement('div');cover.className='modal-cover';const panel=document.createElement('form');panel.className='reference-dialog studio-dialog';panel.setAttribute('role','dialog');panel.setAttribute('aria-modal','true');panel.setAttribute('aria-label',title);const h=document.createElement('h2');h.textContent=title;panel.append(h);cover.append(panel);$('dialogs').replaceChildren(cover);cover.addEventListener('click',e=>{if(e.target===cover)cover.remove();});return panel;
  }
  function openDocument(path,title){
    const panel=modal(title);panel.className='reference-dialog debrief-dialog';panel.replaceChildren();
    const toolbar=document.createElement('div');toolbar.className='debrief-toolbar';const heading=document.createElement('h2');heading.textContent=title;
    const download=document.createElement('a');download.href=path;download.download=path.split('/').at(-1);download.textContent='Download document';
    toolbar.append(heading,download,button('Close','',()=>$('dialogs').replaceChildren()));const frame=document.createElement('iframe');frame.title=title;frame.src=`contract-viewer.html?document=${encodeURIComponent(path)}`;panel.append(toolbar,frame);
  }
  $('portal-home-button').addEventListener('click',()=>navigate(null));
  $('phase-switch').addEventListener('change',e=>navigate(e.target.value));
  $('phase-instructions').addEventListener('click',()=>{const p=model.phases.find(p=>p.id===state.phase);openDocument(`phases/${p.folder}/AGENTS.md`,`${p.id} · ${p.name} instructions`);});
  $('new-element').addEventListener('click',()=>{
    const form=modal('New element tab'),label=document.createElement('label'),input=document.createElement('input');label.textContent='Element name';input.id='element-name';input.required=true;input.maxLength=60;label.htmlFor=input.id;form.append(label,input);
    const actions=document.createElement('div');actions.className='dialog-actions';actions.append(button('Cancel','',()=>$('dialogs').replaceChildren()));const add=button('Create tab','',()=>{});add.type='submit';actions.append(add);form.append(actions);
    form.addEventListener('submit',e=>{e.preventDefault();const name=input.value.trim();if(!name)return;const id=`element-${Date.now().toString(36)}-${Math.random().toString(36).slice(2,6)}`;tabs.splice(tabs.length-1,0,{id,name,kind:'blank'});save();navigate(state.phase,id);});input.focus();
  });
  $('debrief-button').addEventListener('click',()=>{const tab=tabs.find(t=>t.id===state.tab);if(tab.contract)openDocument(tab.contract,`${tab.name} debrief`);else{const panel=modal(`${tab.name} debrief`),p=document.createElement('p');p.textContent='No locked decisions for this element yet.';panel.append(p,button('Close','',()=>$('dialogs').replaceChildren()));}});
  $('studio-tabs').addEventListener('keydown',e=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();const i=tabs.findIndex(t=>t.id===state.tab),next=e.key==='Home'?0:e.key==='End'?tabs.length-1:(i+(e.key==='ArrowRight'?1:tabs.length-1))%tabs.length;activate(tabs[next].id);$('studio-tabs').querySelector('[aria-selected=true]')?.focus();});
  window.addEventListener('popstate',()=>{state=model.route(location.search,tabs);paint();});
  window.addEventListener('keydown',e=>{if(e.key==='Escape'&&state.phase!=='01')$('dialogs').replaceChildren();});
  // Legacy deferred apps call activate during boot. Only the URL owner chooses startup.
  document.addEventListener('DOMContentLoaded',()=>{ready=true;paint();});
})();
