(function () {
  'use strict';
  const data = window.AgentStudioData;
  const renderer = window.AgentStudioScene;
  if (!data || !renderer) { document.getElementById('scene-root').textContent = 'The studio files did not load. Keep index.html, data.js, scene.js, app.js, and the styles together.'; return; }
  const $ = selector => document.querySelector(selector);
  const esc = value => String(value ?? '').replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
  const clone = value => JSON.parse(JSON.stringify(value));
  const key = 'slate-agent-studio-v1';
  const accessKey = 'slate-agent-studio-global-access-v1';
  const credentialKey='slate-agent-studio-credential-status-v1';
  const accessNames = {ask:'Ask before actions',workspace:'Workspace access',full:'Full access'};
  const layouts = {window:'One conversation window',pairs:'Message pairs',messages:'One message per card'};
  const accessControl=document.getElementById('global-access');
  const paths = {
    chat:'M4 5h16v11H9l-5 4Z',source:'M5 4h10l4 4v12H5Z M14 4v5h5 M8 13h8 M8 16h6',image:'M3 4h18v16H3Z M3 16l6-6 5 5 3-3 4 4 M16 8h.01',
    text:'M5 4h14v16H5Z M8 8h8 M8 12h8 M8 16h5',dashboard:'M3 4h18v16H3Z M3 9h18 M10 9v11',agent:'M6 6h12a3 3 0 0 1 3 3v8H3V9a3 3 0 0 1 3-3Z M8 10v3 M16 10v3 M12 3v3 M9 20h6',
    chevron:'m9 5 7 7-7 7',down:'m6 9 6 6 6-6',back:'m14 5-7 7 7 7',check:'m5 12 4 4L19 6',provider:'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Z M3 12h18 M12 3c5 5 5 13 0 18-5-5-5-13 0-18Z',
    model:'m12 3 9 5-9 5-9-5Z M3 12l9 5 9-5 M3 16l9 5 9-5',effort:'M12 3a6 6 0 0 0-4 10.5V17h8v-3.5A6 6 0 0 0 12 3Z M9 21h6',speed:'m13 2-8 12h6l-1 8L20 9h-7Z',
    lock:'M6 10h12v11H6Z M8 10V7a4 4 0 0 1 8 0v3',settings:'M5 6h14 M5 12h14 M5 18h14 M9 3v6 M15 9v6 M10 15v6',more:'M5 12h.01 M12 12h.01 M19 12h.01',
    play:'m8 4 12 8-12 8Z',stop:'M6 6h12v12H6Z',pause:'M7 4h3v16H7Z M14 4h3v16h-3Z',send:'m3 3 19 9-19 9 4-9Z M7 12h15',close:'m6 6 12 12 M6 18 18 6',
    key:'M14 10a5 5 0 1 0 0-1 M10 13l-7 7 M5 18l2 2 M8 15l2 2',plus:'M12 4v16 M4 12h16',layout:'M3 4h18v16H3Z M9 4v16',fork:'M7 3v9a5 5 0 0 0 5 5h5 M17 12V5 M14 8l3-3 3 3 M14 14l3 3-3 3',
    clock:'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Z M12 7v5l3 2',history:'M3 10a9 9 0 1 1 0 5 M3 4v6h6 M12 7v5l3 2',copy:'M8 8h12v13H8Z M4 16V3h12',save:'M4 3h13l4 4v14H4Z M8 3v6h9 M8 14h9v7',
    paperclip:'m8 13 6-6a3 3 0 0 1 4 4l-8 8a5 5 0 0 1-7-7l9-9 M7 14l7-7',sun:'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z M12 2v2 M12 20v2 M2 12h2 M20 12h2 M5 5l1 1 M18 18l1 1 M5 19l1-1 M18 6l1-1',
    moon:'M20 15A9 9 0 0 1 9 3a9 9 0 1 0 11 12Z',eye:'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z',trash:'M3 6h18 M8 6V3h8v3 M6 6l1 15h10l1-15 M10 10v7 M14 10v7',
    wire:'M3 5h5c8 0 0 14 8 14h5 M3 3v4 M21 17v4',info:'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Z M12 10v7 M12 7h.01',refresh:'M20 10a8 8 0 0 0-14-5L3 8 M3 3v5h5 M4 14a8 8 0 0 0 14 5l3-3 M16 16h5v5'
  };
  const icon = name => `<svg class="ui-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="${paths[name] || paths.settings}"/></svg>`;
  const modeIcon = {chat:'chat','source-chat':'source',image:'image',text:'text',dashboard:'dashboard','free-agent':'agent'};
  const read = (name,fallback) => { try { return JSON.parse(localStorage.getItem(name)) || fallback; } catch (_) { return fallback; } };
  const write = (name,value) => { try { localStorage.setItem(name,JSON.stringify(value)); return true; } catch (_) { return false; } };
  let saved = read(key,{version:1,mode:'chat',modes:{},notes:{},theme:'light'});
  if (saved.version !== 1) saved = {version:1,mode:'chat',modes:{},notes:{},theme:'light'};
  saved.modes ||= {}; saved.notes ||= {};
  let access = read(accessKey,{policy:'ask',boardAuto:false,updated:0});
  let credential=read(credentialKey,{status:'missing'});
  if (!accessNames[access.policy]) access.policy = 'ask';
  let mode = data.modalities.some(item=>item.id===saved.mode) ? saved.mode : 'chat';
  let scenarioId = '';
  let state;
  const liveModes=new Map();
  let menus = [];
  let menuTimer,tipTimer,toastTimer;
  let editingDialog = null;
  let channel;
  try { channel = new BroadcastChannel('slate-agent-studio-access'); } catch (_) {}
  const provider = () => data.providers.find(item=>item.id===state.provider) || data.providers[0];
  const availableModels = () => provider().models.filter(item=>item.id==='gpt-image-2'?mode==='image':item.id==='sample-api-text'?mode!=='image':true);
  const model = () => availableModels().find(item=>item.id===state.model) || availableModels()[0];
  const busy = () => ['running','waiting','approval','paused'].includes(state.status);
  const canSteer = () => busy() && (['dashboard','free-agent','chat','source-chat'].includes(mode) || state.interactive);
  const modeDefinition = () => data.modalities.find(item=>item.id===mode);
  const time = seconds => `${Math.floor(Math.max(0,seconds)/60)}:${String(Math.floor(Math.max(0,seconds)%60)).padStart(2,'0')}`;
  const initialMessages = type => type==='source-chat' ? [{role:'user',text:'Use this project dashboard as a starting point.'},{role:'assistant',text:'I can help revise it. What would you like to improve first?'}] : [{role:'user',text:'Help me explore a quieter layout for this project.'},{role:'assistant',text:'We can keep the important information visible and reduce the supporting controls. Which part should we refine first?'}];
  function fresh(type) {
    const definition = data.modalities.find(item=>item.id===type);
    return {modality:type,provider:'codex',model:'gpt-6.1-sol',effort:'Medium',speed:'Standard',presentation:definition.defaultPresentation || 'pairs',interactive:definition.interactive,
      continuity:type==='dashboard',conversationEpoch:1,status:'idle',prompt:definition.defaultPrompt,messages:initialMessages(type),
      revisions:['image','dashboard'].includes(type)?[{id:1,title:type==='image'?'Image 1':'Revision 1'},{id:2,title:type==='image'?'Image 2':'Revision 2'},{id:3,title:type==='image'?'Image 3':'Revision 3'}]:[],selectedRevision:0,appliedRevision:0,
      activity:[],activityExpanded:false,elapsedSeconds:0,lastEventSeconds:0,pendingSteering:'',sourceTitle:type==='source-chat'?'Project overview dashboard':type==='image'?'Earlier image and dialogue':'Earlier dashboard and dialogue',
      keyState:'missing',generationMode:'request',imageCount:1,aspect:'3:2',seed:'Random',attachment:false,liveEffort:true,effectiveEffort:'Medium',effectiveAccess:access.policy,boardAuto:access.boardAuto,
      quiet:false,autoProgress:false,startedAt:0,lastEventAt:0,nextStep:0,runCount:0,notes:'',branches:[],activeBranch:'main',placed:0,placedResults:[],queuedMessages:[],upstreamMessages:['image','dashboard'].includes(type)?initialMessages(type):[],lastRunContext:null};
  }
  function restore(type) {
    const result = {...fresh(type),...(saved.modes[type] || {}),modality:type};
    if (!data.providers.some(item=>item.id===result.provider)) result.provider='codex';
    const p=data.providers.find(item=>item.id===result.provider);
    if (!p.models.some(item=>item.id===result.model)) result.model=p.models[0].id;
    if (['running','waiting','approval','paused'].includes(result.status)) {
      result.status='idle'; result.autoProgress=false;
      result.activity.push({text:'Sample task is not resumed after reopening. Conversation, choices, and outputs are retained.',kind:'status'});
    }
    return result;
  }
  state=restore(mode);
  function persist() { saved.mode=mode; saved.modes[mode]=clone(state); write(key,saved); }
  function feedback(text) { $('#flow-feedback').textContent=text; }
  function toast(text) { $('#toast').textContent=text; $('#toast').hidden=false; clearTimeout(toastTimer); toastTimer=setTimeout(()=>{$('#toast').hidden=true;},3800); feedback(text); }
  function event(text,kind='status') { state.activity.push({text,kind,time:time(state.elapsedSeconds)}); state.activity=state.activity.slice(-30); state.lastEventSeconds=0; state.lastEventAt=Date.now(); }
  function closeMenus(level=0) { clearTimeout(menuTimer); for (const menu of menus.splice(level)) menu.element.remove(); document.querySelectorAll('[data-menu][aria-expanded=true]').forEach(item=>item.setAttribute('aria-expanded','false')); hideTip(); }
  function setMode(next) {
    if (!data.modalities.some(item=>item.id===next)) return;
    persist(); liveModes.set(mode,state); closeMenus(); mode=next; state=liveModes.get(mode) || restore(mode); scenarioId=''; render(); persist();
    feedback('This mode keeps its own remembered model, draft, and layout. Access is shared globally.');
  }
  function normalizeModel() {
    const m=model();
    state.model=m.id;
    if (!m.efforts.includes(state.effort)) state.effort=m.efforts.includes('Medium')?'Medium':m.efforts[0] || '';
    if (!m.speeds.includes(state.speed)) state.speed=m.speeds[0] || '';
    if(state.capabilityModel!==m.id){state.liveEffort=Boolean(m.canChangeEffortWhileRunning);state.capabilityModel=m.id;}
    if (!busy()) state.effectiveEffort=state.effort;
  }
  function toolbar() {
    const p=provider(),m=model();
    const symbol=p.id==='codex'?'C':p.id==='cursor'?'↗':p.id==='openai'?'O':'L';
    return `<div class="agent-toolbar"><button class="control provider-chip" data-menu="agent" aria-haspopup="menu" aria-expanded="false" data-tooltip="Choose the service and model. Effort and speed are separate settings."><span class="provider-symbol" aria-hidden="true">${symbol}</span>${esc(p.label)} <span aria-hidden="true">·</span> ${esc(m.label)}${icon('down')}</button>${m.efforts.length?`<button class="control" data-menu="effort" aria-haspopup="menu" data-tooltip="How much thinking work to request. Supported models can change it during a task.">${icon('effort')}Effort · ${esc(state.effort)}${icon('down')}</button>`:''}${m.speeds.length>1?`<button class="control" data-menu="speed" aria-haspopup="menu" data-tooltip="Response-speed option. Separate from thinking effort.">${icon('speed')}${esc(state.speed)}${icon('down')}</button>`:''}<span class="agent-access-slot" data-global-access-slot></span><button class="icon-button" data-menu="settings" aria-haspopup="menu" aria-label="Agent settings" data-tooltip="Conversation layout and tool-specific settings.">${icon('settings')}</button><button class="icon-button" data-menu="more" aria-haspopup="menu" aria-label="More agent actions">${icon('more')}</button></div>${['image','dashboard'].includes(mode)?`<div class="setting-strip">${mode==='image'?`<button class="mode-toggle ${state.interactive?'is-on':''}" data-action="toggle-interactive" aria-pressed="${state.interactive}" data-tooltip="When on, keep a continuing dialogue and choose any chat-train layout.">${icon('chat')}Interactive agent · ${state.interactive?'On':'Off'}</button><button class="control" data-menu="generation" aria-haspopup="menu">${esc(state.generationMode==='request'?'Generate on request':'Regenerate when inputs change')}${icon('down')}</button>`:`<button class="control" data-menu="presentation" aria-haspopup="menu">${icon('layout')}${esc(layouts[state.presentation])}${icon('down')}</button>`}${mode==='image'?`<button class="control" data-menu="image-options" aria-haspopup="menu">${esc(state.imageCount)} image${state.imageCount>1?'s':''} · ${esc(state.aspect)}${icon('down')}</button>`:''}</div>`:''}`;
  }
  function composer() {
    const running=busy(),steering=canSteer(),singleControl=running&&!steering&&['image','text'].includes(mode);
    const label=singleControl?'Stop':steering?'Send update':mode==='image'?'Generate':mode==='text'?'Write':mode==='dashboard'?'Build':'Send';
    return `<div class="composer-box">${state.attachment?`<div class="attachment-chip">${icon('image')}Reference image · ${state.inputVersion || 1}<button data-action="remove-attachment" aria-label="Remove reference image">×</button></div>`:''}<textarea id="agent-prompt" rows="2" aria-label="Message or instructions" placeholder="${running?'Add a direction while the agent works…':'Describe what you want…'}">${esc(state.prompt)}</textarea><div class="composer-actions"><button class="icon-button" data-action="attach" aria-label="Attach sample reference image" data-tooltip="Add a sample image as an explicit reference.">${icon('paperclip')}</button><span class="composer-hint">${running?'Task continues · draft stays editable':'Ctrl + Enter to send'}</span>${mode==='free-agent'&&running?`<button class="icon-button" data-action="${state.status==='paused'?'resume':'pause'}" aria-label="${state.status==='paused'?'Resume task':'Pause task'}" data-tooltip="${state.status==='paused'?'Continue the paused task.':'Pause work and keep the conversation.'}">${icon(state.status==='paused'?'play':'pause')}</button>`:''}${running&&!singleControl?`<button class="icon-button" data-action="stop" aria-label="Stop task" data-tooltip="End the current task and retain its conversation and outputs.">${icon('stop')}</button>`:''}<button class="primary-button" data-action="${singleControl?'stop':'send'}">${icon(singleControl?'stop':mode==='image'&&!running?'play':'send')}${label}</button></div></div>`;
  }
  function inlineFeedback() {
    if (state.status==='approval') return `<div class="inline-feedback"><strong>Read a reference outside the workspace?</strong><span>Sample target: shared research folder</span><div><button class="control" data-action="allow">Allow once</button><button class="control" data-action="deny">Refuse</button></div></div>`;
    if (state.status==='error') {const keyError=provider().needsKey&&state.keyState!=='valid';return `<div class="inline-feedback is-error"><strong>${keyError?(state.keyState==='expired'?'The saved API key was refused':'No API key is saved'):'The sample service did not complete the request'}</strong><span>Your draft, references, and results are retained.</span><div><button class="control" data-action="${keyError?'change-key':'retry'}">${keyError?(state.keyState==='expired'?'Change API key':'Add API key'):'Retry request'}</button></div></div>`;}
    return '';
  }
  function render() {
    state.keyState=credential.status;
    normalizeModel();
    liveModes.set(mode,state);
    $('#mode-nav').innerHTML=data.modalities.map(item=>`<button class="mode-link ${mode===item.id?'is-active':''}" data-mode="${item.id}" aria-current="${mode===item.id?'page':'false'}" data-tooltip="${esc(item.description)}">${icon(modeIcon[item.id])}<span>${esc(item.label)}</span></button>`).join('');
    $('#mode-select').innerHTML=data.modalities.map(item=>`<option value="${item.id}" ${item.id===mode?'selected':''}>${esc(item.label)}</option>`).join('');
    const d=modeDefinition();
    $('#mode-title').textContent=d.label;
    $('#mode-kicker').textContent=['image','text','dashboard'].includes(mode)?'Generated work':'Conversation';
    $('#mode-description').textContent=d.description;
    const scenario=data.scenarios.find(item=>item.id===scenarioId);
    $('#scenario-select').innerHTML=`<option value="">Choose a sample flow…</option>`+data.scenarios.map(item=>`<option value="${item.id}" ${item.id===scenarioId?'selected':''}>${esc(item.label)}</option>`).join('');
    $('#scenario-help').textContent=scenario?.description || 'Use the controls directly, or load a sample state to inspect a specific interaction.';
    $('#scene-root').innerHTML=renderer.render(state);
    $('#scene-root .scene-caption')?.remove();
    document.querySelectorAll('[data-toolbar-slot]').forEach(slot=>{slot.innerHTML=toolbar();});
    $('[data-global-access-slot]').append(accessControl);
    document.querySelectorAll('[data-composer-slot]').forEach(slot=>{slot.innerHTML=inlineFeedback()+composer();});
    $('#flow-title').textContent=scenario?.label || 'The same control language';
    $('#flow-explanation').textContent=scenario?.expected || 'Provider, model, effort, and speed share one predictable picker. Secondary actions follow the same icon / label / check / chevron formula.';
    const checks=['Model selector is available before sending','Access is remembered globally'];
    if (['image','dashboard'].includes(mode)) checks.push('Fourth input explicitly carries continuity');
    if (state.interactive) checks.push('Changing layout keeps messages and draft');
    if (busy()) checks.push('Current-task settings remain inspectable');
    $('#flow-checks').innerHTML=checks.map(text=>`<div class="flow-check">${icon('check')}<span>${esc(text)}</span></div>`).join('');
    $('#live-effort-support').checked=state.liveEffort;
    $('#live-effort-support').disabled=!model().efforts.length;
    $('#design-comment').value=saved.notes[mode] || '';
    $('#feedback-mode-label').textContent=d.label;
    updateAccessButton(); updateEffective(); updateClocks(); persist();
  }
  function updateEffective() {
    const values=[['Provider',provider().label],['Model',model().label],['Effort now',state.effectiveEffort || 'Not offered'],['Effort next',state.effort || 'Not offered'],['Speed',state.speed || 'Not offered'],['Access now',accessNames[busy()?state.effectiveAccess:access.policy]],['Global access',accessNames[access.policy]],['Board changes',access.boardAuto?'Apply directly':'Review before apply'],['Context',state.continuity?`Connected · path ${state.conversationEpoch}`:`Fresh · path ${state.conversationEpoch}`]];
    $('#effective-settings').innerHTML=values.map(([label,value])=>`<div><dt>${esc(label)}</dt><dd>${esc(value)}</dd></div>`).join('');
  }
  function updateAccessButton() { accessControl.innerHTML=`${icon('lock')}<span>Access · ${esc(accessNames[access.policy])}</span>${icon('down')}`; }
  function updateClocks() {
    if (['running','waiting','approval'].includes(state.status) && state.startedAt) state.elapsedSeconds=Math.floor((Date.now()-state.startedAt)/1000);
    if (state.lastEventAt && busy()) state.lastEventSeconds=Math.floor((Date.now()-state.lastEventAt)/1000);
    const badge=$('#flow-status');
    badge.textContent={idle:'Ready',running:'Working',waiting:'Waiting for service',approval:'Needs permission',paused:'Paused',error:'Needs attention',completed:'Complete',stopped:'Stopped'}[state.status] || 'Ready';
    badge.className='flow-badge'+(['running','waiting'].includes(state.status)?' is-running':'')+(state.status==='error'?' is-error':'');
    const meta=$('.activity-heading .activity-meta');
    if (meta) meta.textContent=state.status==='idle'?'No run in progress':`${time(state.elapsedSeconds)} elapsed · Last update ${time(state.lastEventSeconds)} ago`;
  }
  function loadScenario(id) {
    const scenario=data.scenarios.find(item=>item.id===id); if (!scenario) return;
    persist(); closeMenus(); mode=scenario.modality; state={...fresh(mode),...(saved.modes[mode]||{}),modality:mode};
    scenarioId=id; state.status=scenario.status || 'idle'; state.quiet=Boolean(scenario.quiet); state.autoProgress=false; state.activity=[]; state.pendingSteering='';
    if (scenario.provider) { state.provider=scenario.provider; state.model=data.providers.find(p=>p.id===state.provider).models[0].id; }
    if (scenario.keyState) setCredentialStatus(scenario.keyState);
    if (scenario.interactive != null) { state.interactive=scenario.interactive; state.continuity=scenario.interactive; }
    if (id==='continuity-cutoff') state.continuity=true;
    if (id==='dashboard-steering') { state.provider='codex'; state.model='gpt-6.1-sol'; state.liveEffort=true; }
    state.elapsedSeconds=state.quiet?84:state.status==='idle'?0:16; state.lastEventSeconds=state.quiet?48:3;
    state.startedAt=Date.now()-state.elapsedSeconds*1000; state.lastEventAt=Date.now()-state.lastEventSeconds*1000;
    state.effectiveAccess=access.policy;
    if (id==='approval-wait') state.effectiveAccess='ask';
    if (state.status!=='idle') state.activity=[{text:state.quiet?'Sample: references prepared; waiting for the service.':state.status==='error'?'Sample: request failed; inputs retained.':'Sample: connected references prepared.',kind:'status',time:'0:03'}];
    if (state.quiet) state.status='waiting';
    render(); feedback(scenario.expected);
  }
  function startRun(explicitText) {
    const text=typeof explicitText==='string'?explicitText:state.prompt.trim();
    if (!text.trim()) { toast('Add an instruction before sending.'); return; }
    if (provider().needsKey && state.keyState!=='valid') { state.status='error'; event(state.keyState==='expired'?'Saved API key was refused. Choose Change API key.':'No API key is saved. Use the API row chevron to add one.','error'); render(); toast('Use Provider → OpenAI API → Add / Change API key. Your draft stays intact.'); return; }
    if(state.continuity && !state.messages.length && state.upstreamMessages?.length) state.messages=clone(state.upstreamMessages);
    if (mode==='image'&&!state.interactive) { state.messages=[]; state.conversationEpoch++; }
    state.lastRunContext={path:state.conversationEpoch,inherited:state.continuity&&(mode!=='image'||state.interactive)?clone(state.upstreamMessages || []):[],history:clone(state.messages)};
    state.lastInstruction=text;
    state.runSettings={count:state.imageCount,aspect:state.aspect,seed:state.seed};
    state.messages.push({role:'user',text}); if(typeof explicitText!=='string')state.prompt=''; state.status='running'; state.quiet=false; state.autoProgress=true; state.elapsedSeconds=0; state.lastEventSeconds=0;
    state.startedAt=Date.now();state.lastEventAt=Date.now();state.nextStep=0;state.runCount++;state.effectiveAccess=access.policy;state.effectiveEffort=state.effort;
    event('Sample task started with your selected model and explicit inputs.','status'); render(); feedback('Sample run started. Try sending another instruction or changing effort while it works.');
  }
  function send() {
    if (!busy()) { startRun(); return; }
    if(!canSteer()){toast('This request is independent. Stop it first, or wait until it finishes before sending another.');return;}
    if (!state.prompt.trim()) { toast('Write the update you want to send.'); return; }
    const text=state.prompt.trim();state.prompt='';
    const delivered=state.provider==='codex' && ['running','waiting'].includes(state.status);
    if(delivered)state.messages.push({role:'user',text});else(state.queuedMessages ||= []).push(text);
    state.pendingSteering=(delivered?'Delivered to the current sample task: ':'Queued for the next turn: ')+text;
    event(delivered?'Your follow-up reached the current sample task.':'Your follow-up is queued; the current task has not received it.','commentary');render();toast(state.pendingSteering);
  }
  function stop() { updateClocks();state.status='stopped';state.autoProgress=false;state.liveRefreshPending=false;state.pendingSteering='';if(state.queuedMessages?.length){state.prompt=[state.prompt,...state.queuedMessages].filter(Boolean).join('\n\n');state.queuedMessages=[];}event('Sample task stopped. Conversation, outputs, and any queued text retained.');render();toast('Stopped. Queued updates stay in the draft for an explicit next send.'); }
  function changeEffort(value) {
    state.effort=value;
    if (busy() && state.liveEffort && ['running','waiting'].includes(state.status)) { state.effectiveEffort=value;event(`Sample capability: effort changed to ${value} for the running task.`);toast(`Effort ${value} applies to the current sample task.`); }
    else if (busy()) { toast(`Effort ${value} is queued for the next turn. This task keeps ${state.effectiveEffort || 'its current setting'}.`); }
    else { state.effectiveEffort=value;toast(`Effort set to ${value}.`); }
    closeMenus(); render();
  }
  function setGlobalAccess(policy) {
    access={...access,policy,updated:Date.now()};write(accessKey,access);channel?.postMessage(access);
    if (busy()) event(`Global access changed to ${accessNames[policy]}. Current provider run keeps its captured access; next run uses the global choice.`);
    closeMenus();render();toast('Global preference saved for every mode, open studio window, and later visit.');
  }
  function receiveAccess(value) {
    if (!value || !accessNames[value.policy] || value.updated < access.updated) return;
    access=value; updateAccessButton();updateEffective();feedback('Global access preference updated from another studio instance.');
  }
  if (channel) channel.onmessage=message=>receiveAccess(message.data);
  function setCredentialStatus(status){credential={status};write(credentialKey,credential);state.keyState=status;for(const item of liveModes.values())item.keyState=status;}
  window.addEventListener('storage',e=>{if(e.key===accessKey){try{receiveAccess(JSON.parse(e.newValue));}catch(_){}}if(e.key===credentialKey){try{const next=JSON.parse(e.newValue);if(['missing','expired','valid'].includes(next?.status)){credential=next;closeMenus();render();}}catch(_){}}});
  function cutContinuity() {
    if (!state.continuity) return;
    if(!state.upstreamMessages?.length)state.upstreamMessages=clone(state.messages);
    if(busy()){updateClocks();state.status='stopped';state.autoProgress=false;if(state.queuedMessages?.length){state.prompt=[state.prompt,...state.queuedMessages].filter(Boolean).join('\n\n');state.queuedMessages=[];}event('Downstream sample task stopped because its continuity input changed. Queued text is retained in the draft.');}
    state.continuity=false;state.conversationEpoch++;state.messages=[];
    state.liveRefreshPending=false;
    event('Continuity wire cut. Later turns start fresh; previous outputs and the upstream conversation remain available.');render();toast('Downstream context reset. Outputs and your draft were kept.');
  }
  function connectContinuity() { state.continuity=true;if(mode==='image')state.interactive=true;event('Fourth input connected to the earlier conversation checkpoint. No generation was started.');render();toast(mode==='image'?'Continuity connected and interactive dialogue enabled for the next request.':'Continuity connected. It will be used by the next request.'); }
  function createDialog(title,body,actions) {
    closeMenus(); hideTip();
    $('#dialog-layer').innerHTML=`<div class="dialog-backdrop"><section class="dialog" role="dialog" aria-modal="true" aria-labelledby="dialog-title"><div class="dialog-header"><h2 id="dialog-title">${esc(title)}</h2><button class="icon-button" data-action="close-dialog" aria-label="Close dialog">${icon('close')}</button></div>${body}<div class="dialog-actions">${actions || '<button class="control" data-action="close-dialog">Close</button>'}</div></section></div>`;
    const first=$('#dialog-layer input,#dialog-layer textarea,#dialog-layer select,#dialog-layer button'); first?.focus();
  }
  function closeDialog() { $('#dialog-layer').innerHTML='';editingDialog=null; }
  function keyDialog() {
    editingDialog='key';
    createDialog(state.keyState==='missing'?'Add API key':'Change API key',`<p>Use a sample credential to explore this flow. The composer and existing images stay in place.</p><label for="demo-key">API key · masked entry</label><input id="demo-key" type="password" autocomplete="off" placeholder="Sample credential" aria-describedby="key-demo-note"><p id="key-demo-note">Prototype: only the saved / expired status is retained. Credential text is not stored or sent.</p><button class="control" data-action="sample-key">Use sample key</button><div class="demo-key-status" id="key-validation" role="status"></div>`,`<button class="control" data-action="close-dialog">Cancel</button><button class="primary-button" data-action="save-key">Save key</button>`);
    $('#demo-key').focus();
  }
  function saveKey() {
    if (!$('#demo-key').value.trim()) { $('#key-validation').textContent='Enter a sample value or choose Use sample key.';return; }
    $('#demo-key').value='';setCredentialStatus('valid');if(state.status==='error')state.status='idle';event('Sample credential replaced. The next explicit request uses the saved status.');closeDialog();render();toast('Key changed for this provider across all modes. Draft and outputs are unchanged.');
  }
  function showDecisions() {
    createDialog('Your design references',`<p>Selected examples guide the proposal. Rows left unselected remain open for exploration.</p>${data.decisions.map(item=>`<div class="decision-item"><strong>${esc(item.title)}</strong><p>${esc(item.detail)}</p><span class="decision-source">Survey: ${esc(item.source)}</span></div>`).join('')}`);
  }
  function reviewPreview(action) {
    const revision=state.revisions[state.selectedRevision];
    createDialog(action==='apply'?'Apply this dashboard preview?':'Discard this preview?',`<p>${esc(revision?.title || 'Selected revision')} → Project dashboard on the sample board</p><p>${action==='apply'?'This makes the selected version the applied dashboard. Other revisions stay in history.':'Return to the applied version. The generated revision remains available in history.'}</p>`,`<button class="control" data-action="close-dialog">Keep reviewing</button><button class="primary-button" data-action="${action==='apply'?'confirm-apply':'confirm-discard'}">${action==='apply'?'Apply selected version':'Discard preview'}</button>`);
  }
  function showMemory() {
    createDialog('Conversation context',`<dl><div><dt>Fourth input</dt><dd>${state.continuity?'Connected':'Disconnected'}</dd></div><div><dt>Downstream conversation</dt><dd>Path ${state.conversationEpoch}</dd></div><div><dt>Last request inherited</dt><dd>${state.lastRunContext?.inherited?.length || 0} messages</dd></div></dl><p>${state.continuity?'The next request inherits the connected conversation checkpoint.':'The next request starts without inherited conversation context.'}</p><p>Existing outputs: ${state.revisions.length}. Draft instructions are retained. Public work updates are separate from conversation context.</p>${state.upstreamMessages?.length?`<details><summary>Retained upstream conversation</summary>${state.upstreamMessages.map(item=>`<p><strong>${esc(item.role==='user'?'You':'Agent')}</strong> · ${esc(item.text)}</p>`).join('')}</details>`:''}`);
  }
  function forkDialog() {
    createDialog('Branch this conversation',`<p>Choose the point where a new direction starts. The original conversation remains available.</p><label for="fork-checkpoint">Continue after</label><select id="fork-checkpoint">${state.messages.map((item,index)=>`<option value="${index}" ${index===state.messages.length-1?'selected':''}>${index+1}. ${esc(item.role==='user'?'You':'Agent')}: ${esc(item.text.slice(0,65))}</option>`).join('')}</select>`,`<button class="control" data-action="close-dialog">Cancel</button><button class="primary-button" data-action="confirm-fork">Create branch</button>`);
  }
  function fork() {
    const through=Number($('#fork-checkpoint')?.value || state.messages.length-1);
    captureBranch();state.activeBranch=`branch-${Date.now()}`;state.conversationTitle=`Branch ${state.branches.length+1}`;
    state.messages=state.messages.slice(0,through+1);state.conversationEpoch++;state.status='idle';state.autoProgress=false;closeDialog();render();toast('New branch created at that message. The original is under More → Saved conversations.');
  }
  function captureBranch(){const record={id:state.activeBranch,label:state.conversationTitle||'Original conversation',messages:clone(state.messages),epoch:state.conversationEpoch,prompt:state.prompt,sourceTitle:state.sourceTitle,sourceImageIndex:state.sourceImageIndex};const at=state.branches.findIndex(item=>item.id===record.id);if(at<0)state.branches.push(record);else state.branches[at]=record;}
  function sourceChat(){const revision=clone(state.revisions[state.selectedRevision]),index=revision.variant??state.selectedRevision;persist();const prior=liveModes.get('source-chat')||saved.modes['source-chat'];mode='source-chat';state=fresh(mode);state.sourceTitle=`${revision?.title||'Selected image'} · Image generator`;state.sourceImageIndex=index;state.messages=[];state.prompt='Help me refine this image.';if(prior)state.branches=[...clone(prior.branches||[]),{id:`source-${Date.now()}`,label:prior.conversationTitle||'Earlier source conversation',messages:clone(prior.messages),prompt:prior.prompt,epoch:prior.conversationEpoch,sourceTitle:prior.sourceTitle,sourceImageIndex:prior.sourceImageIndex}];scenarioId='source-chat';render();toast('New chat opened from the selected image. Earlier source conversations are saved separately.');}
  function inputChanged(){state.inputVersion=(state.inputVersion||1)+1;event(`Sample reference changed to version ${state.inputVersion}.`);render();if(mode==='image'&&state.generationMode==='inputs'){if(busy()){state.liveRefreshPending=true;toast('Input changed. A refreshed sample result will follow the current run.');}else startRun(state.lastInstruction||state.prompt||modeDefinition().defaultPrompt);}else toast('Reference changed. Generate on request waits for your explicit send.');}
  function download(name,content,type='application/json') { const url=URL.createObjectURL(new Blob([content],{type}));const a=document.createElement('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1200); }
  function exportReview() { persist();download('agent-studio-review.json',JSON.stringify({version:1,notes:saved.notes,globalAccess:access,modes:saved.modes,designDecisions:data.decisions,sourceSurvey:'survey-preferences.json'},null,2));toast('Review exported with your notes and explored settings.'); }
  function rowsFor(name) {
    const row=(label,iconName,extra={})=>({label,icon:iconName,...extra});
    if (name==='agent') return [row('Provider','provider',{value:provider().label,submenu:'providers'}),row('Model','model',{value:model().label,submenu:'models'}),...(model().efforts.length?[row('Effort','effort',{value:state.effort,submenu:'effort'})]:[]),...(model().speeds.length>1?[row('Speed','speed',{value:state.speed,submenu:'speed'})]:[]),{separator:true},row('Access','lock',{value:accessNames[access.policy],submenu:'access'})];
    if (name==='providers') return data.providers.map(item=>row(item.label,'provider',{checked:item.id===state.provider,action:'provider',valueId:item.id,disabled:busy(),submenu:item.needsKey?'credentials':undefined,split:item.needsKey,help:item.needsKey?'Select API mode with the row. Use its arrow to add or change the key.':`Use the ${item.label} sample connection.`}));
    if (name==='models') return availableModels().map(item=>row(item.label,'model',{checked:item.id===state.model,action:'model',valueId:item.id,disabled:busy(),help:'Sample catalog entry. The running request keeps its original model.'}));
    if (name==='effort') return model().efforts.map(value=>row(value,'effort',{checked:value===state.effort,action:'effort',valueId:value,help:busy()?(state.liveEffort?'Apply to the supported running sample task.':'Queue for the next turn. The current task keeps its setting.'):'Requested thinking effort.'}));
    if (name==='speed') return model().speeds.map(value=>row(value,'speed',{checked:value===state.speed,action:'speed',valueId:value,disabled:busy()}));
    if (name==='access') return [...Object.entries(accessNames).map(([id,label])=>row(label,'lock',{checked:access.policy===id,action:'access',valueId:id})),{separator:true},row('Board changes','dashboard',{value:access.boardAuto?'Apply directly':'Review first',submenu:'board-access'})];
    if (name==='board-access') return [row('Review before applying','eye',{checked:!access.boardAuto,action:'board-access',valueId:'review'}),row('Apply directly','dashboard',{checked:access.boardAuto,action:'board-access',valueId:'auto'})];
    if (name==='credentials') return [row(state.keyState==='missing'?'Add API key':'Change API key','key',{action:'change-key'}),...(state.keyState!=='missing'?[row('Remove API key','trash',{action:'remove-key',danger:true})]:[])];
    if (name==='settings') return [row('Conversation layout','layout',{value:layouts[state.presentation],submenu:'presentation',disabled:mode==='free-agent'}),...(['image','dashboard'].includes(mode)?[row('Continuity','wire',{value:state.continuity?'Connected':'Disconnected',submenu:'continuity'})]:[]),...(mode==='image'?[row('Generation trigger','refresh',{submenu:'generation'}),row('Image options','image',{submenu:'image-options'}),row('Interactive agent','chat',{checked:state.interactive,action:'toggle-interactive'})]:[]),{separator:true},row('Access','lock',{submenu:'access'})];
    if (name==='presentation') return Object.entries(layouts).map(([id,label])=>row(label,'layout',{checked:state.presentation===id,action:'presentation',valueId:id}));
    if (name==='continuity') return [row(state.continuity?'Cut continuity wire':'Connect continuity wire','wire',{action:state.continuity?'cut-continuity':'connect-continuity'}),row('Inspect context','eye',{action:'open-memory'})];
    if (name==='generation') return [row('Generate on request','play',{checked:state.generationMode==='request',action:'generation',valueId:'request'}),row('Regenerate when inputs change','refresh',{checked:state.generationMode==='inputs',action:'generation',valueId:'inputs'})];
    if (name==='image-options') return [row('Image count','image',{value:String(state.imageCount),submenu:'image-count'}),row('Aspect ratio','image',{value:state.aspect,submenu:'aspect'}),row('Seed','settings',{value:state.seed,submenu:'seed'}),{separator:true},row('Change sample reference','refresh',{action:'sample-input-change'})];
    if (name==='image-count') return [1,2,3,4].map(value=>row(`${value} image${value>1?'s':''}`,'image',{checked:state.imageCount===value,action:'image-count',valueId:String(value)}));
    if (name==='aspect') return ['1:1','3:2','2:3','16:9'].map(value=>row(value,'image',{checked:state.aspect===value,action:'aspect',valueId:value}));
    if (name==='seed') return ['Random','Locked: 42'].map(value=>row(value,'settings',{checked:state.seed===value,action:'seed',valueId:value}));
    if (name==='more') return [row('Full conversation','chat',{action:'full-conversation'}),row('Branch from here','fork',{action:'fork',disabled:!state.messages.length}),...(state.branches.length?[row('Saved conversations','history',{submenu:'branches'})]:[]),row('Rename','text',{action:'rename'}),{separator:true},row('Inspect context','eye',{action:'open-memory'}),row('Access','lock',{submenu:'access'}),...(mode==='free-agent'?[row('Pause task','pause',{action:'pause',disabled:!busy()}),row('Stop task','stop',{action:'stop',disabled:!busy()})]:[])];
    if (name==='branches') return state.branches.map((item,index)=>row(item.label,'chat',{action:'branch',valueId:String(index)}));
    if (name==='result') return [row('Copy result','copy',{action:'copy-result'}),row('Save image as','save',{action:'save-result'}),row('Place on board','plus',{action:'place-result'}),{separator:true},row('Start chat from this image','chat',{action:'source-chat'})];
    return [];
  }
  function openMenu(name,anchor,level=0) {
    closeMenus(level);hideTip();
    if (!anchor?.isConnected) return;
    anchor.setAttribute('aria-expanded','true');
    const element=document.createElement('div');element.className='menu-panel';element.setAttribute('role','menu');element.setAttribute('aria-label',name);element.dataset.menuName=name;element.dataset.level=String(level);
    let rows=rowsFor(name);
    if (level && innerWidth<620) rows=[{label:'Back',icon:'back',action:'menu-back'}, {separator:true},...rows];
    element.innerHTML=rows.map(item=>item.separator?'<div class="menu-divider" role="separator"></div>':`<div class="menu-row" ${item.submenu?`data-submenu="${item.submenu}"`:''} data-disabled="${Boolean(item.disabled)}"><button role="menuitem" class="menu-item ${item.danger?'danger':''}" ${item.disabled?'disabled':''} ${item.submenu&&!item.split?`data-submenu="${item.submenu}" aria-haspopup="menu"`:''} ${item.action?`data-menu-action="${item.action}" data-value="${esc(item.valueId || '')}"`:''} ${item.help?`data-tooltip="${esc(item.help)}"`:''}><span class="menu-icon">${icon(item.icon)}</span><span class="menu-label">${esc(item.label)}</span>${item.value?`<span class="menu-value">${esc(item.value)}</span>`:''}<span class="menu-trailing">${item.checked?icon('check'):item.submenu&&!item.split?icon('chevron'):''}</span></button>${item.split?`<button role="menuitem" class="menu-split-arrow" data-submenu="${item.submenu}" aria-haspopup="menu" aria-label="API key actions" data-tooltip="Add or change your API key without selecting or running a model.">${icon('chevron')}</button>`:''}</div>`).join('');
    $('#menu-layer').append(element);
    const box=element.getBoundingClientRect(),rect=anchor.getBoundingClientRect();let left=level?rect.right+5:rect.left,top=level?rect.top:rect.bottom+6;
    if (left+box.width>innerWidth-10) left=level?rect.left-box.width-5:innerWidth-box.width-10;
    if (level&&innerWidth<620) left=Math.min(Math.max(10,rect.left),innerWidth-box.width-10);
    top=Math.max(10,Math.min(top,innerHeight-box.height-10));left=Math.max(10,left);
    element.style.left=left+'px';element.style.top=top+'px';menus.push({name,element,anchor});
  }
  function handleMenuAction(action,value) {
    if (action==='provider') { state.provider=value;state.model=availableModels()[0].id;normalizeModel();closeMenus();render();toast(provider().needsKey?'API provider selected. Use its separate chevron to manage the key.':'Provider selected. No request was started.');return; }
    if (action==='model') { state.model=value;normalizeModel();closeMenus();render();toast('Model remembered for this mode, including after reopening.');return; }
    if (action==='effort') { changeEffort(value);return; }
    if (action==='access') { setGlobalAccess(value);return; }
    if (action==='board-access') { access={...access,boardAuto:value==='auto',updated:Date.now()};write(accessKey,access);channel?.postMessage(access);closeMenus();render();toast('Board-change preference saved globally, separately from provider access.');return; }
    if (action==='presentation') { state.presentation=value;closeMenus();render();toast('Same messages and draft; only the layout changed. No request sent.');return; }
    if (['speed','generation','image-count','aspect','seed'].includes(action)) {
      const property={generation:'generationMode','image-count':'imageCount'}[action]||action;state[property]=action==='image-count'?Number(value):value;closeMenus();render();toast('Setting saved. No generation was started.');return;
    }
    if (action==='branch') {const item=clone(state.branches[Number(value)]);if(item){captureBranch();state.activeBranch=item.id;state.conversationTitle=item.label;state.messages=clone(item.messages);state.prompt=item.prompt||'';state.conversationEpoch=item.epoch;if(item.sourceTitle)state.sourceTitle=item.sourceTitle;state.sourceImageIndex=item.sourceImageIndex;state.status='idle';closeMenus();render();toast('Saved conversation opened. Current branch, draft, and source binding retained.');}return;}
    if (action==='menu-back') { const last=menus.at(-1);closeMenus(menus.length-1);last?.anchor.focus();return; }
    closeMenus();handleAction(action);
  }
  function handleAction(action,button) {
    if (action==='send') return send();
    if (action==='stop') return stop();
    if (action==='pause') {if (!busy()) return;updateClocks();state.status='paused';event('Sample task paused. Draft and conversation remain available.');render();return toast('Paused. Resume continues this task; Stop ends it.');}
    if (action==='resume') {state.status='running';state.startedAt=Date.now()-state.elapsedSeconds*1000;event('Sample task resumed.');render();return;}
    if (action==='retry') return startRun();
    if (action==='toggle-interactive') {state.interactive=!state.interactive;state.continuity=state.interactive;render();return toast(state.interactive?'Interactive mode on. The fourth wire carries conversation context.':'Interactive mode off and continuity disconnected. New image requests are independent.');}
    if (action==='cut-continuity') return cutContinuity();
    if (action==='connect-continuity') return connectContinuity();
    if (action==='select-revision') {state.selectedRevision=Number(button.dataset.value);render();return feedback('Earlier output selected. Context and generation settings were not changed.');}
    if (action==='open-presentation') return openMenu('presentation',button);
    if (action==='open-result-menu') return openMenu('result',button);
    if (action==='expand-activity') {state.activityExpanded=!state.activityExpanded;render();return;}
    if (action==='attach' || action==='remove-attachment') {state.attachment=action==='attach';return inputChanged();}
    if(action==='sample-input-change'){state.attachment=true;return inputChanged();}
    if (action==='change-key') return keyDialog();
    if (action==='sample-key') {$('#demo-key').value='demo-sample-key';$('#key-validation').textContent='Sample ready. Choose Save key.';return;}
    if (action==='save-key') return saveKey();
    if (action==='remove-key') {setCredentialStatus('missing');render();return toast('Saved key status cleared across modes. Draft and outputs kept.');}
    if (action==='close-dialog') return closeDialog();
    if (action==='allow' || action==='deny') {state.status=action==='allow'?'running':'stopped';state.startedAt=Date.now()-state.elapsedSeconds*1000;event(action==='allow'?'Sample access allowed once; global preference unchanged.':'Sample access refused; task stopped.');render();return;}
    if (action==='apply') {if(access.boardAuto){state.appliedRevision=state.selectedRevision;render();return toast('Selected revision applied on the sample board under the explicit global board grant.');}return reviewPreview('apply');}
    if (action==='discard') return reviewPreview('discard');
    if (action==='confirm-apply') {state.appliedRevision=state.selectedRevision;closeDialog();render();return toast('Selected dashboard revision applied on the sample board.');}
    if (action==='confirm-discard') {state.selectedRevision=state.appliedRevision || 0;closeDialog();render();return toast('Preview discarded. Existing versions stay in history.');}
    if (action==='open-memory') return showMemory();
    if (action==='fork') return forkDialog();
    if (action==='confirm-fork') return fork();
    if (action==='full-conversation') {state.presentation='window';render();return toast('Full conversation opened without another request.');}
    if (action==='rename') {createDialog('Name this conversation','<label for="conversation-name">Name</label><input type="text" id="conversation-name" placeholder="Project exploration" maxlength="60">','<button class="control" data-action="close-dialog">Cancel</button><button class="primary-button" data-action="save-name">Save name</button>');return;}
    if (action==='save-name') {state.conversationTitle=$('#conversation-name').value || state.conversationTitle;closeDialog();render();return toast('Conversation name saved. Source labels are unchanged.');}
    if (action==='save-result') {const svg=$('.result-preview svg');if(svg)download('slate-sample-image.svg',svg.outerHTML,'image/svg+xml');return toast('Selected sample image saved as SVG.');}
    if (action==='copy-result') {const svg=$('.result-preview svg');if(svg&&navigator.clipboard)navigator.clipboard.writeText(svg.outerHTML).then(()=>toast('Selected sample image markup copied.'),()=>toast('Clipboard unavailable. Use Save image as instead.'));return;}
    if (action==='place-result') {state.placed++;const revision=state.revisions[state.selectedRevision];(state.placedResults ||= []).push({index:revision?.variant??state.selectedRevision,title:revision?.title||'Selected image'});render();return toast(`Copy placed on the sample board (${state.placed}). Original output retained.`);}
    if (action==='source-chat') return sourceChat();
    if (action==='locate-agent') {$('.scene-card').scrollIntoView({behavior:'smooth',block:'center'});return toast('Mira located on the sample board.');}
  }
  document.addEventListener('click',e=>{
    const modeButton=e.target.closest('[data-mode]');if(modeButton)return setMode(modeButton.dataset.mode);
    const menuAction=e.target.closest('[data-menu-action]');if(menuAction&&!menuAction.disabled)return handleMenuAction(menuAction.dataset.menuAction,menuAction.dataset.value);
    const submenu=e.target.closest('button[data-submenu]');if(submenu&&!submenu.disabled){const level=Number(submenu.closest('.menu-panel').dataset.level)+1;return openMenu(submenu.dataset.submenu,submenu.closest('.menu-row'),level);}
    const menuButton=e.target.closest('[data-menu]');if(menuButton){const was=menus[0]?.anchor===menuButton;closeMenus();if(!was)openMenu(menuButton.dataset.menu,menuButton);return;}
    const action=e.target.closest('[data-action]');if(action&&!action.disabled)return handleAction(action.dataset.action,action);
    if(e.target.classList.contains('dialog-backdrop'))closeDialog();
  });
  document.addEventListener('pointerdown',e=>{if(!e.target.closest('.menu-panel,[data-menu],#global-access'))closeMenus();});
  document.addEventListener('pointerover',e=>{
    const anyRow=e.target.closest('.menu-row');
    if(anyRow&&!anyRow.dataset.submenu&&!anyRow.contains(e.relatedTarget)){closeMenus(Number(anyRow.closest('.menu-panel').dataset.level)+1);return;}
    const row=e.target.closest('.menu-row[data-submenu]');if(!row || row.contains(e.relatedTarget) || row.dataset.disabled==='true')return;
    const level=Number(row.closest('.menu-panel').dataset.level)+1;
    clearTimeout(menuTimer);menuTimer=setTimeout(()=>openMenu(row.dataset.submenu,row,level),140);
  });
  document.addEventListener('keydown',e=>{
    if (e.key==='Escape') {if(editingDialog||$('#dialog-layer').children.length)closeDialog();else if(menus.length){const anchor=menus.at(-1).anchor;closeMenus(menus.length-1);anchor?.focus();}hideTip();return;}
    const panel=e.target.closest('.menu-panel');
    if (panel && ['ArrowDown','ArrowUp','ArrowRight','ArrowLeft','Home','End'].includes(e.key)) {
      e.preventDefault();const items=[...panel.querySelectorAll('button[role=menuitem]:not(:disabled)')];const at=items.indexOf(document.activeElement);
      if(e.key==='ArrowRight'){const row=document.activeElement.closest('[data-submenu]');if(row){openMenu(row.dataset.submenu,row.closest('.menu-row'),Number(panel.dataset.level)+1);menus.at(-1).element.querySelector('button:not(:disabled)')?.focus();}return;}
      if(e.key==='ArrowLeft'){const anchor=menus.at(-1).anchor;closeMenus(menus.length-1);anchor.querySelector('button')?.focus();return;}
      const next=e.key==='Home'?0:e.key==='End'?items.length-1:(at+(e.key==='ArrowUp'?-1:1)+items.length)%items.length;items[next]?.focus();
    }
    if ((e.ctrlKey||e.metaKey)&&e.key==='Enter' && e.target.id==='agent-prompt') {e.preventDefault();send();}
    if ($('#dialog-layer .dialog') && e.key==='Tab') {const items=[...$('#dialog-layer').querySelectorAll('button,input,select,textarea')].filter(item=>!item.disabled);const first=items[0],last=items.at(-1);if(e.shiftKey&&document.activeElement===first){e.preventDefault();last.focus();}else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first.focus();}}
  });
  document.addEventListener('input',e=>{if(e.target.id==='agent-prompt'){state.prompt=e.target.value;persist();}if(e.target.id==='design-comment'){saved.notes[mode]=e.target.value;write(key,saved);$('#notes-status').textContent='Saved locally';}});
  $('#scenario-select').addEventListener('change',e=>loadScenario(e.target.value));
  $('#restart-scenario').addEventListener('click',()=>{if(scenarioId)loadScenario(scenarioId);else{const kept={provider:state.provider,model:state.model,effort:state.effort,speed:state.speed};state={...fresh(mode),...kept};render();}});
  $('#show-decisions').addEventListener('click',showDecisions);
  $('#export-review').addEventListener('click',exportReview);
  accessControl.addEventListener('click',()=>openMenu('access',accessControl));
  $('#mode-select').addEventListener('change',e=>setMode(e.target.value));
  function toggleStudioPanel(id){for(const name of ['setup','feedback']){const panel=$(`#studio-${name}`),button=$(`#studio-${name}-toggle`);const open=name===id&&panel.hidden;panel.hidden=!open;button.setAttribute('aria-expanded',String(open));}closeMenus();}
  $('#studio-setup-toggle').addEventListener('click',()=>toggleStudioPanel('setup'));
  $('#studio-feedback-toggle').addEventListener('click',()=>toggleStudioPanel('feedback'));
  $('#open-second-instance').addEventListener('click',()=>window.open(location.href,'_blank','noopener'));
  $('#live-effort-support').addEventListener('change',e=>{state.liveEffort=e.target.checked;persist();feedback(state.liveEffort?'Effort updates can apply during this sample task.':'Effort updates are queued for the next turn in this sample capability mode.');});
  function applyTheme(){document.documentElement.dataset.theme=saved.theme==='dark'?'dark':'light';$('#theme-toggle').innerHTML=icon(saved.theme==='dark'?'sun':'moon');}
  $('#theme-toggle').addEventListener('click',()=>{saved.theme=saved.theme==='dark'?'light':'dark';applyTheme();write(key,saved);});
  function hideTip(){clearTimeout(tipTimer);$('#tooltip').hidden=true;}
  function showTip(target){const text=target?.dataset.tooltip;if(!text)return;const tip=$('#tooltip');tip.textContent=text;tip.hidden=false;const rect=target.getBoundingClientRect(),box=tip.getBoundingClientRect();tip.style.left=Math.max(8,Math.min(rect.left,innerWidth-box.width-8))+'px';tip.style.top=Math.max(8,Math.min(rect.bottom+7,innerHeight-box.height-8))+'px';}
  document.addEventListener('pointerover',e=>{const target=e.target.closest('[data-tooltip]');if(!target||target.contains(e.relatedTarget))return;hideTip();tipTimer=setTimeout(()=>showTip(target),450);});
  document.addEventListener('pointerout',e=>{if(e.target.closest('[data-tooltip]')&&!e.target.closest('[data-tooltip]').contains(e.relatedTarget))hideTip();});
  document.addEventListener('focusin',e=>showTip(e.target.closest('[data-tooltip]')));
  document.addEventListener('focusout',hideTip);
  window.addEventListener('resize',()=>{closeMenus();hideTip();});window.addEventListener('scroll',hideTip,true);
  setInterval(()=>{
    updateClocks();
    if(state.status!=='running'||!state.autoProgress||state.quiet)return;
    const stages=['Sample: reviewing the connected references.','Sample: preparing the next result.','Sample: checking the output against your latest direction.'];
    if(state.nextStep<3 && state.elapsedSeconds>=(state.nextStep+1)*4){event(stages[state.nextStep++],'tool');render();}
    if(state.elapsedSeconds>=16){state.status='completed';state.autoProgress=false;state.messages.push({role:'assistant',text:mode==='image'?'Here is the next image study. You can keep refining it in this conversation.':mode==='dashboard'?'The next dashboard revision is ready. Review it before applying it to the board.':mode==='text'?'Project brief\n\nCreate a clear, welcoming workspace that brings research, decisions, and next actions together. Keep the main content readable and the supporting controls predictable.':'I have prepared the next step based on your instructions. Which part would you like to explore next?'});if(['image','dashboard'].includes(mode)){const settings=state.runSettings||{count:1,aspect:state.aspect,seed:state.seed};const count=mode==='image'?Math.min(4,Math.max(1,settings.count||1)):1;let seedHash=0;for(const char of state.lastInstruction||'')seedHash=(seedHash*31+char.charCodeAt(0))>>>0;for(let i=0;i<count;i++)state.revisions.push({id:Date.now()+i,title:mode==='image'?`Image ${state.revisions.length+1}`:`Revision ${state.revisions.length+1}`,aspect:settings.aspect,seed:settings.seed,variant:settings.seed==='Random'?state.revisions.length%4:(seedHash+42+i)%4});state.selectedRevision=state.revisions.length-count;}event('Sample task complete.','status');const queued=state.queuedMessages?.shift();const refreshed=state.liveRefreshPending;state.liveRefreshPending=false;state.pendingSteering='';render();if(queued){event('The queued follow-up is now being delivered as the next sample turn.');startRun(queued);}else if(refreshed)startRun(state.lastInstruction||modeDefinition().defaultPrompt);}
  },1000);
  window.addEventListener('beforeunload',persist);
  applyTheme();render();
  // Read-only diagnostics keep the prototype inspectable during browser checks.
  window.AgentStudio={snapshot:()=>clone({mode,state,access,scenarioId}),loadScenario,exportReview};
}());
