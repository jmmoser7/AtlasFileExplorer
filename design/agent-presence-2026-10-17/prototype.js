/* Disposable, local-only design review. Native model and transports remain the owners. */
(async () => {
  'use strict';
  let metrics,coverFlow,providerIcons,neutralFits;
  try {
    const read=async file=>{const response=await fetch(file,{cache:'no-store'});if(!response.ok)throw new Error(file+' response '+response.status);return response.json();};
    [metrics,coverFlow,providerIcons,neutralFits]=await Promise.all(['metrics.json','cover-flow-preview.json','provider-icons.json','hat-neutral-fits.json'].map(read));
    if(!metrics.parameters||typeof metrics.revision!=='string'||!Object.values(metrics.parameters).every(p=>Number.isFinite(p.value)))throw new Error('Invalid metric ledger');
    if(!Array.isArray(coverFlow.samples)||coverFlow.samples.length<2||!coverFlow.samples.every(p=>['offset','x','yawDeg','z','focal'].every(k=>Number.isFinite(p[k]))))throw new Error('Invalid generated Cover Flow poses');
    if(metrics.parameters.hatFlowCover?.value!==coverFlow.cover)throw new Error('Cover Flow metrics do not match the generated cover size');
    if(!providerIcons.icons?.ProviderCodex?.path||!providerIcons.icons?.ProviderCursor?.path)throw new Error('Missing canonical harness icons');
    if(!neutralFits.fits||!Object.values(neutralFits.fits).every(f=>['x','y','width','height','rotation'].every(k=>Number.isFinite(f[k]))))throw new Error('Invalid neutral hat fits');
  } catch(error) {
    const alert=document.createElement('p');alert.className='initialization-error';alert.setAttribute('role','alert');
    alert.textContent='The design review could not load its design data. Open it through the local HTTP preview and reload. '+error.message;
    document.body.append(alert);document.getElementById('prototype-loading')?.remove();document.documentElement.dataset.prototypeStatus='error';return;
  }
  const params=Object.fromEntries(Object.entries(metrics.parameters).map(([name,metric])=>[name,metric.value]));
  const revision=metrics.revision;
  const storageKey = 'slate-presence-design-2026-10-17';
  const $ = (s, r=document) => r.querySelector(s);
  const esc = v => String(v ?? '').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const icon = (name, cls='') => `<svg class="${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${{
    chevron:'<path d="m6 9 6 6 6-6"/>',locate:'<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="2.5"/><path d="M12 1v4m0 14v4M1 12h4m14 0h4"/>',
    pause:'<path d="M8 5v14m8-14v14" stroke-width="3.6"/>',play:'<path d="m8 5 11 7-11 7Z" fill="currentColor" stroke="none"/>',stop:'<rect x="5" y="5" width="14" height="14" rx="1.3" fill="currentColor" stroke="none"/>',
    more:'<circle cx="4" cy="12" r="1.8" fill="currentColor"/><circle cx="12" cy="12" r="1.8" fill="currentColor"/><circle cx="20" cy="12" r="1.8" fill="currentColor"/>',
    arrow:'<path d="M12 20V4m-7 7 7-7 7 7" stroke-width="2.2"/>',conversation:'<path d="M5 3h14v18H5Z"/><path d="M8 7h8M8 11h8m-8 4h5"/>',clone:'<rect x="3" y="4" width="6" height="16" rx="1"/><rect x="15" y="4" width="6" height="16" rx="1"/><path d="M5 8h2m10 0h2M5 12h2m10 0h2"/>',
    access:'<rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3m-4 5v2"/>',clock:'<circle cx="12" cy="12" r="9"/><path d="M12 6v6l4 3"/>',retire:'<path d="M3 18h18M5.5 15a6.5 6.5 0 0 1 13 0M12 3v2M4.2 7.2l1.4 1.4m12.8 0 1.4-1.4M2 13h2m16 0h2"/>',close:'<path d="m6 6 12 12M18 6 6 18"/>'
  }[name] || ''}</svg>`;
  const sample = [
    {role:'outgoing',text:'hey hows it going?'},
    {role:'incoming',text:'Hey! Going well—how’s it going with you?'},
    {role:'outgoing',text:'pretty well just testing the interface'},
    {role:'incoming',text:'Sounds good—happy to be your test conversation. How’s the interface feeling so far?'}
  ];
  let stored = {};
  try { stored = JSON.parse(localStorage.getItem(storageKey) || '{}'); } catch (_) {}
  const data = {
    me:{id:'me',type:'human',name:stored.me?.name || 'You',photo:stored.me?.photo || ''},
    agents:Array.isArray(stored.agents) ? stored.agents : [{id:'agent',type:'agent',name:'Agent',hat:'cap',retired:false}],
    humans:Array.isArray(stored.humans) ? stored.humans : [],
    threads:stored.threads || {agent:sample,me:[]},
    drafts:stored.drafts&&typeof stored.drafts==='object'&&!Array.isArray(stored.drafts)?stored.drafts:{}
  };
  const state = {zoom:1,pan:{x:0,y:0},chat:'agent',profile:null,popover:null,hats:null,canvasChat:false,canvasChatScale:'canvas',paused:false,listening:false,working:false,position:{x:340,y:300},lastTime:0,workProgress:0};
  data.agents.forEach(p=>{p.provider=p.provider==='cursor'?'cursor':'codex';delete p.queuedProvider;});
  state.taskAgentId=null;state.pendingProvider=null;
  const faceControllers = new Set();
  let worldFace;
  const focusedDrafts = data.drafts;
  const participant = id => id==='me' ? data.me : [...data.agents,...data.humans].find(p=>p.id===id);
  const currentAgent = () => data.agents.find(p=>!p.retired) || data.agents[0];
  const hats = () => globalThis.PresenceHats?.items || [{id:'cap',name:'Baseball cap'}];
  const hatSvg = id => globalThis.PresenceHats?.svg(id) || '';
  const fittedHatCache=new Map();
  function fittedHat(id) {
    if(!id||id==='none')return '';
    if(fittedHatCache.has(id))return fittedHatCache.get(id);
    const fit=neutralFits.fits[id];if(!fit)return '';
    const art=new DOMParser().parseFromString(hatSvg(id),'image/svg+xml').documentElement;
    for(const key of ['x','y','width','height'])art.setAttribute(key,fit[key]);
    art.setAttribute('preserveAspectRatio','none');
    const markup=`<svg class="fitted-hat" viewBox="0 0 240 260" aria-hidden="true"><g transform="rotate(${fit.rotation} ${fit.x+fit.width/2} ${fit.y+fit.height/2})">${art.outerHTML}</g></svg>`;
    fittedHatCache.set(id,markup);return markup;
  }
  function persist() {
    try { localStorage.setItem(storageKey,JSON.stringify(data)); }
    catch (_) { status('Browser storage is full. The most recent change could not be saved.'); }
  }
  function status(text) { $('#review-status').textContent=text; }
  function faceMarkup(p) {
    if (p.type==='human') return `<span class="human-avatar">${p.photo?`<img src="${esc(p.photo)}" alt="">`:esc(p.name==='You'?'JM':p.name.slice(0,2).toUpperCase())}</span>`;
    return `<span class="face-host" data-face-owner="${esc(p.id)}"><svg class="face-svg" data-avatar="${esc(p.id)}"></svg>${fittedHat(p.hat)}</span>`;
  }
  function initFaces(root=document) {
    root.querySelectorAll('svg[data-avatar]').forEach(svg=>{
      if(svg._controller) return;
      const controller=AvatarFace.create(svg);
      svg.setAttribute('viewBox','40 30 240 260');
      controller.setTarget(state.listening?'curious':'neutral','forward',AvatarRig.presentationDefaults.settings,{duration:0});
      svg._controller=controller;faceControllers.add(controller);
    });
  }
  function cleanFaces(root) {
    root.querySelectorAll('svg[data-avatar]').forEach(svg=>{svg._controller?.destroy();faceControllers.delete(svg._controller);});
  }
  function replace(root, html) {cleanFaces(root);root.innerHTML=html;initFaces(root);}
  function applyParams() {
    for(const [key,value] of Object.entries(params)) document.documentElement.style.setProperty('--'+key.replace(/[A-Z]/g,m=>'-'+m.toLowerCase()),String(value)+(metrics.parameters[key].unit==='ratio'?'':'px'));
    $('#dialog-width-value').value=params.dialogWidth;
    $('#dialog-width').value=params.dialogWidth;
    updateSnapshot();positionCanvasChat();
  }
  function updateSnapshot() {
    $('#snapshot').value=JSON.stringify({revision,params,view:{width:innerWidth,height:innerHeight,zoom:state.zoom,chat:state.chat,profile:state.profile,hats:state.hats,canvasChat:state.canvasChat,canvasChatScale:state.canvasChatScale,listening:state.listening}},null,2);
  }
  function renderRoster() {
    const members=[data.me,...data.agents.filter(p=>!p.retired),...data.humans];
    $('#roster').classList.toggle('crowded',members.length>3);
    replace($('#roster'),members.map((p,i)=>`${i?'<span class="roster-rule" aria-hidden="true"></span>':''}<div class="participant" data-participant="${esc(p.id)}"><button class="roster-avatar" data-open-chat="${esc(p.id)}" aria-label="Chat with ${esc(p.name)}">${faceMarkup(p)}</button><button class="roster-name" data-profile="${esc(p.id)}" aria-label="${esc(p.name)} profile" aria-expanded="${state.profile===p.id}"><span>${esc(p.name)}</span>${icon('chevron','chevron')}</button></div>`).join('')+'<span class="roster-rule" aria-hidden="true"></span><button class="add-button" aria-label="Add a person or agent" aria-expanded="'+(state.popover==='add')+'">+</button>');
    $('#roster').querySelectorAll('[data-open-chat]').forEach(el=>el.onclick=()=>openChat(el.dataset.openChat));
    $('#roster').querySelectorAll('[data-profile]').forEach(el=>el.onclick=()=>openProfile(el.dataset.profile));
    $('.add-button').onclick=()=>{state.profile=null;state.popover=state.popover==='add'?null:'add';renderProfile();renderRoster();};
  }
  function renderProfile() {
    const layer=$('#profile-layer');
    const activeName=$('.profile-name-input',layer),selection=activeName===document.activeElement?{start:activeName.selectionStart,end:activeName.selectionEnd}:null;
    if(state.popover==='add') {
      const retired=data.agents.filter(p=>p.retired);
      replace(layer,`<section class="add-popover" aria-label="Add collaborators"><button data-add-agent>Add agent</button><button data-add-person>Add person</button>${retired.length?'<hr>'+retired.map(p=>`<button data-restore="${esc(p.id)}">Bring back ${esc(p.name)}</button>`).join(''):''}<small>People and agents live together here.</small></section>`);
      $('[data-add-agent]',layer).onclick=addAgent;$('[data-add-person]',layer).onclick=addPerson;
      layer.querySelectorAll('[data-restore]').forEach(b=>b.onclick=()=>restore(b.dataset.restore));
      return;
    }
    const p=participant(state.profile);
    if(!p){replace(layer,'');return;}
    const segment=$(`[data-participant="${p.id}"]`),bounds=segment?.getBoundingClientRect();
    const width=Math.min(248,innerWidth-24);
    const right=Math.max(12,Math.min(innerWidth-width-12,innerWidth-(bounds?.right || innerWidth-22)));
    const ownProfile=p.id==='me',editable=ownProfile||p.type==='agent';
    replace(layer,`<section class="profile-popover" style="right:${right}px" aria-label="${esc(p.name)} profile"><div class="profile-heading"><span>${ownProfile?'Your profile':p.type==='agent'?'Agent profile':'Profile'}</span><button class="close-button" aria-label="Close profile">×</button></div><button class="profile-picture" aria-label="${ownProfile?'Add or change profile picture':'Chat with '+esc(p.name)}">${faceMarkup(p)}${ownProfile?'<span class="photo-add">+</span>':''}</button><span class="profile-picture-text">${ownProfile?(p.photo?'Change profile picture':'Add a profile picture'):p.type==='agent'?'Your board collaborator':'Direct conversation'}</span>${ownProfile&&p.photo?'<button class="remove-photo" data-remove-photo>Remove photo</button>':''}${editable?`<input class="profile-name-input" aria-label="Display name" maxlength="60" value="${esc(p.name)}"><p class="profile-save">Saved as you type</p>`:`<div class="profile-name-display">${esc(p.name)}</div><button class="profile-link" data-profile-chat>Chat with ${esc(p.name)}</button>`}${p.type==='agent'?`<button class="profile-link" data-appearance><span>Hat</span><span class="tiny-hat">${hatSvg(p.hat)}</span></button>`:''}${ownProfile?'<input type="file" accept="image/png,image/jpeg,image/webp" id="photo-file" hidden>':''}</section>`);
    $('.close-button',layer).onclick=closeProfile;
    const nameInput=$('.profile-name-input',layer);
    if(nameInput){nameInput.oninput=e=>{p.name=e.target.value.trim() || (p.id==='me'?'You':'Agent');persist();renderRoster();updateBoardLabels();};nameInput.onchange=()=>{renderChat();updateSnapshot();};}
    $('.profile-picture',layer).onclick=()=>ownProfile?$('#photo-file').click():openChat(p.id);
    $('[data-profile-chat]',layer)?.addEventListener('click',()=>openChat(p.id));
    $('[data-remove-photo]',layer)?.addEventListener('click',()=>{p.photo='';persist();renderProfile();renderRoster();renderChat();});
    $('[data-appearance]',layer)?.addEventListener('click',()=>{openChat(p.id);state.hats='flow';renderChat();});
    if(ownProfile)$('#photo-file',layer).onchange=e=>{
      const file=e.target.files[0];if(!file)return;
      if(file.size>3*1024*1024){status('Choose a profile image smaller than 3 MB.');return;}
      const reader=new FileReader();reader.onload=()=>{p.photo=String(reader.result);persist();renderProfile();renderRoster();renderChat();};reader.readAsDataURL(file);
    };
    if(selection){const input=$('.profile-name-input',layer);input.focus({preventScroll:true});input.setSelectionRange(selection.start,selection.end);}
    updateSnapshot();
  }
  function closeProfile(){state.profile=null;state.popover=null;renderProfile();renderRoster();updateSnapshot();}
  function openProfile(id){
    rememberDrafts();state.profile=state.profile===id?null:id;state.popover=null;state.hats=null;
    state.chat=null;state.canvasChat=false;state.listening=false;
    renderChat();renderCanvasChat();updateBoardState();renderProfile();renderRoster();
    if(state.profile)$('.profile-name-input')?.focus({preventScroll:true});
  }
  function openChat(id) {
    if(!participant(id))return;
    state.chat=id;state.profile=null;state.popover=null;state.hats=null;state.canvasChat=false;state.pendingProvider=null;
    renderProfile();renderRoster();renderChat();renderCanvasChat();updateSnapshot();
  }
  function messageHtml(p, messages) {
    if(!messages.length)return `<div class="empty-thread">${faceMarkup(p)}${p.id==='me'?'A little space for your own thoughts.':'Start a conversation.'}</div>`;
    return messages.map(m=>m.role==='system'?`<div class="system-message">${esc(m.text)}</div>`:`<div class="message-row ${m.role}">${m.role==='incoming'?`<div class="message-avatar">${faceMarkup(p)}</div>`:''}<div class="bubble">${esc(m.text)}</div></div>`).join('');
  }
  const composerHtml = (compact=false) => `<form class="composer" data-composer="${compact?'canvas':'window'}"><textarea rows="1" placeholder="Message" aria-label="Message" spellcheck="true"></textarea><button class="send-button" type="submit" aria-label="Save message locally" disabled>${icon('arrow')}</button></form>`;
  const providerName=id=>id==='cursor'?'Cursor':'Codex';
  function providerGlyph(id) {
    const name=id==='cursor'?'ProviderCursor':'ProviderCodex',glyph=providerIcons.icons[name];
    const viewBox=Array.isArray(glyph.view_box)?glyph.view_box.join(' '):glyph.view_box;
    return `<svg class="provider-glyph" viewBox="${esc(viewBox)}" fill="${glyph.filled?'currentColor':'none'}" stroke="${glyph.filled?'none':'currentColor'}" stroke-width="${glyph.stroke_width}" aria-hidden="true"><path d="${esc(glyph.path)}"/></svg>`;
  }
  function providerMenu(p) {
    if(state.popover!=='provider')return '';
    const pending=state.pendingProvider?.agentId===p.id?state.pendingProvider:null;
    return `<section class="provider-menu" aria-label="Choose harness">${['codex','cursor'].map(id=>`<button class="provider-option" data-harness="${id}" aria-pressed="${p.provider===id}">${providerGlyph(id)}<span>${providerName(id)}</span>${p.provider===id?'<span class="selected-mark">✓</span>':''}</button>`).join('')}<div class="provider-model"><span>Model</span><span>Default</span></div><p class="provider-note">A new session starts on your next message. Your visible history stays here.</p>${p.queuedProvider?`<p class="provider-note">${providerName(p.queuedProvider)} is queued for after this task.</p>`:''}${pending?`<div class="provider-switch-offer"><span>${esc(p.name)} is working. Switch to ${providerName(pending.target)}:</span><button data-switch-choice="after">After this task</button><button data-switch-choice="stop">Stop task and switch</button><button data-switch-choice="cancel">Cancel</button></div>`:''}</section>`;
  }
  function headerHtml(p) {
    if(p.type==='human')return `<div class="chat-header"><div class="chat-identity">${faceMarkup(p)}<button class="chat-identity-name" data-chat-profile>${esc(p.name)}</button></div><span class="participant-description">${p.id==='me'?'Your private notes':'Direct conversation'}</span><button class="overflow-button" data-close-chat aria-label="Close conversation">${icon('close')}</button></div>`;
    return `<div class="chat-header"><div class="chat-identity">${faceMarkup(p)}<button class="chat-identity-name" data-chat-profile>${esc(p.name)}</button><button class="round-button locate-button" data-locate aria-label="Find ${esc(p.name)} on the board">${icon('locate')}</button></div><button class="round-button" data-pause aria-label="${state.paused?'Resume':'Pause'} agent">${icon(state.paused?'play':'pause')}</button><button class="round-button" data-stop aria-label="Stop current task">${icon('stop')}</button><button class="hat-button" data-hats aria-label="Choose a hat" aria-expanded="${!!state.hats}">${hatSvg(p.hat)}</button><button class="provider-button" data-provider aria-label="Harness: ${providerName(p.provider)} · Default${p.queuedProvider?', '+providerName(p.queuedProvider)+' queued':''}" aria-expanded="${state.popover==='provider'}"><span class="provider-label">${providerGlyph(p.provider)}<span>${providerName(p.provider)} · Default</span>${p.queuedProvider?`<span class="provider-queued">${icon('clock')}</span>`:''}</span>${icon('chevron','chevron')}</button><button class="overflow-button" data-overflow aria-label="Agent menu" aria-expanded="${state.popover==='overflow'}">${icon('more')}</button>${state.popover==='overflow'?overflowHtml():''}${providerMenu(p)}</div>`;
  }
  function overflowHtml() {
    return `<section class="overflow-menu" aria-label="Agent actions"><button data-menu-action="conversation">${icon('conversation')}<span>Full conversation</span></button><hr><button data-menu-action="clone">${icon('clone')}<span>Clone agent</span></button><button data-menu-action="access">${icon('access')}<span>Full access</span></button><button data-menu-action="schedule">${icon('clock')}<span>Schedule…</span></button><hr><button class="retire-action" data-menu-action="retire">${icon('retire')}<span>Retire agent</span></button></section>`;
  }
  function applyHarness(p,target) {
    if(p.provider===target)return;
    p.provider=target;p.sessionRevision=(p.sessionRevision||0)+1;delete p.queuedProvider;state.pendingProvider=null;
    (data.threads[p.id] ||= []).push({role:'system',text:providerName(target)+' will be used for the next message. Earlier messages remain visible.'});
    persist();renderChat();renderCanvasChat();status('Harness changed locally. No message was resent and no provider was invoked.');
  }
  function chooseHarness(p,target) {
    if(target===p.provider)return;
    if(state.working&&state.taskAgentId===p.id){state.pendingProvider={agentId:p.id,target};renderChat();return;}
    state.popover=null;applyHarness(p,target);
  }
  function stopCurrentTask(ownerId=state.taskAgentId) {
    if(state.taskAgentId&&ownerId!==state.taskAgentId)return false;
    if(!state.taskAgentId&&ownerId&&ownerId!==currentAgent()?.id)return false;
    const p=participant(state.taskAgentId);if(p)delete p.queuedProvider;
    state.paused=true;state.working=false;state.listening=false;state.pendingProvider=null;state.taskAgentId=null;
    persist();updateBoardState();return true;
  }
  function switchChoice(p,choice) {
    const pending=state.pendingProvider;if(!pending||pending.agentId!==p.id)return;
    if(choice==='cancel'){state.pendingProvider=null;renderChat();return;}
    if(choice==='after'){p.queuedProvider=pending.target;state.pendingProvider=null;state.popover=null;persist();renderChat();status(providerName(p.queuedProvider)+' is queued after this finite task; current work continues.');return;}
    const target=pending.target;stopCurrentTask(p.id);state.popover=null;applyHarness(p,target);
  }
  function hatPickerHtml(p) {
    if(!state.hats || p.type!=='agent')return '';
    return `<section class="hat-picker" aria-label="Hat picker"><div class="hat-flow" role="listbox" aria-label="Agent hats" tabindex="0"><div class="hat-scroll" aria-hidden="true"><div class="hat-track">${hats().map(()=>'<span class="hat-stop"></span>').join('')}</div></div><div class="hat-stage">${hats().map(h=>`<button class="flow-card ${p.hat===h.id?'selected':''} ${h.id==='none'?'none-hat':''}" role="option" id="flow-hat-${esc(h.id)}" data-hat="${esc(h.id)}" aria-label="${esc(h.name)}" aria-selected="${p.hat===h.id}" tabindex="-1">${h.id==='none'?'<span>No hat</span>':hatSvg(h.id)}</button>`).join('')}</div><span class="hat-flow-name"></span><button class="hat-flow-arrow previous" aria-label="Previous hat">‹</button><button class="hat-flow-arrow next" aria-label="Next hat">›</button></div></section>`;
  }
  let hatFlowView=null;
  function coverPose(offset) {
    const samples=coverFlow.samples,step=samples[1].offset-samples[0].offset;
    const position=Math.max(0,Math.min(samples.length-1,(offset-samples[0].offset)/step)),a=Math.floor(position),b=Math.min(samples.length-1,a+1),t=position-a;
    return Object.fromEntries(['x','yawDeg','z','focal'].map(k=>[k,samples[a][k]+(samples[b][k]-samples[a][k])*t]));
  }
  function selectHat(p,id) {
    if(p.hat===id)return;
    p.hat=id;persist();
    document.querySelectorAll('[data-face-owner]').forEach(host=>{
      if(host.dataset.faceOwner!==p.id)return;
      host.querySelector(':scope > .fitted-hat')?.remove();
      const art=fittedHat(id);if(art)host.insertAdjacentHTML('beforeend',art);
    });
    const button=$('[data-hats]');if(button)button.innerHTML=hatSvg(id);
    updateSnapshot();
  }
  function bindHatFlow(p) {
    const flow=$('.hat-flow');if(!flow){hatFlowView=null;return;}
    const scroll=$('.hat-scroll',flow),track=$('.hat-track',flow),stage=$('.hat-stage',flow),cards=[...flow.querySelectorAll('[data-hat]')],items=hats(),cover=params.hatFlowCover;
    let pointer=null,suppressClick=false,frame=0;
    function paint() {
      frame=0;if(!flow.isConnected)return;
      const position=scroll.scrollLeft/cover,index=Math.max(0,Math.min(items.length-1,Math.round(position)));
      cards.forEach((card,i)=>{
        const offset=i-position,pose=coverPose(offset),visible=Math.abs(offset)<=4;
        card.style.visibility=visible?'visible':'hidden';card.style.transform=`perspective(${pose.focal}px) translate3d(${pose.x}px,0,${pose.z}px) rotateY(${pose.yawDeg}deg)`;card.style.zIndex=String(Math.round(1000+pose.z));
        card.classList.toggle('selected',i===index);card.setAttribute('aria-selected',String(i===index));
      });
      $('.hat-flow-name',flow).textContent=items[index].name;flow.setAttribute('aria-activedescendant','flow-hat-'+items[index].id);
      selectHat(p,items[index].id);
    }
    function queuePaint(){if(!frame)frame=requestAnimationFrame(paint);}
    function go(index){scroll.scrollTo({left:Math.max(0,Math.min(items.length-1,index))*cover,behavior:matchMedia('(prefers-reduced-motion: reduce)').matches?'auto':'smooth'});}
    const resize=()=>{const index=Math.max(0,items.findIndex(h=>h.id===p.hat));track.style.paddingInline=Math.max(0,(scroll.clientWidth-cover)/2)+'px';scroll.scrollLeft=index*cover;paint();};
    scroll.addEventListener('scroll',queuePaint,{passive:true});
    flow.addEventListener('wheel',e=>{e.preventDefault();e.stopPropagation();const delta=Math.abs(e.deltaX)>Math.abs(e.deltaY)?e.deltaX:e.deltaY;if(delta)go(Math.round(scroll.scrollLeft/cover)+Math.sign(delta));},{passive:false});
    flow.addEventListener('pointerdown',e=>{if(e.button!==0||e.target.closest('.hat-flow-arrow'))return;pointer={x:e.clientX,left:scroll.scrollLeft};suppressClick=false;});
    flow.addEventListener('pointermove',e=>{if(!pointer)return;const dx=e.clientX-pointer.x;if(Math.abs(dx)>4){suppressClick=true;if(!flow.hasPointerCapture(e.pointerId))flow.setPointerCapture(e.pointerId);}if(suppressClick){scroll.style.scrollSnapType='none';scroll.scrollLeft=pointer.left-dx;}});
    const finish=e=>{if(!pointer)return;pointer=null;scroll.style.scrollSnapType='';if(suppressClick)go(Math.round(scroll.scrollLeft/cover));if(flow.hasPointerCapture(e.pointerId))flow.releasePointerCapture(e.pointerId);};
    flow.addEventListener('pointerup',finish);flow.addEventListener('pointercancel',finish);
    flow.addEventListener('keydown',e=>{const index=items.findIndex(h=>h.id===p.hat);if(['ArrowLeft','ArrowRight','Home','End'].includes(e.key)){e.preventDefault();e.stopPropagation();go(e.key==='Home'?0:e.key==='End'?items.length-1:index+(e.key==='ArrowLeft'?-1:1));}});
    cards.forEach((card,i)=>card.onclick=e=>{if(suppressClick){e.preventDefault();suppressClick=false;return;}go(i);flow.focus({preventScroll:true});});
    $('.previous',flow).onclick=()=>go(Math.round(scroll.scrollLeft/cover)-1);$('.next',flow).onclick=()=>go(Math.round(scroll.scrollLeft/cover)+1);
    hatFlowView={resize};resize();
  }
  function renderFaceSizes() {
    const host=$('#face-size-review');if(host.hidden)return;
    const selected=participant(state.chat),p=selected?.type==='agent'?selected:currentAgent();if(!p)return;
    replace(host,[36,72,150].map(size=>`<div class="face-size-sample ${size===150?'large':''}"><div style="width:${size}px;height:${size}px">${faceMarkup(p)}</div><span>${size}px · same fitted art</span></div>`).join(''));
    updateFaceSizePose();
  }
  function updateFaceSizePose(){const expression=state.listening?'curious':state.paused?'resting':state.working?'concentrating':'neutral';$('#face-size-review').querySelectorAll('[data-avatar]').forEach(svg=>svg._controller?.setTarget(expression,state.listening?'forward':'down-right',AvatarRig.presentationDefaults.settings,AvatarRig.presentationDefaults.motion));}
  function renderChat() {
    const layer=$('#chat-layer'),p=participant(state.chat);
    if(!p){hatFlowView=null;replace(layer,'');return;}
    const active=document.activeElement,activeInput=active?.matches('textarea')&&layer.contains(active)?{start:active.selectionStart,end:active.selectionEnd}:null;
    const activeHat=active?.closest('.hat-flow');
    rememberDrafts();
    replace(layer,`<section class="chat-window ${p.type==='human'?'human-chat':''}" aria-label="Conversation with ${esc(p.name)}">${headerHtml(p)}${hatPickerHtml(p)}<div class="chat-body"><div class="messages" aria-label="Conversation history">${messageHtml(p,data.threads[p.id] || [])}</div>${composerHtml()}</div></section>`);
    const messages=$('.messages',layer);messages.scrollTop=messages.scrollHeight;
    bindComposer(layer,p.id);
    $('[data-chat-profile]',layer).onclick=()=>openProfile(p.id);
    layer.querySelectorAll('[data-close-chat]').forEach(b=>b.onclick=()=>{rememberDrafts();state.chat=null;state.popover=null;state.hats=null;renderChat();updateSnapshot();});
    $('[data-locate]',layer)?.addEventListener('click',()=>{state.pan={x:0,y:0};updateWorld();status('The camera centers on the agent without moving it.');});
    $('[data-pause]',layer)?.addEventListener('click',()=>{state.paused=!state.paused;state.listening=false;updateBoardState();renderChat();renderCanvasChat();});
    $('[data-stop]',layer)?.addEventListener('click',()=>{const stopped=stopCurrentTask(p.id);renderChat();status(stopped?'The local task stopped. A queued harness change was not applied.':'This collaborator has no active local task to stop.');});
    $('[data-hats]',layer)?.addEventListener('click',()=>{state.hats=state.hats?null:'flow';state.popover=null;state.pendingProvider=null;renderChat();updateSnapshot();});
    $('[data-provider]',layer)?.addEventListener('click',()=>{state.popover=state.popover==='provider'?null:'provider';state.hats=null;state.pendingProvider=null;renderChat();});
    $('[data-overflow]',layer)?.addEventListener('click',()=>{state.popover=state.popover==='overflow'?null:'overflow';state.hats=null;renderChat();updateSnapshot();});
    layer.querySelectorAll('[data-menu-action]').forEach(b=>b.onclick=()=>menuAction(b.dataset.menuAction,p));
    layer.querySelectorAll('[data-harness]').forEach(b=>b.onclick=()=>chooseHarness(p,b.dataset.harness));
    layer.querySelectorAll('[data-switch-choice]').forEach(b=>b.onclick=()=>switchChoice(p,b.dataset.switchChoice));
    bindHatFlow(p);
    if(activeInput){const input=$('textarea',layer);input.focus({preventScroll:true});input.setSelectionRange(activeInput.start,activeInput.end);}
    else if(activeHat)$('.hat-flow',layer)?.focus({preventScroll:true});
    updateSnapshot();
  }
  function rememberDrafts(){document.querySelectorAll('[data-composer]').forEach(f=>{const id=f.dataset.thread;if(id)focusedDrafts[id]=$('textarea',f).value;});persist();}
  function bindComposer(root,id) {
    const form=$('[data-composer]',root);if(!form)return;
    form.dataset.thread=id;
    const input=$('textarea',form),send=$('.send-button',form);input.value=focusedDrafts[id] || '';
    const resize=()=>{send.disabled=!input.value.trim();input.style.height='auto';input.style.height=Math.min(100,Math.max(36,input.scrollHeight))+'px';focusedDrafts[id]=input.value;persist();};
    input.oninput=resize;
    input.onkeydown=e=>{if(e.key==='Enter'&&!e.shiftKey&&!e.isComposing){e.preventDefault();if(input.value.trim())form.requestSubmit();}};
    form.onsubmit=e=>{e.preventDefault();const text=input.value.trim();if(!text)return;(data.threads[id] ||= []).push({role:'outgoing',text});focusedDrafts[id]='';input.value='';persist();renderChat();renderCanvasChat();status('Message saved in this browser. This prototype does not send to a provider or another person.');};
    resize();
  }
  function menuAction(action,p) {
    state.popover=null;
    if(action==='retire'){retire(p);return;}
    if(action==='clone'){addAgent(p);return;}
    if(action==='conversation'){state.hats=null;renderChat();status('The full local sample conversation is visible.');return;}
    renderChat();status(action==='access'?'Full access needs an explicit native permission flow. It is not changed by this visual review.':'Scheduling is a native integration step. No schedule was created.');
  }
  function addAgent(source) {
    const base=source?.type==='agent'?source:null,id='agent-'+Date.now();
    const p={id,type:'agent',name:base?base.name+' copy':'Agent '+(data.agents.length+1),hat:base?.hat || 'beanie',provider:base?.provider||'codex',retired:false};
    data.agents.push(p);data.threads[id]=[];persist();state.popover=null;renderProfile();renderRoster();openChat(id);status('Added a local sample collaborator. No provider agent was launched.');
  }
  function addPerson() {
    const p={id:'person-'+Date.now(),type:'human',name:'Alex',photo:''};data.humans.push(p);data.threads[p.id]=[];persist();renderRoster();openChat(p.id);status('Alex is a local example participant. No invitation or remote message was sent.');
  }
  function retire(p) {
    stopCurrentTask(p.id);delete p.queuedProvider;
    if(state.pendingProvider?.agentId===p.id)state.pendingProvider=null;
    state.chat=null;state.canvasChat=false;persist();renderChat();renderCanvasChat();
    const board=p.id===currentAgent()?.id?$('#board-agent'):null;board?.classList.add('retiring');
    if(board)setTimeout(()=>{const puff=document.createElement('span');puff.className='puff';board.append(puff);setTimeout(()=>puff.remove(),1000);},550);
    setTimeout(()=>{p.retired=true;persist();renderRoster();board?.classList.remove('retiring');if(board)updateBoardVisual();else updateBoardState();status('Retired locally. Use + to bring this collaborator back, with its conversation intact.');},1100);
  }
  function restore(id){const p=participant(id);if(!p)return;p.retired=false;persist();state.popover=null;renderProfile();renderRoster();updateBoardVisual();updateBoardState();status('Collaborator restored with its name, hat, and local conversation.');}
  function updateBoardLabels(){const p=currentAgent();$('.agent-caption').textContent=p?.name || 'Agent';}
  function updateBoardVisual() {
    const p=currentAgent();if(!p)return;
    const host=$('.board-agent-visual');replace(host,faceMarkup(p));worldFace=$('svg',host)?._controller;updateBoardLabels();updateBoardState();
  }
  function updateBoardState() {
    const board=$('#board-agent'),p=currentAgent();
    board.classList.toggle('retired-agent',!p || p.retired);board.classList.toggle('is-listening',state.listening);
    $('.agent-activity').textContent=state.listening?'Listening':state.paused?'Paused':state.working?'Arranging':'Reading';
    $('#book').classList.toggle('hidden',state.paused||state.listening||state.working||p?.retired);
    worldFace?.setTarget(state.listening?'curious':state.paused?'resting':state.working?'concentrating':'neutral',state.listening?'forward':'down-right',AvatarRig.presentationDefaults.settings,AvatarRig.presentationDefaults.motion);
    updateFaceSizePose();
    updateSnapshot();
  }
  function listen() {
    const p=currentAgent();if(!p || p.retired)return;
    rememberDrafts();state.paused=true;state.listening=true;state.canvasChat=true;state.chat=null;state.profile=null;state.popover=null;
    updateBoardState();renderProfile();renderRoster();renderChat();renderCanvasChat();
    status('Listening: visible work is paused. The conversation follows the agent’s board position.');
    setTimeout(()=>$('#canvas-chat-layer textarea')?.focus(),220);
  }
  function renderCanvasChat() {
    const layer=$('#canvas-chat-layer'),p=currentAgent();
    if(!state.canvasChat||!p){replace(layer,'');return;}
    replace(layer,`<section class="canvas-chat" aria-label="Chat beneath ${esc(p.name)}"><div class="chat-body"><div class="canvas-chat-top"><span class="listening-dot"></span><span>${esc(p.name)} is listening</span><button class="close-button" aria-label="Close board chat">×</button></div><div class="messages" aria-label="Recent conversation">${messageHtml(p,(data.threads[p.id] || []).slice(-2))}</div>${composerHtml(true)}</div></section>`);
    $('.close-button',layer).onclick=()=>{rememberDrafts();state.canvasChat=false;state.listening=false;renderCanvasChat();updateBoardState();status('Board chat closed. The agent remains paused until resumed.');};
    bindComposer(layer,p.id);positionCanvasChat();$('.messages',layer).scrollTop=$('.messages',layer).scrollHeight;
    updateSnapshot();
  }
  function worldOrigin(){return {x:Math.min(innerWidth*.35,380)-340*state.zoom+state.pan.x,y:Math.min(innerHeight*.43,310)-300*state.zoom+state.pan.y};}
  function updateWorld() {
    const o=worldOrigin();$('#world').style.transform=`translate(${o.x}px,${o.y}px) scale(${state.zoom})`;
    $('#board-agent').style.left=state.position.x+'px';$('#board-agent').style.top=state.position.y+'px';
    $('#book').style.left=(state.position.x+29)+'px';$('#book').style.top=(state.position.y+22)+'px';
    positionCanvasChat();
  }
  function positionCanvasChat() {
    const chat=$('.canvas-chat');if(!chat)return;
    const o=worldOrigin(),scale=state.canvasChatScale==='canvas'?state.zoom:1;
    const x=o.x+(state.position.x+params.avatarDiameter/2)*state.zoom-params.canvasChatAnchorOffset*scale,y=o.y+(state.position.y+params.avatarDiameter)*state.zoom+params.canvasChatGap*scale;
    chat.style.left=x+'px';chat.style.top=y+'px';chat.style.transform=`scale(${scale})`;
  }
  function setZoom(value){state.zoom=value;$('#zoom-label').textContent=Math.round(value*100)+'%';document.querySelectorAll('[data-zoom]').forEach(b=>b.classList.toggle('selected',Number(b.dataset.zoom)===value));updateWorld();updateSnapshot();}
  function startWork(){state.paused=false;state.listening=false;state.working=true;state.canvasChat=false;state.chat=null;state.hats=null;state.pendingProvider=null;state.taskAgentId=currentAgent()?.id;state.workProgress=0;renderChat();renderCanvasChat();updateBoardState();status('Local visible-work example. Open its chat to review a busy harness switch, or double-click it to pause.');}
  function completeCurrentTask() {
    const p=participant(state.taskAgentId),queued=p?.queuedProvider;state.working=false;state.taskAgentId=null;
    if(state.pendingProvider?.agentId===p?.id)state.pendingProvider=null;
    if(queued)applyHarness(p,queued);else renderChat();
    updateBoardState();status(queued?'The finite task finished; '+providerName(queued)+' is selected for the next message. No message was resent.':'The visible-work example finished. Its movements and drag were continuous.');
  }
  function animate(time) {
    const dt=state.lastTime?Math.min(50,time-state.lastTime):0;state.lastTime=time;
    if(state.working&&!state.paused){
      state.workProgress+=dt/8500;const t=Math.min(1,state.workProgress),phase=t<.36?t/.36:t<.7?(t-.36)/.34:(t-.7)/.3;
      if(t<.36){state.position.x=340+(170-340)*phase;state.position.y=300+(340-300)*phase;}
      else if(t<.7){state.position.x=170+(290-170)*phase;state.position.y=340+(380-340)*phase;$('#card-landscape').style.left=(90+120*phase)+'px';$('#card-landscape').style.top=(300+40*phase)+'px';$('#card-landscape').style.transform=`rotate(${-4+4*phase}deg)`;}
      else{state.position.x=290+(340-290)*phase;state.position.y=380+(300-380)*phase;}
      if(t===1)completeCurrentTask();
      updateWorld();
    }
    requestAnimationFrame(animate);
  }
  document.querySelectorAll('[data-review]').forEach(b=>b.onclick=()=>{
    const action=b.dataset.review,p=currentAgent();
    if(action==='agent-chat')openChat(p.id);
    if(action==='self-profile')openProfile('me');
    if(action==='self-chat')openChat('me');
    if(action==='hats'){openChat(p.id);state.hats='flow';renderChat();}
    if(action==='face-sizes'){$('#face-size-review').hidden=!$('#face-size-review').hidden;renderFaceSizes();}
    if(action==='listening')listen();
    if(action==='work')startWork();
    if(action==='long-names'){
      const enabled=data.me.name!=='Jordan Moser · Studio';
      data.me.name=enabled?'Jordan Moser · Studio':'You';p.name=enabled?'Presentation collaborator':'Agent';
      data.agents=data.agents.filter(a=>!a.id.startsWith('review-peer-'));
      if(enabled)['Research collaborator','Image specialist','Presentation designer','Writing partner','Source reviewer'].forEach((name,i)=>data.agents.push({id:'review-peer-'+i,type:'agent',name,hat:['beret','construction','fedora','beanie','cap'][i],provider:'codex',retired:false}));
      persist();renderRoster();renderChat();updateBoardLabels();status(enabled?'Long-name and five-peer layout stress state. Names truncate within the unified capsule.':'Returned to the compact local participant names.');
    }
    if(action==='people')addPerson();
    if(action==='empty'){state.chat=p.id;data.threads[p.id]=[];persist();renderChat();status('Cleared this prototype’s local sample thread.');}
    if(action==='close'){state.chat=null;state.canvasChat=false;state.profile=null;state.popover=null;state.hats=null;renderProfile();renderChat();renderCanvasChat();renderRoster();}
    updateSnapshot();
  });
  document.querySelectorAll('[data-zoom]').forEach(b=>b.onclick=()=>setZoom(Number(b.dataset.zoom)));
  document.querySelectorAll('[data-chat-scale]').forEach(b=>b.onclick=()=>{
    state.canvasChatScale=b.dataset.chatScale;
    document.querySelectorAll('[data-chat-scale]').forEach(option=>{const selected=option.dataset.chatScale===state.canvasChatScale;option.classList.toggle('selected',selected);option.setAttribute('aria-pressed',String(selected));});
    positionCanvasChat();updateSnapshot();status(state.canvasChatScale==='overlay'?'Proposed exception: the attached chat stays readable in screen units while its anchor follows the canvas avatar.':'Attached chat follows canvas scale under P0.9. The avatar always follows canvas scale.');
  });
  document.querySelectorAll('[data-zoom-step]').forEach(b=>b.onclick=()=>{const steps=[.25,.5,1,1.5,2],index=steps.indexOf(state.zoom);setZoom(steps[Math.max(0,Math.min(steps.length-1,index+Number(b.dataset.zoomStep)))]);});
  $('#zoom-label').onclick=()=>setZoom(1);
  $('#board-agent').ondblclick=e=>{e.stopPropagation();listen();};
  $('#board-agent').onkeydown=e=>{if(e.key==='Enter'){e.preventDefault();listen();}};
  function setReviewCollapsed(closed){$('#review-panel').classList.toggle('collapsed',closed);$('#review-toggle').setAttribute('aria-expanded',String(!closed));$('#review-chevron').textContent=closed?'⌃':'⌄';}
  $('#review-toggle').onclick=()=>setReviewCollapsed(!$('#review-panel').classList.contains('collapsed'));
  $('#dialog-width').oninput=e=>{params.dialogWidth=Number(e.target.value);applyParams();};
  $('#export-snapshot').onclick=()=>{updateSnapshot();$('#snapshot-details').open=true;$('#snapshot').select();const blob=new Blob([$('#snapshot').value],{type:'application/json'}),url=URL.createObjectURL(blob),a=document.createElement('a');a.href=url;a.download=revision+'-snapshot.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),2000);status('Exported the current named parameters and view state.');};
  let drag=null;
  $('#canvas').onpointerdown=e=>{if(e.target.closest('button'))return;drag={x:e.clientX,y:e.clientY,pan:{...state.pan}};$('#canvas').setPointerCapture(e.pointerId);};
  $('#canvas').onpointermove=e=>{if(!drag)return;state.pan={x:drag.pan.x+e.clientX-drag.x,y:drag.pan.y+e.clientY-drag.y};updateWorld();};
  $('#canvas').onpointerup=()=>{drag=null;};
  document.addEventListener('pointerdown',e=>{
    if(state.profile&&!e.target.closest('.profile-popover,.roster,.chat-identity-name'))closeProfile();
    if(state.popover==='add'&&!e.target.closest('.add-popover,.add-button')){state.popover=null;renderProfile();renderRoster();}
    if(['overflow','provider'].includes(state.popover)&&!e.target.closest('.chat-header')){state.popover=null;renderChat();}
  });
  document.addEventListener('keydown',e=>{
    if(e.key!=='Escape'||e.isComposing||e.keyCode===229)return;
    if(!(state.hats&&state.chat)&&!state.popover&&!state.profile&&!state.canvasChat&&!state.chat)return;
    e.preventDefault();rememberDrafts();
    if(state.hats&&state.chat){state.hats=null;renderChat();}
    else if(state.pendingProvider&&state.popover==='provider'){state.pendingProvider=null;renderChat();}
    else if(state.popover){const previous=state.popover;state.popover=null;if(previous==='add'){renderProfile();renderRoster();}else renderChat();}
    else if(state.profile)closeProfile();
    else if(state.canvasChat){state.canvasChat=false;state.listening=false;renderCanvasChat();updateBoardState();}
    else if(state.chat){state.chat=null;state.hats=null;renderChat();}
    updateSnapshot();
  });
  let narrowReview=innerWidth<=950;
  addEventListener('resize',()=>{const narrow=innerWidth<=950;if(narrow&&!narrowReview)setReviewCollapsed(true);narrowReview=narrow;updateWorld();hatFlowView?.resize();if(state.profile)renderProfile();updateSnapshot();});
  if(narrowReview)setReviewCollapsed(true);
  renderRoster();renderChat();updateBoardVisual();applyParams();updateWorld();requestAnimationFrame(animate);
  globalThis.PresencePrototype={params,revision,getState:()=>JSON.parse(JSON.stringify({state,data})),setZoom,openChat,listen,snapshot:()=>JSON.parse($('#snapshot').value)};
  $('#slate').removeAttribute('inert');$('#slate').setAttribute('aria-busy','false');$('#prototype-loading')?.remove();
  document.documentElement.dataset.prototypeStatus='ready';
})();
