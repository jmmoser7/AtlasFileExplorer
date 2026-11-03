/* Slate chat train only. Native geometry and controls; local sample responses. */
(async () => {
  'use strict';
  await document.fonts.ready;
  const M = window.ChatTrainModel, R = window.ChatTrainReference, Picker=window.ChatAgentPicker;
  const $ = id => document.getElementById(id);
  const canvas = $('canvas'), world = $('world'), cardsEl = $('cards'), rails = $('rails');
  const storageKey = 'slate-chat-train-baseline-v1';
  const fixture = new URLSearchParams(location.search).has('reference');
  const isolated = fixture || new URLSearchParams(location.search).has('sample');
  const seed = () => M.create(fixture ? {transcript: [
    {role:'user', text:"What delayed the first reply, but it's ongoing background work."},
    {role:'assistant', text:"Slate's own logs are the place to check for the 35-second launch delay. If you want, I can look through the AtlasFileExplorer repo for the code that launches the agent."}
  ]} : {transcript:window.ChatTrainConversation});
  let state = seed(), camera = {x: 12, y: 0, zoom: 1}, cards = [], gesture = null, space = false;
  let menuCard = null, timer = null, hintTimer = null, statusTimer = null;
  let fullAccess = {}, schedules = {};
  let selectedCardId=null;
  const scrolls = new Map(), readerPaused = new Set();
  try {
    const saved = isolated ? null : JSON.parse(localStorage.getItem(storageKey) || 'null');
    if (saved?.state?.version === 1) {
      state = saved.state;
      for(const [id,settings] of Object.entries(state.meta||{}))if(id.startsWith("pairs:")&&settings.detail==="full")delete settings.detail;
      if (state.running) M.stop(state);
      camera = saved.camera || camera;
      fullAccess = saved.fullAccess || {};
      schedules = saved.schedules || {};
    }
  } catch { /* An invalid local snapshot cannot prevent opening the reference. */ }
  const textContext = document.createElement('canvas').getContext('2d');
  const widthOf = (text, size=14) => {
    textContext.font = `300 ${size}px ChatTrainUbuntu`;
    return Math.max(0, ...String(text).split('\n').map(line => textContext.measureText(line).width));
  };
  function lines(text, width, size=14) {
    textContext.font = `300 ${size}px ChatTrainUbuntu`;
    let count = 0;
    for (const paragraph of String(text).split('\n')) {
      if (!paragraph) { count++; continue; }
      let row = '';
      for (const word of paragraph.match(/\S+\s*|\s+/g) || []) {
        if (row && textContext.measureText(row + word.trimEnd()).width > width) { count++; row = ''; }
        if (textContext.measureText(word.trimEnd()).width > width) {
          for (const char of word) {
            if (row && textContext.measureText(row + char).width > width) { count++; row = ''; }
            row += char;
          }
        } else row += word;
      }
      count++;
    }
    return Math.max(1, count);
  }
  function measure(card) {
    const windowMode=state.presentation==='window';
    const text = card.turns.map(t => t.text).join('\n\n');
    const minTitle = Math.max(176, Math.min(windowMode?440:320,widthOf(`${card.title} · ${card.model} ${Picker.label(card.effort)}`, 13) + 108));
    const natural = Math.min(Math.max(320, minTitle), Math.max(minTitle, widthOf(card.isDraft ? card.draftText : text, card.isDraft ? 14 : 13) + 24));
    const old = state._measured?.[card.id];
    const width = card.customSize?.width || ((card.fold !== 'open' && old?.width) ? old.width : windowMode?Math.max(440,minTitle):natural);
    const wrap = width - 32;
    const composerHeight = windowMode?Math.min(card.fullExpanded?Infinity:128,Math.max(20,lines(card.draftText||'',wrap-30)*20)+2):Math.max(16.1, lines(card.draftText || '', wrap) * 16.1) + 2;
    let transcriptHeight = 0;
    for (let i=0; i<card.turns.length; i++) {
      if (i) transcriptHeight += windowMode?20:card.detail === 'pair' && card.turns[i-1].role === 'user' && card.turns[i].role === 'assistant' ? 34 : 12;
      transcriptHeight += lines(card.turns[i].text || '', wrap-(windowMode&&card.turns[i].role==='user'?28:0)) * (windowMode?20:16.1);
    }
    const composer = card.isTail || card.isDraft;
    const summary = card.detail === 'summary' && !card.isTail;
    const openHeight = card.isDraft ? Math.max(62, 32 + composerHeight + 14) : summary ? Math.max(62,32+lines(text,wrap,13)*14.95+18) : Math.max(62, 32 + transcriptHeight + (composer ? 2 + 18 + composerHeight : 0) + 14);
    const capsule = 32 + Math.min(3, lines(text, wrap)) * 16.1 + 18;
    let height = openHeight;
    const fold = card.streamOpen && card.fold === 'collapsed' ? 'partial' : card.fold;
    if (!card.isDraft && fold !== 'open') height = (fold === 'partial' ? capsule*2 : capsule) + (composer ? composerHeight - 2 + 14 : 0);
    if (fold === 'partial' && composer) height = Math.min(height, openHeight);
    if (card.customSize && !card.customSize.autoHeight && fold === 'open') height = card.customSize.height;
    if (windowMode && fold === 'open' && (!card.customSize||card.customSize.autoHeight)) height = card.fullExpanded?openHeight+18:Math.min(560,Math.max(130,openHeight+18));
    return {width, height, composerHeight, transcriptHeight, capsule};
  }
  const element = (tag, className, text) => {
    const el = document.createElement(tag);
    if (className) el.className = className;
    if (text !== undefined) el.textContent = text;
    return el;
  };
  const esc = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;'}[c]));
  function button(className, label, hint, click) {
    const el = element('button', className);
    el.type = 'button'; el.setAttribute('aria-label', label);
    if (hint) el.dataset.hint = hint;
    if (click) el.addEventListener('click', click);
    return el;
  }
  function save() {
    if (isolated) return;
    try { localStorage.setItem(storageKey, JSON.stringify({state,camera,fullAccess,schedules})); } catch {}
  }
  function status(text) {
    $('canvas-status').textContent = text;
    $('canvas-status').classList.add('is-visible');
    clearTimeout(statusTimer); statusTimer = setTimeout(() => $('canvas-status').classList.remove('is-visible'), 4200);
  }
  function setCamera() {
    world.style.transform = `translate(${camera.x}px,${camera.y}px) scale(${camera.zoom})`;
    $('zoom-reset').textContent = `${Math.round(camera.zoom*100)}%`;
  }
  function path(from, to) {
    const reach = Math.max(36, Math.abs(to.x-from.x)*.5);
    return `M ${from.x} ${from.y} C ${from.x+reach} ${from.y},${to.x-reach} ${to.y},${to.x} ${to.y}`;
  }
  function paintRails() {
    const byId = new Map(cards.map(c=>[c.id,c]));
    rails.innerHTML = cards.filter(c=>byId.has(c.parentId)).map(card=> {
      const parent=byId.get(card.parentId), siblings=cards.filter(c=>c.parentId===card.parentId);
      const strand=Math.max(0,siblings.indexOf(card))*5.5;
      return `<path d="${path({x:parent.x+parent.width-6,y:parent.y+10+strand},{x:card.x+6,y:card.y+10})}"/>`;
    }).join('');
  }
  function draw(focusId, selection) {
    if(gesture?.kind==='resize'||gesture?.kind==='move')return;
    const active = document.activeElement;
    if (!focusId && active?.classList.contains('card-composer')) {
      focusId=active.closest('.chat-card').dataset.id;
      selection=[active.selectionStart,active.selectionEnd];
    }
    for (const el of cardsEl.querySelectorAll('.transcript')) {
      const id=el.closest('.chat-card').dataset.id;
      scrolls.set(id,el.scrollTop);
    }
    cards=M.project(state,measure);
    cardsEl.replaceChildren();
    for (const card of cards) {
      const windowMode=state.presentation==='window';
      const metrics=measure(card), node=element('article',`chat-card${windowMode?' is-window':''}`);
      node.dataset.id=card.id; node.dataset.fold=card.fold;
      node.classList.toggle('is-selected',selectedCardId===card.id);
      node.setAttribute('aria-label', `${card.title} ${card.isDraft?'draft':card.turns[0]?.text||'chat'}`);
      Object.assign(node.style,{left:`${card.x}px`,top:`${card.y}px`,width:`${card.width}px`,height:`${card.height}px`});
      const header=element('header','card-header');
      const dot=element('span',`status-dot${card.running?' is-running':''}`);
      header.append(dot);
      const title=element('div','header-text');
      const name=button('conversation-title','Rename chat','Click to rename this conversation.',()=>rename(card));
      name.textContent=card.title; title.append(name,element('span','title-divider',' · '));
      if (card.running) {
        const responding=element('span','responding-text');
        responding.append(element('span','responding-word','Responding'),element('span','token-count',`~${state.usage.estimated}`));
        responding.setAttribute('role','status'); title.append(responding);
      } else if (card.isTail) {
        const model=button(`model-name${fullAccess[card.sessionId]?' is-full':''}`,'Agent settings','Choose provider, model, thinking effort, and response speed.',e=>openModels(card,e.currentTarget));
        model.setAttribute('aria-haspopup','menu');
        const speed=element('span','agent-speed-mark');speed.dataset.hint=`${Picker.label(card.speed)} response speed`;
        const boltCount=window.ChatAgentCatalog.speedInfo[card.speed]?.bolts||0;
        speed.innerHTML=Array.from({length:boltCount},()=>'<svg viewBox="0 0 18 18" aria-hidden="true"><path d="M10 1L3 10h5l-1 7 7-10H9z"/></svg>').join('');speed.hidden=!boltCount;
        const caret=element('span','picker-caret');caret.innerHTML='<svg viewBox="0 0 8 7" aria-hidden="true"><path d="M1.6 2.15L4 4.85L6.4 2.15"/></svg>';
        const modelLabel=element('span','model-label',card.model);modelLabel.dataset.hint=card.model;
        model.append(speed,modelLabel,element('span','agent-effort-label',Picker.label(card.effort)),caret);title.append(model);
      } else {title.append(element('span','historical-model',card.model),element('span','agent-effort-label',Picker.label(card.effort)));}
      header.append(title);
      if (!card.isDraft) {
        const fold=element('div','fold-controls');
        const shownFold=card.streamOpen&&card.fold==='collapsed'?'partial':card.fold;
        const offers=shownFold==='collapsed'?[['expand',1],['expand',2]]:shownFold==='partial'?[['collapse',1],['expand',1]]:[['collapse',2],['collapse',1]];
        for (const [direction,step] of offers) {
          const label=`${direction==='collapse'?'Collapse':'Expand'}${step===2?' fully':' one step'}`;
          const b=button('fold-hit',label,null,()=>{
            const index=['collapsed','partial','open'].indexOf(shownFold);
            M.setFold(state,card.id,['collapsed','partial','open'][Math.max(0,Math.min(2,index+(direction==='expand'?step:-step)))]);
            changed();
          });
          const up=direction==='collapse', d=up?'M1.6 4.35 L4 1.65 L6.4 4.35':'M1.6 1.65 L4 4.35 L6.4 1.65';
          b.innerHTML=`<svg viewBox="0 0 8 7" aria-hidden="true"><path d="${d}"/>${step===2?`<path transform="translate(0 ${up?2:-2})" d="${d}"/>`:''}</svg>`;
          fold.append(b);
        }
        header.append(fold);
      }
      const menu=button('header-menu','Chat menu','Conversation layout, branching, access, and card actions.',e=>openChatMenu(card,e.currentTarget));
      menu.innerHTML='<i></i><i></i><i></i>'; header.append(menu);
      if(!card.isDraft){
        const output=button(`output-handle${card.running?' is-stop':''}`,card.running?'Stop response':'Continue chat',card.running?'Stop the sample reply. Keep the text received so far.':'Click for an unsent continuation. Drag to position it; drag from an earlier card to branch.');
        output.addEventListener('pointerdown',e=>beginOutput(e,card)); header.append(output);
      }
      node.append(header);
      if (!card.isDraft) {
        const transcript=element('div','transcript'); transcript.tabIndex=0;
        transcript.setAttribute('aria-label','Conversation text');
        const hasComposer=card.isTail;
        const bottom=hasComposer?14+metrics.composerHeight+18:14;
        const textTop=windowMode?40:32;
        transcript.style.top=`${textTop}px`;
        transcript.style.height=`${Math.max(0,card.height-textTop-bottom-(windowMode?8:0))}px`;
        const collapsed=card.fold==='collapsed'&&!card.streamOpen;
        const summary=card.detail==='summary'&&!card.isTail&&!collapsed&&card.fold==='open';
        const turns=card.turns;
        if(summary){transcript.style.top='28px';transcript.style.height=`${Math.max(0,card.height-28-10)}px`;}
        if(collapsed){transcript.style.top='32px';transcript.style.height='48.3px';transcript.style.overflow='hidden';transcript.style.lineHeight='16.1px';}
        if(collapsed||summary)transcript.append(element('p','turn',turns.map(t=>t.text).join('\n\n')));
        else for (let i=0;i<turns.length;i++) {
          if (i&&card.detail==='pair'&&turns[i-1].role==='user'&&turns[i].role==='assistant') transcript.append(element('div','pair-rule'));
          const content=turns[i].text+(card.running&&i===turns.length-1?' |':'');
          const turn=element('p','turn',windowMode?undefined:content); turn.dataset.role=turns[i].role;
          if(windowMode){turn.append(element('span','turn-text',content));}
          transcript.append(turn);
        }
        const thumb=element('div','scroll-thumb');node.append(thumb);
        const updateThumb=(show=false)=>{
          const view=transcript.clientHeight,total=transcript.scrollHeight;
          thumb.hidden=total<=view+1;
          const size=Math.max(24,view*view/Math.max(view,total));
          thumb.style.height=`${size}px`;thumb.style.top=`${parseFloat(transcript.style.top)+(view-size)*transcript.scrollTop/Math.max(1,total-view)}px`;
          if(show){thumb.classList.add('is-scrolling');clearTimeout(thumb._fade);thumb._fade=setTimeout(()=>thumb.classList.remove('is-scrolling'),650);}
        };
        node._updateThumb=updateThumb;
        transcript.addEventListener('keydown',()=>{node._userScrollAt=performance.now();});
        transcript.addEventListener('scroll',()=>{
          updateThumb(performance.now()-(node._userScrollAt||-10000)<1000);
          scrolls.set(card.id,transcript.scrollTop);
          if (transcript.scrollTop+transcript.clientHeight<transcript.scrollHeight-3) readerPaused.add(card.id); else readerPaused.delete(card.id);
        });
        node.append(transcript);
      }
      if (card.isTail||card.isDraft) {
        const composer=element('textarea','card-composer');
        composer.setAttribute('aria-label','Message'); composer.placeholder=windowMode?'Message…':'Message';
        composer.rows=1; composer.value=card.draftText||''; composer.spellcheck=false;
        composer.style.height=`${metrics.composerHeight}px`;
        composer.addEventListener('input',()=>{M.setDraft(state,card.id,composer.value);draw(card.id,[composer.selectionStart,composer.selectionEnd]);save();});
        composer.addEventListener('keydown',e=>{
          if (e.key==='Enter'&&!e.shiftKey&&!e.isComposing) {e.preventDefault();send(card.id,composer.value);}
        });
        composer.addEventListener('focus',()=>{state.activeCardId=card.id;});
        node.append(composer);
        if(windowMode){
          const sendButton=button('window-send','Send message','Send this message. Enter sends; Shift+Enter starts a new line.',()=>send(card.id,composer.value));
          sendButton.innerHTML='<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3 10h13M10 4l6 6-6 6"/></svg>';
          const rule=element('div','window-composer-rule');rule.style.bottom=`${metrics.composerHeight+26}px`;
          sendButton.disabled=Boolean(state.running);node.append(rule,sendButton);
        }
      }
      if (!card.isDraft) for (const direction of ['n','s','w','e','nw','ne','sw','se']) {
        const edge=element('div',`resize-edge resize-${direction}`); edge.dataset.edge=direction;
        edge.addEventListener('pointerdown',e=>beginResize(e,card,direction)); node.append(edge);
      }
      node.addEventListener('pointerdown',e=>{
        state.activeCardId=card.id;
        if (e.button!==0||space||e.target.closest('button,input,textarea,.resize-edge')) return;
        if(selectedCardId===card.id&&e.target.closest('.transcript'))return;
        gesture={kind:'move',card:{...card},start:point(e),screen:{x:e.clientX,y:e.clientY},moved:false};
        e.preventDefault();
      });
      cardsEl.append(node);
      const transcript=node.querySelector('.transcript');
      if (transcript) transcript.scrollTop=card.running&&!readerPaused.has(card.id)?transcript.scrollHeight:(scrolls.get(card.id)||0);
      node._updateThumb?.();
    }
    paintRails(); setCamera();
    if (focusId) {
      const target=Array.from(cardsEl.children).find(el=>el.dataset.id===focusId||cards.find(c=>c.id===el.dataset.id)?.draftId===focusId)?.querySelector('.card-composer');
      if (target) {target.focus({preventScroll:true});if(selection)target.setSelectionRange(...selection);}
    }
  }
  function changed(focusId) {closeMenu();draw(focusId);save();}
  function rename(card) {
    closeMenu();
    const title=Array.from(cardsEl.children).find(el=>el.dataset.id===card.id)?.querySelector('.header-text');
    if (!title) return;
    const input=element('input','rename-input'); input.value=card.title; input.setAttribute('aria-label','Chat title');
    title.replaceChildren(input); input.focus(); input.select(); let finished=false;
    const finish=commit=>{if(finished)return;finished=true;if(commit)M.rename(state,card.id,input.value);changed();};
    input.addEventListener('blur',()=>finish(true));
    input.addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();finish(true);}if(e.key==='Escape'){e.stopPropagation();finish(false);}});
  }
  const icons={
    window:'<rect x="2" y="3" width="12" height="10" rx="1"/><path d="M2 6h12"/>',
    train:'<rect x="1" y="4" width="4" height="8" rx=".5"/><rect x="10" y="4" width="4" height="8" rx=".5"/><path d="M5 6h5"/>',
    pairs:'<rect x="2" y="2" width="12" height="12" rx="1"/><path d="M4 8h8"/>',
    text:'<path d="M4 1.5h6l3 3V14H4zM10 1.5v3h3M6 7h5M6 9h5M6 11h4"/>',
    fork:'<path d="M2 5h5v6h7M7 5h7M11 2l3 3-3 3M11 8l3 3-3 3"/>',
    access:'<rect x="3" y="7" width="10" height="7" rx="1"/><path d="M5 7V4a3 3 0 0 1 6 0v3"/>',
    clock:'<circle cx="8" cy="8" r="6"/><path d="M8 4v4l3 2"/>',
    fit:'<path d="M1 5V1h4M11 1h4v4M15 11v4h-4M5 15H1v-4M4 5h8M4 8h6M4 11h8"/>',
    trash:'<path d="M2 4h12M6 4V2h4v2M4 4l1 10h6l1-10M7 7v4M9 7v4"/>',
    check:'<path d="M3 8l3 3 7-7"/>'
  };
  const glyph=name=>`<svg viewBox="0 0 16 16" aria-hidden="true">${icons[name]||''}</svg>`;
  function closeMenu(){ Picker.close();$('menus').replaceChildren();menuCard=null; }
  function makeMenu(anchor, className='') {
    closeMenu();const menu=element('div',`native-menu ${className}`);menu.setAttribute('role','menu');
    $('menus').append(menu);
    const box=anchor.getBoundingClientRect();menu.style.left=`${Math.max(5,Math.min(innerWidth-240,box.left))}px`;menu.style.top=`${box.bottom+2}px`;
    return menu;
  }
  function placeMenu(menu) {
    const box=menu.getBoundingClientRect();if(box.bottom>innerHeight-6)menu.style.top=`${Math.max(6,innerHeight-box.height-6)}px`;
    if(box.right>innerWidth-6)menu.style.left=`${Math.max(6,innerWidth-box.width-6)}px`;
    menu.querySelector('button')?.focus({preventScroll:true});
  }
  function menuRow(menu,label,icon,action,hint,danger=false,checked=false,disabled=false) {
    const row=button(`menu-row${danger?' danger':''}`,label,hint,()=>{closeMenu();action();});
    row.setAttribute('role','menuitem');row.innerHTML=`<span class="menu-glyph">${glyph(checked?'check':icon)}</span><span>${esc(label)}</span>`;
    row.disabled=disabled;
    menu.append(row);return row;
  }
  function openModels(card,anchor) {
    if(state.running)return;closeMenu();menuCard=card.id;
    Picker.open({anchor,card,onChange:config=>{M.setAgent(state,card.id,config);draw();save();},onClose:closeMenu,onStatus:status});
  }
  function changePresentation(value){
    if(!M.changePresentation(state,value))return;changed();
    const card=cards.find(c=>c.id===state.activeCardId)||cards.find(c=>c.isTail);
    if(value==='window'&&card){
      camera.zoom=Math.min(camera.zoom,1.3,(canvas.clientHeight-70)/card.height);
      camera.x=(canvas.clientWidth-card.width*camera.zoom)/2-card.x*camera.zoom;
      camera.y=Math.max(30,(canvas.clientHeight-card.height*camera.zoom)/2)-card.y*camera.zoom;
      setCamera();
      const transcript=Array.from(cardsEl.children).find(el=>el.dataset.id===card.id)?.querySelector('.transcript');
      if(transcript)transcript.scrollTop=transcript.scrollHeight;
    }else revealActive();
    if(card)focusCard(card.id);save();
  }
  function openChatMenu(card,anchor) {
    const menu=makeMenu(anchor);menuCard=card.id;
    if(!state.running) for (const [value,label,icon] of [['window','Single chat window','window'],['messages','Chat train','train'],['pairs','Message pairs','pairs']]) {
      menuRow(menu,label,icon,()=>changePresentation(value),value==='window'?'Keep this conversation path in one scrollable window with its composer pinned below.':value==='messages'?'Put each message and reply in its own connected card.':'Keep each message and its reply together in a connected card.',false,value===state.presentation);
    }
    {
      if(!state.running)menu.append(element('div','menu-rule'));
      menuRow(menu,'Full conversation','text',()=>{M.expandFull(state,card.id);changed();},'Fit all text in this card, preserving its width and message-pair divider.');
    }
    {
      menu.append(element('div','menu-rule'));
      if(!state.running)menuRow(menu,'Fork chat here','fork',()=>{const id=M.continueFrom(state,card.id,{fork:true});changed(id);revealActive();},card.isDraft?'Send this draft before branching from its reply.':'Start a separate branch with this conversation as its starting context.',false,false,card.isDraft);
      menuRow(menu,'Full access','access',()=>{fullAccess[card.sessionId]=!fullAccess[card.sessionId];changed();},'Run commands and edit files without asking, from the next message. Sample setting only.',false,fullAccess[card.sessionId]);
      menuRow(menu,schedules[card.sessionId]?'Edit schedule…':'Schedule…','clock',()=>schedule(card),'Repeat this conversation on a schedule. Local sample only.');
      if(schedules[card.sessionId])menuRow(menu,'Stop schedule','clock',()=>{delete schedules[card.sessionId];changed();},'Remove the sample schedule.');
    }
    menu.append(element('div','menu-rule'));
    menuRow(menu,'Fit to text','fit',()=>{M.fit(state,card.id);changed();},'Reset authored size and fit this card to its conversation text.');
    if(card.isTail)menuRow(menu,'Delete card','trash',()=>{if(card.running)clearInterval(timer);M.deleteLeaf(state,card.id);changed();},'Delete the final card in this branch.',true);
    placeMenu(menu);
  }
  function dialog(title) {
    $('dialogs').replaceChildren();
    const cover=element('div','modal-cover'), panel=element('section','reference-dialog');
    panel.setAttribute('role','dialog');panel.setAttribute('aria-modal','true');panel.setAttribute('aria-label',title);
    panel.append(element('h2','',title));cover.append(panel);$('dialogs').append(cover);
    cover.addEventListener('click',e=>{if(e.target===cover)$('dialogs').replaceChildren();});
    return panel;
  }
  function closeButton(panel) {const close=element('button','','Close');close.addEventListener('click',()=>$('dialogs').replaceChildren());panel.append(close);close.focus();}
  function schedule(card) {
    const panel=dialog('Sample schedule');panel.append(element('p','','This local canvas can retain a schedule for review. It does not run background tasks.'));
    const select=element('select');select.setAttribute('aria-label','Schedule frequency');
    for(const name of ['Every day','Every week','Every month']){const option=element('option','',name);select.append(option);}
    select.value=schedules[card.sessionId]||'Every day';panel.append(select);
    const saveButton=element('button','','Save');saveButton.addEventListener('click',()=>{schedules[card.sessionId]=select.value;save();$('dialogs').replaceChildren();});panel.append(saveButton);closeButton(panel);
  }
  function sampleReply(prompt) {
    if(/tree/i.test(prompt))return 'Two plane trees would give the courtyard shade. Keep the trunks clear of the main path and leave room for their canopies to grow.';
    if(/wall|where|place/i.test(prompt))return 'Place them along the north wall, with a clear route across the courtyard. We can adjust the spacing in the next message.';
    return 'We can work through that here. Start with the main requirement, then refine the details one at a time. This is a sample reply so we can review the chat train’s behavior.';
  }
  function send(id,text) {
    if(state.running){status('Wait for this response, or use the Stop square.');return;}
    const result=M.send(state,id,text);if(!result)return;
    readerPaused.delete(result.cardId);changed(result.cardId);revealActive();
    const reply=sampleReply(text);let offset=0;
    clearInterval(timer);
    timer=setInterval(()=>{
      if(!state.running||state.running.assistantId!==result.assistantId){clearInterval(timer);return;}
      const next=reply.slice(offset,offset+4);offset+=next.length;
      M.append(state,result.assistantId,next,offset>=reply.length);draw();
      if(offset>=reply.length){clearInterval(timer);save();}
    },70);
  }
  const point=e=>{const r=canvas.getBoundingClientRect();return{x:(e.clientX-r.left-camera.x)/camera.zoom,y:(e.clientY-r.top-camera.y)/camera.zoom};};
  function beginOutput(e,card) {
    if(e.button!==0)return;e.preventDefault();e.stopPropagation();closeMenu();
    if(card.running){clearInterval(timer);M.stop(state);changed(card.id);return;}
    if(card.isDraft){focusCard(card.id);return;}
    if(state.running){status('Wait for this response, or use the Stop square.');return;}
    gesture={kind:'output',card:{...card},start:point(e),screen:{x:e.clientX,y:e.clientY},moved:false};
  }
  function beginResize(e,card,direction) {
    if(e.button!==0||space)return;e.preventDefault();e.stopPropagation();closeMenu();
    if(card.fold!=='open'){M.expandFull(state,card.id);draw();card=cards.find(c=>c.id===card.id)||card;}
    gesture={kind:'resize',card:{...card},direction,start:point(e),screen:{x:e.clientX,y:e.clientY},moved:false};
  }
  function focusCard(id) {
    const node=Array.from(cardsEl.children).find(el=>el.dataset.id===id||cards.find(c=>c.id===el.dataset.id)?.draftId===id);
    node?.querySelector('.card-composer')?.focus({preventScroll:true});
  }
  function revealActive() {
    const card=cards.find(c=>c.id===state.activeCardId);if(!card)return;
    const right=(card.x+card.width)*camera.zoom+camera.x,bottom=(card.y+card.height)*camera.zoom+camera.y;
    if(right>canvas.clientWidth-30)camera.x-=right-canvas.clientWidth+30;
    if(card.x*camera.zoom+camera.x<30)camera.x=30-card.x*camera.zoom;
    if(bottom>canvas.clientHeight-30)camera.y-=bottom-canvas.clientHeight+30;
    if(card.y*camera.zoom+camera.y<30)camera.y=30-card.y*camera.zoom;
    setCamera();save();
  }
  canvas.addEventListener('pointerdown',e=>{
    if(e.button===1||e.button===2||(space&&e.button===0)) {
      if(e.target.closest('button,input,textarea')&&e.button===0)return;
      e.preventDefault();gesture={kind:'pan',screen:{x:e.clientX,y:e.clientY},origin:{x:camera.x,y:camera.y},moved:false};canvas.classList.add('is-panning');
    } else if(e.target===canvas||e.target===world||e.target===cardsEl){closeMenu();document.activeElement?.blur();state.activeCardId=null;selectedCardId=null;cardsEl.querySelectorAll('.chat-card').forEach(n=>n.classList.remove('is-selected'));}
  });
  canvas.addEventListener('contextmenu',e=>e.preventDefault());
  window.addEventListener('pointermove',e=>{
    if(!gesture)return;
    const g=gesture,p=point(e),dx=p.x-g.start?.x,dy=p.y-g.start?.y;
    if(Math.hypot(e.clientX-g.screen.x,e.clientY-g.screen.y)>4)g.moved=true;
    if(g.kind==='pan'){camera.x=g.origin.x+e.clientX-g.screen.x;camera.y=g.origin.y+e.clientY-g.screen.y;setCamera();return;}
    if(!g.moved)return;
    const node=Array.from(cardsEl.children).find(el=>el.dataset.id===g.card.id);
    if(g.kind==='move') {
      g.next={x:g.card.x+dx,y:g.card.y+dy};Object.assign(node.style,{left:`${g.next.x}px`,top:`${g.next.y}px`});
      const c=cards.find(c=>c.id===g.card.id);c.x=g.next.x;c.y=g.next.y;paintRails();
    } else if(g.kind==='resize') {
      const left=g.direction.includes('w'),top=g.direction.includes('n');
      const width=Math.max(state.presentation==='window'?320:176,g.card.width+(left?-dx:g.direction.includes('e')?dx:0));
      const autoHeight=g.direction==='w'||g.direction==='e';
      const preview={...g.card,fold:'open',fullExpanded:autoHeight,customSize:{width,autoHeight:true}};
      const metrics=measure(preview);
      const height=autoHeight?metrics.height:Math.max(state.presentation==='window'?200:62,g.card.height+(top?-dy:g.direction.includes('s')?dy:0));
      g.next={width,height,autoHeight,x:g.card.x+(left?g.card.width-width:0),y:g.card.y+(top?g.card.height-height:0)};
      Object.assign(node.style,{left:`${g.next.x}px`,top:`${g.next.y}px`,width:`${width}px`,height:`${height}px`});
      const text=node.querySelector('.transcript');if(text){const bottom=g.card.isTail?14+metrics.composerHeight+18:14;text.style.height=`${Math.max(0,height-parseFloat(text.style.top)-bottom-(state.presentation==='window'?8:0))}px`;}
      const composer=node.querySelector('.card-composer');if(composer)composer.style.height=`${metrics.composerHeight}px`;
      const rule=node.querySelector('.window-composer-rule');if(rule)rule.style.bottom=`${metrics.composerHeight+26}px`;
      node._updateThumb?.();
    } else if(g.kind==='output') {
      g.next={x:p.x-6,y:p.y-10};
      $('gesture-preview').innerHTML=`<path d="${path({x:g.card.x+g.card.width-6,y:g.card.y+10},{x:p.x,y:p.y})}"/><rect x="${g.next.x}" y="${g.next.y}" width="176" height="62" rx="8"/>`;
    }
  });
  window.addEventListener('pointerup',e=>{
    if(!gesture)return;const g=gesture;gesture=null;canvas.classList.remove('is-panning');$('gesture-preview').replaceChildren();
    if(g.kind==='pan'){save();return;}
    if(g.kind==='move'){selectedCardId=g.card.id;if(g.moved){M.move(state,g.card.id,g.next.x,g.next.y);changed();}else cardsEl.querySelectorAll('.chat-card').forEach(n=>n.classList.toggle('is-selected',n.dataset.id===selectedCardId));}
    if(g.kind==='resize'&&g.moved){M.resize(state,g.card.id,g.next.width,g.next.autoHeight?null:g.next.height,{x:g.next.x,y:g.next.y});changed();}
    if(g.kind==='output') {
      if(!g.moved&&!g.card.isTail){status('Drag from this earlier card to create a branch, or choose “Fork chat here” in its menu.');return;}
      const id=M.continueFrom(state,g.card.id,{fork:!g.card.isTail,...(g.moved?g.next:{})});
      if(id){changed(id);revealActive();}
    }
  });
  canvas.addEventListener('wheel',e=>{
    if(gesture){e.preventDefault();return;}
    const transcript=e.target.closest('.transcript');
    if(transcript&&(state.presentation==='window'||transcript.closest('.chat-card').dataset.id===state.activeCardId)&&transcript.scrollHeight>transcript.clientHeight+1&&!e.ctrlKey&&!e.shiftKey) {
      transcript.closest('.chat-card')._userScrollAt=performance.now();
      e.preventDefault();transcript.scrollTop+=e.deltaY/camera.zoom;return;
    }
    e.preventDefault();closeMenu();
    if(e.shiftKey){camera.x-=e.deltaY;setCamera();save();return;}
    zoomAt(camera.zoom*StudioCanvasNavigation.factor(e.deltaY),e.clientX,e.clientY);
  },{passive:false});
  function zoomAt(zoom,x,y) {
    const anchor=point({clientX:x,clientY:y}),rect=canvas.getBoundingClientRect();
    camera.zoom=StudioCanvasNavigation.clamp(zoom);camera.x=x-rect.left-anchor.x*camera.zoom;camera.y=y-rect.top-anchor.y*camera.zoom;
    setCamera();save();
  }
  function fitView() {
    if(!cards.length)return;
    const left=Math.min(...cards.map(c=>c.x)),top=Math.min(...cards.map(c=>c.y));
    const right=Math.max(...cards.map(c=>c.x+c.width)),bottom=Math.max(...cards.map(c=>c.y+c.height));
    camera.zoom=Math.min(1.5,(canvas.clientWidth-100)/(right-left),(canvas.clientHeight-100)/(bottom-top));
    camera.zoom=Math.max(.25,camera.zoom);camera.x=(canvas.clientWidth-(right-left)*camera.zoom)/2-left*camera.zoom;camera.y=(canvas.clientHeight-(bottom-top)*camera.zoom)/2-top*camera.zoom;
    setCamera();save();
  }
  window.addEventListener('keydown',e=>{
    if(window.StudioShell&&window.StudioShell.activeId!=='chat-train')return;
    const typing=e.target.matches('input,textarea,select')||e.target.isContentEditable;
    if(e.key==='Escape') {
      closeMenu();$('dialogs').replaceChildren();$('hint').hidden=true;
      if(gesture){gesture=null;$('gesture-preview').replaceChildren();canvas.classList.remove('is-panning');draw();}
      return;
    }
    if(!typing&&e.code==='Space'){e.preventDefault();space=true;canvas.classList.add('space-held');}
    if(!typing&&(e.ctrlKey||e.metaKey)&&['z','y'].includes(e.key.toLowerCase())){
      e.preventDefault();const redo=e.key.toLowerCase()==='y'||e.shiftKey;
      if((redo?M.redo:M.undo)(state))changed();
    }
    if(!typing&&e.key==='Delete'&&state.activeCardId){if(M.deleteLeaf(state,state.activeCardId))changed();}
    if(e.target.closest('.native-menu')&&['ArrowDown','ArrowUp'].includes(e.key)){
      e.preventDefault();const rows=[...e.target.closest('.native-menu').querySelectorAll('button:not(:disabled)')];const index=rows.indexOf(document.activeElement);rows[(index+(e.key==='ArrowDown'?1:rows.length-1))%rows.length].focus();
    }
  });
  window.addEventListener('keyup',e=>{if(e.code==='Space'){space=false;canvas.classList.remove('space-held');}});
  window.addEventListener('blur',()=>{space=false;canvas.classList.remove('space-held');});
  document.addEventListener('pointerdown',e=>{if(!e.target.closest('.native-menu,.header-menu,.model-name'))closeMenu();});
  document.addEventListener('pointerover',e=>{
    const target=e.target.closest('[data-hint]');clearTimeout(hintTimer);$('hint').hidden=true;if(!target)return;
    hintTimer=setTimeout(()=>{
      if(gesture)return;const box=target.getBoundingClientRect(),hint=$('hint');hint.textContent=target.dataset.hint;hint.hidden=false;
      hint.style.left=`${Math.max(5,Math.min(innerWidth-hint.offsetWidth-5,box.left))}px`;hint.style.top=`${Math.min(innerHeight-hint.offsetHeight-5,box.bottom+8)}px`;
    },750);
  });
  document.addEventListener('pointerout',e=>{if(e.target.closest('[data-hint]')===e.relatedTarget?.closest?.('[data-hint]'))return;clearTimeout(hintTimer);$('hint').hidden=true;});
  $('zoom-out').addEventListener('click',()=>zoomAt(camera.zoom/1.2,innerWidth/2,innerHeight/2));
  $('zoom-in').addEventListener('click',()=>zoomAt(camera.zoom*1.2,innerWidth/2,innerHeight/2));
  $('zoom-reset').addEventListener('click',()=>zoomAt(1,innerWidth/2,innerHeight/2));
  $('fit-view').addEventListener('click',fitView);
  $('reset-example').addEventListener('click',()=>{clearInterval(timer);state=seed();scrolls.clear();readerPaused.clear();camera={x:12,y:0,zoom:1};changed();});
  $('theme-button').addEventListener('click',()=>{
    const dark=document.documentElement.dataset.theme!=='dark';document.documentElement.dataset.theme=dark?'dark':'light';
    $('theme-button').textContent=dark?'Light':'Dark';try{localStorage.setItem('slate-chat-train-theme',dark?'dark':'light');}catch{}
  });
  try {if(localStorage.getItem('slate-chat-train-theme')==='dark'){$('theme-button').click();}}catch{}
  $('reference-button').addEventListener('click',()=>{
    const panel=dialog('Native chat train reference');
    panel.append(element('p','','Slate renderer capture. The canvas uses the same card dimensions, text font, colors, and controls. Replies on the canvas are local samples.'));
    const img=element('img');img.src=`chat-train/${R.references[document.documentElement.dataset.theme==='dark'?1:0].file}`;img.alt='Slate native renderer chat card';panel.append(img);
    closeButton(panel);
  });
  $('notes-button').addEventListener('click',()=>{const open=$('notes-panel').hidden;$('notes-panel').hidden=!open;$('notes-button').setAttribute('aria-expanded',String(open));if(open)$('notes').focus();});
  try{$('notes').value=localStorage.getItem('slate-chat-train-review-notes')||'';}catch{}
  $('notes').addEventListener('input',()=>{try{localStorage.setItem('slate-chat-train-review-notes',$('notes').value);$('note-status').textContent='Saved on this device';}catch{$('note-status').textContent='Use Save notes to keep a copy';}});
  $('export-notes').addEventListener('click',()=>{
    const blob=new Blob([JSON.stringify({baseline:'Native chat train',notes:$('notes').value,date:new Date().toISOString()},null,2)],{type:'application/json'});
    const a=element('a');a.href=URL.createObjectURL(blob);a.download='chat-train-review.json';a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000);
  });
  for(const [id,hint]of[['reference-button','Open a capture from Slate’s native renderer.'],['notes-button','Studio review notes. These are outside the chat interface.'],['reset-example','Restore the original sample conversation.'],['theme-button','Switch between Slate’s light and dark colors.']])$(id).dataset.hint=hint;
  draw();
  // Read-only inspection surface for fixture comparison and interaction checks.
  window.ChatTrainStudio={get state(){return state;},get cards(){return cards;},get camera(){return {...camera};},measure,fitView};
})();
