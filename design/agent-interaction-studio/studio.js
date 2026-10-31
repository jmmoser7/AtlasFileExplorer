/* Studio navigation owns only the tabs and review documents, never the element's UI. */
(() => {
  'use strict';
  const $=id=>document.getElementById(id),key='slate-design-studio-tabs-v1';
  let tabs=[{id:'chat-train',name:'Chat Train',kind:'chat-train',contract:'contracts/chat-train.md'},{id:'image-generator',name:'Images',kind:'image-generator',contract:'contracts/image-generator.md',status:'draft'},{id:'dashboard-builder',name:'Dashboard Builder',kind:'element',contract:'contracts/dashboard-builder.md',status:'draft'},{id:'text-agent',name:'Text Agent',kind:'element',contract:'contracts/text-agent.md',status:'draft'},{id:'rhino-agent',name:'Rhino 3D',kind:'element',contract:'contracts/rhino-agent.md',status:'draft'}],active='chat-train';
  try{const saved=JSON.parse(localStorage.getItem(key)||'null');if(Array.isArray(saved?.tabs))tabs.push(...saved.tabs.filter(t=>t.id!=='chat-train'&&typeof t.name==='string'&&t.kind==='blank'));}catch{}
  const save=()=>{try{localStorage.setItem(key,JSON.stringify({tabs}));}catch{}};
  window.StudioShell={get activeId(){return active;},get tabs(){return tabs.map(t=>({...t}));},activate};
  const button=(name,cls,click)=>{const n=document.createElement('button');n.type='button';n.className=cls;n.textContent=name;n.addEventListener('click',click);return n;};
  function activate(id){
    if(!tabs.some(t=>t.id===id))return;
    window.ChatAgentPicker.close();active=id;const tab=tabs.find(t=>t.id===id),isChat=tab.kind==='chat-train',isImage=tab.kind==='image-generator';
    $('canvas').hidden=!isChat;document.querySelector('.studio-bottom:not(#image-studio-bottom)').hidden=!isChat;
    $('studio-empty').hidden=isChat||isImage||tab.kind==='element';$('image-generator-canvas').hidden=!isImage;$('image-studio-bottom').hidden=!isImage;$('reference-button').hidden=!isChat;$('notes-button').hidden=!isChat;
    $('notes-panel').hidden=true;$('notes-button').setAttribute('aria-expanded','false');
    document.querySelectorAll('[data-element-studio]').forEach(el=>el.hidden=el.dataset.elementStudio!==id);
    $('menus').replaceChildren();$('dialogs').replaceChildren();$('hint').hidden=true;
    $('studio-empty').replaceChildren();const p=document.createElement('p'),title=document.createElement('strong');title.textContent=tab.name;p.append(title,document.createTextNode('Canvas ready for the next element review.'));$('studio-empty').append(p);
    render();
    window.dispatchEvent(new CustomEvent('studio-element-changed',{detail:{id}}));
  }
  function render(){
    const strip=$('studio-tabs');strip.replaceChildren();
    for(const tab of tabs){const b=button('','studio-tab',()=>activate(tab.id));b.setAttribute('role','tab');b.setAttribute('aria-selected',String(tab.id===active));b.setAttribute('aria-controls',tab.kind==='chat-train'?'canvas':tab.kind==='image-generator'?'image-generator-canvas':tab.kind==='element'?tab.id+'-canvas':'studio-empty');b.title=tab.name;
      b.tabIndex=tab.id===active?0:-1;const dot=document.createElement('i');dot.className='tab-dot';const label=document.createElement('span');label.textContent=tab.name;b.append(dot,label);
      if(tab.kind!=='blank')strip.append(b);
      else{const slot=document.createElement('div');slot.className='studio-tab-slot';const close=button('×','studio-tab-close',()=>{tabs=tabs.filter(t=>t.id!==tab.id);save();activate(active===tab.id?'chat-train':active);});close.setAttribute('aria-label',`Close ${tab.name} tab`);slot.append(b,close);strip.append(slot);}
    }
  }
  function modal(title){const cover=document.createElement('div');cover.className='modal-cover';const form=document.createElement('form');form.className='reference-dialog studio-dialog';form.setAttribute('role','dialog');form.setAttribute('aria-modal','true');form.setAttribute('aria-label',title);const h=document.createElement('h2');h.textContent=title;form.append(h);cover.append(form);$('dialogs').replaceChildren(cover);cover.addEventListener('click',e=>{if(e.target===cover)cover.remove();});return form;}
  $('new-element').addEventListener('click',()=>{
    const form=modal('New element tab'),label=document.createElement('label'),input=document.createElement('input');label.textContent='Element name';input.id='element-name';input.required=true;input.maxLength=60;input.placeholder='For example, Image generator';label.htmlFor=input.id;form.append(label,input);
    const actions=document.createElement('div');actions.className='dialog-actions';actions.append(button('Cancel','',()=>$('dialogs').replaceChildren()));const add=button('Create tab','',()=>{});add.type='submit';actions.append(add);form.append(actions);
    form.addEventListener('submit',e=>{e.preventDefault();const name=input.value.trim();if(!name)return;const id=`element-${Date.now().toString(36)}`;tabs.push({id,name,kind:'blank'});save();activate(id);});input.focus();
  });
  $('debrief-button').addEventListener('click',()=>{
    const tab=tabs.find(t=>t.id===active),panel=modal(`${tab.name} debrief`);
    if(!tab.contract){const p=document.createElement('p');p.textContent='This element has no locked decisions yet. Its debrief will be created from the shared contract template as the review progresses.';panel.append(p,button('Close','',()=>$('dialogs').replaceChildren()));return;}
    panel.className='reference-dialog debrief-dialog';panel.replaceChildren();const toolbar=document.createElement('div');toolbar.className='debrief-toolbar';const title=document.createElement('h2');title.textContent=`${tab.name} · ${tab.status==='draft'?'draft for review':'agreed contract'}`;const download=document.createElement('a');download.href=tab.contract;download.download=`${tab.id}-contract.md`;download.textContent='Download document';toolbar.append(title,download,button('Close','',()=>$('dialogs').replaceChildren()));const frame=document.createElement('iframe');frame.title=`${tab.name} debrief document`;frame.src=`contract-viewer.html?document=${encodeURIComponent(tab.contract)}`;panel.append(toolbar,frame);
  });
  $('studio-tabs').addEventListener('keydown',e=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();const i=tabs.findIndex(t=>t.id===active);const next=e.key==='Home'?0:e.key==='End'?tabs.length-1:(i+(e.key==='ArrowRight'?1:tabs.length-1))%tabs.length;activate(tabs[next].id);$('studio-tabs').querySelector('[aria-selected=true]')?.focus();});
  window.addEventListener('keydown',e=>{if(e.key==='Escape'&&active!=='chat-train')$('dialogs').replaceChildren();});
  render();
})();
