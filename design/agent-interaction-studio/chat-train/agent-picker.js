/* Local design fixture. Production must populate this catalogue from provider capabilities. */
(() => {
  'use strict';
  const {providers,labels}=window.ChatAgentCatalog;
  const paths={
    provider:'<circle cx="8" cy="8" r="6"/><path d="M5 8h6M8 5v6"/>',
    api:'<path d="M5 4L1 8l4 4M11 4l4 4-4 4M9 2L7 14"/>',
    local:'<rect x="1.5" y="2" width="13" height="9" rx="1"/><path d="M5 14h6M8 11v3"/>',
    model:'<path d="M8 1l6 3.5v7L8 15l-6-3.5v-7zM2 4.5L8 8l6-3.5M8 8v7"/>',
    effort:'<path d="M5 11c0-2-2-2-2-5a5 5 0 0 1 10 0c0 3-2 3-2 5M5 12h6M6 14h4"/>',
    speed:'<path d="M9 1L3 9h5l-1 6 6-8H8z"/>',
    key:'<circle cx="5" cy="5" r="3"/><path d="M7 7l7 7M10 10l2-2M12 12l2-2"/>',
    back:'<path d="M10 3L5 8l5 5"/>',check:'<path d="M3 8l3 3 7-7"/>',next:'<path d="M6 3l5 5-5 5"/>'
  };
  const icon=name=>name.startsWith('speed-option:')?Array.from({length:(active?.catalog||window.ChatAgentCatalog).speedInfo[name.split(':')[1]]?.bolts||0},()=>'<svg viewBox="0 0 18 18" aria-hidden="true"><path d="M10 1L3 10h5l-1 7 7-10H9z"/></svg>').join(''):['cursor','codex','openai'].includes(name)?`<span class="provider-logo logo-${name==='codex'?'openai':name}" aria-hidden="true"></span>`:`<svg viewBox="0 0 16 16" aria-hidden="true">${paths[name]||''}</svg>`;
  const el=(tag,cls,text)=>{const n=document.createElement(tag);if(cls)n.className=cls;if(text!==undefined)n.textContent=text;return n;};
  const credentialKey='slate-agent-studio-credential-status-v1';
  let active=null;
  function credential(){try{return JSON.parse(localStorage.getItem(credentialKey)||'null')?.status||'missing';}catch{return'missing';}}
  function providerFor(config,catalog=window.ChatAgentCatalog){return catalog.providers.find(p=>p.id===config.provider)||catalog.providers[0];}
  const resolve=config=>window.ChatAgentCatalog.normalize(config);
  function close(){if(!active)return;active.panel.remove();active=null;}
  function open({anchor,card,onChange,onClose,onStatus,catalog=window.ChatAgentCatalog}){
    close();
    const config=catalog.normalize(card),panel=el('div','native-menu agent-picker');
    panel.setAttribute('role','menu');panel.setAttribute('aria-label','Agent settings');
    const box=anchor.getBoundingClientRect();
    panel.style.left=`${Math.max(8,Math.min(innerWidth-240,box.left))}px`;
    const top=Math.max(8,Math.min(box.bottom+5,innerHeight-160));panel.style.top=`${top}px`;panel.style.maxHeight=`${innerHeight-top-8}px`;
    document.getElementById('menus').append(panel);
    active={panel,config,onChange,onClose,onStatus,catalog,page:'main'};
    render();
  }
  function row({label,value,iconName,action,hint,checked=false,chevron=false,disabled=false}){
    const button=el('button','menu-row agent-row');button.type='button';button.setAttribute('role','menuitem');
    button.setAttribute('aria-label',value?`${label}: ${value}`:label);button.disabled=disabled;
    const g=el('span','menu-glyph');g.innerHTML=icon(iconName);button.append(g,el('span','agent-row-label',label));
    if(value)button.append(el('span','agent-row-value',value));
    const end=el('span','agent-row-end');end.innerHTML=icon(checked?'check':chevron?'next':'');button.append(end);
    if(hint)button.dataset.hint=hint;
    if(action)button.addEventListener('click',action);
    return button;
  }
  function show(page){if(!active)return;active.page=active.page===page?'main':page;render();}
  function commit(patch){
    const old=active.config;
    const next=active.catalog.normalize({...old,...patch,...(patch.provider&&patch.provider!==old.provider?{model:'',effort:old.effort,speed:old.speed}:{})});
    active.config=next;active.onChange(next);show('main');
  }
  function render(){
    if(!active)return;const {panel,config,page,catalog}=active,p=providerFor(config,catalog),model=p.models.find(m=>m.id===config.model),{providers,labels}=catalog;
    panel.replaceChildren();panel.dataset.page=page;
    const fields=[
      ['provider','Provider',p.label,'Choose the app or service that runs the agent.',false],
      ['model','Model',model.label,'Choose which model answers within that provider.',false],
      ['effort','Effort',labels[config.effort],model.efforts.length?'Choose how much thinking work to request. Higher effort may take longer.':'This provider chooses thinking effort automatically.',!model.efforts.length],
      ['speed','Speed',labels[config.speed],model.speeds.length>1?'Choose response speed independently of thinking effort.':'This model has no alternate speed option.',model.speeds.length<2]
    ];
    const expanded=page==='credentials'?'provider':page;
    for(const [field,label,value,hint,disabled] of fields){
      const header=row({label,value,iconName:field==='provider'?p.icon:field==='speed'?`speed-option:${config.speed}`:field,chevron:!disabled,disabled,action:()=>show(field),hint});
      header.classList.add('agent-section-header');
      header.setAttribute('aria-expanded',String(expanded===field));
      panel.append(header);
      if(expanded===field)break;
    }
    if(page!=='main')panel.append(el('div','menu-rule'));
    if(page==='provider'||page==='credentials'){
      for(const item of providers){
        const select=row({label:item.label,iconName:item.icon,checked:item.id===config.provider,action:()=>commit({provider:item.id}),hint:item.hint});
        if(!item.needsKey){panel.append(select);continue;}
        const split=el('div','agent-provider-split');split.append(select);
        const keyButton=el('button','agent-api-chevron');keyButton.type='button';keyButton.innerHTML=icon('next');keyButton.setAttribute('aria-label','OpenAI API key options');keyButton.setAttribute('aria-expanded',String(page==='credentials'));
        keyButton.dataset.hint=credential()==='missing'?'Add an API key without changing provider.':'Change the API key without changing provider.';
        keyButton.addEventListener('click',()=>{active.page=page==='credentials'?'provider':'credentials';render();});split.append(keyButton);panel.append(split);
        if(page==='credentials'){
          panel.append(el('div','menu-rule'));
          const label=credential()==='missing'?'Add API key':'Change API key';
          panel.append(row({label,iconName:'key',action:()=>keyDialog(label),hint:'Open a dedicated masked field. Preserve the chat and draft.'}));
          break;
        }
      }
    }else if(page==='model'){
      for(const item of p.models)panel.append(row({label:item.label,iconName:'model',checked:item.id===config.model,action:()=>commit({model:item.id}),hint:`Use ${item.label} for the next message.`}));
    }else if(page==='effort'||page==='speed'){
      for(const value of page==='effort'?model.efforts:model.speeds)panel.append(row({label:labels[value],iconName:page==='speed'?`speed-option:${value}`:page,checked:value===config[page],action:()=>commit({[page]:value}),hint:`Use ${labels[value].toLowerCase()} ${page} for the next reply.`}));
    }

    panel.querySelector('button:not(:disabled)')?.focus({preventScroll:true});
  }
  function keyDialog(title){
    const callbacks=active;close();callbacks.onClose();
    const host=document.getElementById('dialogs');host.replaceChildren();
    const cover=el('div','modal-cover'),form=el('form','reference-dialog api-key-dialog');
    form.setAttribute('role','dialog');form.setAttribute('aria-modal','true');form.setAttribute('aria-label',title);
    form.append(el('h2','',title),el('p','', 'OpenAI API'));
    const label=el('label','','API key');label.htmlFor='agent-api-key';
    const input=el('input');input.id='agent-api-key';input.type='password';input.autocomplete='off';input.spellcheck=false;input.placeholder='Enter an API key';input.required=true;input.setAttribute('aria-describedby','api-key-note');
    const note=el('p','', 'Studio preview: only “key added” status is saved. The key itself is discarded and is never sent.');note.id='api-key-note';
    const controls=el('div','dialog-actions'),cancel=el('button','','Cancel'),save=el('button','primary-action','Save key');
    cancel.type='button';save.type='submit';controls.append(cancel,save);form.append(label,input,note,controls);cover.append(form);host.append(cover);
    const dismiss=()=>{input.value='';host.replaceChildren();};cancel.addEventListener('click',dismiss);
    cover.addEventListener('click',e=>{if(e.target===cover)dismiss();});
    form.addEventListener('submit',e=>{
      e.preventDefault();if(!input.value.trim())return;
      input.value='';try{localStorage.setItem(credentialKey,JSON.stringify({status:'valid'}));}catch{}
      dismiss();callbacks.onStatus('API key flow saved for review. Credential text was discarded.');
    });input.focus();
  }
  window.addEventListener('storage',e=>{if(e.key===credentialKey&&active?.page==='credentials')render();});
  window.ChatAgentPicker={providers,resolve,providerFor,label:value=>labels[value]||value,open,close};
})();
