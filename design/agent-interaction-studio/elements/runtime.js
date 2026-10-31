/* One runtime owns shared chrome. Element modules supply only their content and preview stages. */
(() => {
  'use strict';
  const M=StudioSession,instances=new Map(),$=id=>document.getElementById(id);
  const make=(tag,cls,text)=>{const e=document.createElement(tag);if(cls)e.className=cls;if(text!=null)e.textContent=text;return e;};
  const load=(key,fallback)=>{try{return JSON.parse(localStorage.getItem(key))||fallback;}catch{return fallback;}};
  const prefKey='slate-element-agent-preferences-v1';
  const preferences=load(prefKey,{config:ChatAgentCatalog.normalize({provider:'codex',model:'Sol'}),access:'workspace'});
  function closeMenus(){ChatAgentPicker.close();$('menus').replaceChildren();}
  function icon(name){
    const paths={robot:'M4 5h12v12H4zM7 9h1m4 0h1M8 13h4M10 2v3M2 9v5m16-5v5',
      chevron:'m7 5 5 5-5 5',send:'m3 3 14 7-14 7 3-7zM6 10h11',stop:'M5 5h10v10H5z',
      history:'M4 4v5h5M4 9a7 7 0 1 1 0 4M10 6v5l3 2',code:'m7 5-5 5 5 5m6-10 5 5-5 5M12 3 8 17',
      fork:'M4 3v14m0-7h7l5-5m-5 5 5 5',fit:'M3 7V3h4m6 0h4v4M3 13v4h4m6 0h4v-4',close:'m5 5 10 10M15 5 5 15'};
    const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 20 20');svg.setAttribute('aria-hidden','true');
    const path=document.createElementNS(svg.namespaceURI,'path');path.setAttribute('d',paths[name]||paths.robot);svg.append(path);return svg;
  }
  function button(label,hint,fn,cls='',glyph){const b=make('button',cls,glyph?null:label);b.type='button';b.setAttribute('aria-label',label);b.dataset.hint=hint||label;b.title=hint||label;b.addEventListener('pointerdown',e=>e.stopPropagation());b.onclick=e=>{e.stopPropagation();fn(b);};if(glyph)b.append(icon(glyph));return b;}
  function menu(anchor,rows){
    closeMenus();const box=anchor.getBoundingClientRect(),el=make('div','native-menu element-menu');el.setAttribute('role','menu');
    const draw=()=>{el.replaceChildren();for(const row of rows()){
      if(!row){el.append(make('div','menu-rule'));continue;}
      const b=button(row.label,row.hint,()=>{if(!row.stay)closeMenus();row.action();if(row.stay)draw();},'menu-row');
      b.setAttribute('role',row.checked!==undefined?'menuitemcheckbox':'menuitem');if(row.checked!==undefined)b.setAttribute('aria-checked',String(row.checked));
      b.prepend(make('span','element-menu-check',row.checked?'✓':''));el.append(b);
    }el.style.left=Math.max(8,Math.min(innerWidth-240,box.left))+'px';el.style.top=Math.max(8,Math.min(innerHeight-el.offsetHeight-8,box.bottom+4))+'px';};
    $('menus').append(el);draw();
  }
  function init(config){
    const storage='slate-element-'+config.id+'-v1',saved=load(storage,null);
    const nodes=saved?.nodes?.length?saved.nodes:[M.create(config)];
    nodes.forEach(n=>{if(n.running){n.running=false;n.events.push({label:'Interrupted',detail:'The page closed before this preview finished.'});}});
    let camera=saved?.camera||{x:0,y:0,zoom:1},drag=null,wire=null;
    const timers=new Map(),canvas=make('main','element-canvas'),world=make('div','element-world'),wires=document.createElementNS('http://www.w3.org/2000/svg','svg'),content=make('div','element-nodes');
    canvas.id=config.id+'-canvas';canvas.hidden=true;canvas.dataset.elementStudio=config.id;canvas.setAttribute('aria-label',config.title+' canvas');
    wires.classList.add('element-wires');world.append(wires,content);canvas.append(world);
    const footer=make('footer','studio-bottom element-footer');footer.hidden=true;footer.dataset.elementStudio=config.id;
    const note=make('span','','Interactive design preview · no live tools');
    const zoomLabel=button('100%','Reset canvas zoom',()=>nav.zoom(1));const actions=make('div');
    actions.append(button('−','Zoom out',()=>nav.zoom(camera.zoom/1.2)),zoomLabel,button('+','Zoom in',()=>nav.zoom(camera.zoom*1.2)),button('Fit','Fit this Studio canvas',fit));footer.append(note,actions);document.body.append(canvas,footer);
    const save=()=>{try{localStorage.setItem(storage,JSON.stringify({nodes,camera}));}catch{note.textContent='Browser storage is full; keep this page open to retain changes.';}};
    const persistPreferences=()=>{localStorage.setItem(prefKey,JSON.stringify(preferences));};
    function transform(){world.style.transform=`translate(${camera.x}px,${camera.y}px) scale(${camera.zoom})`;zoomLabel.textContent=Math.round(camera.zoom*100)+'%';}
    function fit(){
      const left=Math.min(...nodes.map(n=>n.x))-40,top=Math.min(...nodes.map(n=>n.y))-35;
      const right=Math.max(...nodes.map(n=>n.x+n.width))+40,bottom=Math.max(...nodes.map(n=>n.y+(n._visualHeight||n.height+290)))+30;
      camera.zoom=Math.min(1,(canvas.clientWidth-35)/(right-left),(canvas.clientHeight-25)/(bottom-top));camera.x=(canvas.clientWidth-(right-left)*camera.zoom)/2-left*camera.zoom;camera.y=15-top*camera.zoom;transform();save();
    }
    function drawWires(){
      wires.replaceChildren();for(const n of nodes){if(!n.parent)continue;const p=nodes.find(p=>p.id===n.parent);if(!p)continue;
        const path=document.createElementNS(wires.namespaceURI,'path'),a=[p.x+p.width-6,p.y+12],b=[n.x+6,n.y+12],dx=Math.max(60,(b[0]-a[0])*.5);
        path.setAttribute('d',`M${a} C${a[0]+dx},${a[1]} ${b[0]-dx},${b[1]} ${b}`);wires.append(path);
        const hit=path.cloneNode();hit.classList.add('wire-hit');hit.setAttribute('role','button');hit.setAttribute('aria-label','Select history connection to '+n.id);hit.setAttribute('tabindex','0');
        hit.onclick=e=>{e.stopPropagation();document.activeElement?.blur();wire=n;wires.querySelectorAll('.selected').forEach(p=>p.classList.remove('selected'));path.classList.add('selected');};wires.append(hit);
      }
    }
    const nodeRoot=n=>content.querySelector(`[data-element-node="${n.id}"]`);
    function select(n){for(const other of nodes){other.selected=other===n;nodeRoot(other)?.classList.toggle('is-selected',other.selected);}wire=null;}
    function update(n,options){save();if(options?.render===false)return;render(n);drawWires();}
    function setSelection(n,selection){n.selection=selection;save();renderDock(n);}
    function fork(n){
      if(n.running)return;const next=M.branch(n,'node-'+Date.now());n.open=false;select(n);n.selected=false;nodes.push(next);render(n);render(next);drawWires();save();fit();
      nodeRoot(next)?.querySelector('textarea')?.focus({preventScroll:true});return next;
    }
    function options(n,anchor){menu(anchor,()=>[
      {label:'Single dialogue',checked:n.mode==='single',stay:true,hint:'Keep the conversation and composer in this window.',action:()=>{n.mode='single';update(n);}},
      {label:'Chat train',checked:n.mode==='train',stay:true,hint:'Send each next iteration into a connected window.',action:()=>{n.mode='train';update(n);}},null,
      {label:'Preserve model history',checked:n.preserve,stay:true,hint:'Carry the conversation into downstream windows.',action:()=>{n.preserve=!n.preserve;update(n);}},null,
      {label:'Continue in new window',hint:'Create a downstream window with inherited history.',action:()=>fork(n)},
      {label:'View inherited history',hint:'Read the context carried into this window.',action:()=>showHistory(n)},
      {label:'Access settings',hint:'Remember access settings across the three new Studios.',action:()=>accessMenu(n,anchor)},
      ...((config.tools||[]).length?[null,...config.tools.map(t=>({label:t.label,hint:t.hint,action:()=>{t.action(n);update(n);}}))]:[])
    ]);}
    function accessMenu(n,anchor){menu(anchor,()=>['ask','workspace','full'].map(id=>({label:{ask:'Ask before actions',workspace:'Workspace access',full:'Full access'}[id],checked:preferences.access===id,stay:true,hint:'Preview preference only; does not grant this page access to files or applications.',action:()=>{preferences.access=id;persistPreferences();renderDock(n);}})));}
    function showHistory(n){const cover=make('div','modal-cover'),dialog=make('section','reference-dialog element-history-dialog');dialog.setAttribute('role','dialog');dialog.setAttribute('aria-label','Inherited conversation');dialog.append(make('h2','','Inherited conversation'),button('Close','Close history',()=>cover.remove()));
      if(!n.inherited?.length)dialog.append(make('p','','No inherited messages.'));for(const t of n.inherited||[])dialog.append(make('p',t.role==='user'?'is-user':'',t.text));cover.append(dialog);$('dialogs').replaceChildren(cover);}
    function renderDock(n){
      const root=nodeRoot(n);if(!root)return;const previous=root.querySelector('.element-agent'),oldComposer=previous?.querySelector('textarea'),focused=oldComposer===document.activeElement,caret=focused?[oldComposer.selectionStart,oldComposer.selectionEnd]:null,oldTranscript=previous?.querySelector('.element-transcript'),reader=oldTranscript&&oldTranscript.scrollTop+oldTranscript.clientHeight<oldTranscript.scrollHeight-3?oldTranscript.scrollTop:null;previous?.remove();
      const dock=make('section','element-agent');dock.hidden=!n.open;dock.setAttribute('aria-label',config.title+' agent');
      const header=make('header','element-agent-header'),model=ChatAgentCatalog.normalize(preferences.config),p=ChatAgentCatalog.providers.find(p=>p.id===model.provider),m=p.models.find(m=>m.id===model.model);
      const picker=button(`${m.label} ${ChatAgentCatalog.labels[model.effort]||''}`,'Provider, model, effort and speed; API key settings are under the provider chevron.',b=>{ChatAgentPicker.open({anchor:b,card:preferences.config,onChange:c=>{preferences.config=c;persistPreferences();renderDock(n);},onClose:()=>{},onStatus:t=>note.textContent=t});},'element-model');
      const speed=make('span','element-speed', 'ϟ'.repeat(ChatAgentCatalog.speedInfo[model.speed]?.bolts||0));picker.prepend(speed);picker.append(icon('chevron'));picker.title=`${p.label} · ${m.label} · ${ChatAgentCatalog.labels[model.effort]} · ${ChatAgentCatalog.labels[model.speed]}`;
      header.append(picker,button('Agent options','Dialogue style, preserved history and access.',b=>options(n,b),'element-more'));
      header.lastChild.textContent='•••';header.append(make('span','element-mode',n.mode==='train'?'Chat train':'Single dialogue'));
      const act=button(n.activity?'Hide activity':'Show activity','Public progress, tool actions and generated scripts.',()=>{n.activity=!n.activity;update(n);},'element-icon','code');header.append(act);dock.append(header);
      if(n.selection?.text){const selection=make('div','element-selection');selection.append(make('span','','Selected text'),make('q','',n.selection.text),button('Clear text selection','Stop targeting the highlighted passage.',()=>{n.selection=null;update(n);},'element-icon','close'));dock.append(selection);}
      if(n.data?.selectedWidget)dock.append(make('div','element-selection','Target: '+n.data.selectedWidget));
      const transcript=make('div','element-transcript');transcript.dataset.captureWheel='true';transcript.setAttribute('aria-label','Conversation');transcript.tabIndex=0;
      for(const t of n.turns){const turn=make('p','element-turn '+(t.role==='user'?'is-user':'is-agent'),t.text);if(t.selection?.text)turn.prepend(make('q','element-quoted-selection',t.selection.text));transcript.append(turn);}dock.append(transcript);
      const composer=make('textarea','element-composer');composer.rows=1;composer.value=n.prompt;composer.placeholder=config.id==='text-agent'&&n.selection?'Ask about the selected text…':'Describe the next change…';composer.dataset.captureWheel='true';composer.setAttribute('aria-label',config.title+' prompt');composer.onpointerdown=e=>e.stopPropagation();
      composer.oninput=()=>{n.prompt=composer.value;composer.style.height='0px';composer.style.height=Math.max(38,Math.min(110,composer.scrollHeight))+'px';save();};composer.onkeydown=e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();run(n);}if(e.key==='Escape')composer.blur();};dock.append(composer);
      const status=make('div','element-run-row'),label=make('span','element-run-status',n.running?n.events.at(-1)?.label||'Starting preview':n.events.length?n.events.at(-1).label:'Ready');label.setAttribute('role','status');label.setAttribute('aria-live','polite');
      status.append(make('i',n.running?'element-status-dot is-running':'element-status-dot'),label,make('span','element-preview-only','Preview'),button(n.running?'Stop preview':'Send message',n.running?'Stop the simulated run and retain completed output.':'Send this instruction in the Studio simulation.',()=>n.running?stop(n):run(n),'element-send',n.running?'stop':'send'));dock.append(status);
      if(n.activity){
        const activity=make('section','element-activity');activity.setAttribute('aria-label','Public activity');
        const tabs=make('div','element-activity-tabs');for(const [id,label] of [['progress','Progress'],['script','Script']]){const b=button(label,id==='script'?'Generated code; no hidden reasoning.':'Observable steps and tool results.',()=>{n.activityTab=id;update(n);});b.setAttribute('aria-pressed',String((n.activityTab||'progress')===id));tabs.append(b);}activity.append(tabs);
        const output=make('div','element-activity-output');output.dataset.captureWheel='true';
        if(n.activityTab==='script'){const code=make('pre','',n.script||'Generated script will appear when a preview runs.');output.append(code);}else if(!n.events.length)output.append(make('p','','No run in progress. Send an instruction to inspect the preview steps.'));else for(const e of n.events){const line=make('div','element-event');line.append(make('strong','',e.label),make('span','',e.detail));output.append(line);}activity.append(output);dock.append(activity);StudioScroll.attach(output,activity);
      }
      root.append(dock);requestAnimationFrame(()=>{if(!root.isConnected)return;transcript.scrollTop=reader??transcript.scrollHeight;if(focused){composer.focus({preventScroll:true});composer.setSelectionRange(...caret);}StudioScroll.attach(transcript,dock);StudioScroll.attach(composer,dock);n._visualHeight=root.offsetHeight;drawWires();});
    }
    function render(n){
      let root=nodeRoot(n);if(!root){root=make('section','element-node');root.dataset.elementNode=n.id;root.setAttribute('aria-label',config.title+' window');root.onpointerdown=e=>beginDrag(e,n);content.append(root);}
      root.replaceChildren();root.classList.toggle('is-selected',n.selected);Object.assign(root.style,{left:n.x+'px',top:n.y+'px',width:n.width+'px'});
      const caption=make('div','element-caption');caption.append(make('span','',config.title),make('span','',n.parent?'Continued window':'Design preview'));root.append(caption);
      const preview=make('div','element-preview');preview.style.minHeight=n.height+'px';
      preview.append(config.renderPreview({node:n,make,update:opts=>update(n,opts),setSelection:s=>setSelection(n,s)}));root.append(preview);
      const bar=make('div','element-tools');bar.append(button('Toggle agent','Open or close the agent dialogue.',()=>{select(n);n.open=!n.open;update(n);},'element-icon','robot'));
      bar.append(make('span','',`Revision ${n.revision+1}`),button('Continue in new window','Carry this conversation into a new window.',()=>fork(n),'element-icon','fork'));root.append(bar);
      const out=button('Drag history to a new window','Drag to create a downstream window.',()=>{},'element-history-out');out.textContent='';out.onpointerdown=e=>beginDrag(e,n,'branch');root.append(out);
      if(n.parent)root.append(make('i','element-history-in'));
      const resize=make('div','element-resize');resize.setAttribute('role','separator');resize.setAttribute('aria-label','Resize '+config.title+' window');resize.onpointerdown=e=>beginDrag(e,n,'resize');root.append(resize);renderDock(n);
    }
    function beginDrag(e,n,kind='move'){
      if(e.button!==0)return;if(kind==='move'&&e.target.closest('button,textarea,input,select,[contenteditable=true],.element-preview'))return;
      e.preventDefault();e.stopPropagation();drag={node:n,kind,x:e.clientX,y:e.clientY,ox:n.x,oy:n.y,ow:n.width,moved:false};canvas.setPointerCapture(e.pointerId);
    }
    canvas.addEventListener('pointermove',e=>{if(!drag)return;const dx=(e.clientX-drag.x)/camera.zoom,dy=(e.clientY-drag.y)/camera.zoom;if(Math.hypot(e.clientX-drag.x,e.clientY-drag.y)>4)drag.moved=true;if(!drag.moved)return;const n=drag.node;if(drag.kind==='move'){n.x=drag.ox+dx;n.y=drag.oy+dy;nodeRoot(n).style.left=n.x+'px';nodeRoot(n).style.top=n.y+'px';drawWires();}else if(drag.kind==='resize'){n.width=Math.max(420,Math.min(1100,drag.ow+dx));render(n);drawWires();}});
    const endDrag=e=>{if(!drag)return;const d=drag;drag=null;if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);if(e.type==='pointercancel'){d.node.x=d.ox;d.node.y=d.oy;d.node.width=d.ow;render(d.node);}else if(d.kind==='branch'&&d.moved){const box=canvas.getBoundingClientRect(),drop={x:(e.clientX-box.left-camera.x)/camera.zoom,y:(e.clientY-box.top-camera.y)/camera.zoom};const next=fork(d.node);if(next){next.x=drop.x;next.y=drop.y;render(next);drawWires();fit();}}else if(!d.moved)select(d.node);save();};canvas.addEventListener('pointerup',endDrag);canvas.addEventListener('pointercancel',endDrag);
    function stop(n){clearInterval(timers.get(n.id));timers.delete(n.id);M.stop(n);update(n);}
    function run(source){
      if(source.running||!source.prompt.trim())return;
      if(source.selection&&source.selection.revision!==(source.data.textRevision||0)){note.textContent='The selected text changed. Select the passage again before sending.';return;}
      const prompt=source.prompt,selection=source.selection?M.copy(source.selection):null;
      let n=source;if(source.mode==='train'&&source.turns.some(t=>t.role==='user')){n=fork(source);source.prompt='';n.prompt=prompt;n.selection=selection;}
      if(!M.begin(n,prompt,selection))return;n.activity=true;n.activityTab=config.id==='rhino-agent'?'script':'progress';
      n.startedAt=Date.now();update(n);
      const advance=()=>{
        n.stage++;const stage=config.stages[n.stage];
        if(stage){n.events.push({label:stage.label,detail:stage.detail});if(stage.code)n.script+=(n.script?'\n\n':'')+stage.code;renderDock(n);save();return;}
        clearInterval(timers.get(n.id));timers.delete(n.id);config.applyResult(n,prompt);
        M.finish(n,config.id==='text-agent'?'The proposed revision is ready. Review it before applying it to the selected text.':config.id==='rhino-agent'?'The model preview and generated Rhino script are ready to inspect. This Studio has not executed commands in Rhino.':'The dashboard preview is ready. Inspect the changes, then apply or discard them.');
        n.events.push({label:'Complete',detail:'Preview finished. No external tool was called.'});update(n);requestAnimationFrame(fit);
      };
      advance();requestAnimationFrame(fit);timers.set(n.id,setInterval(advance,1000));
    }
    const nav=StudioCanvasNavigation.bind({canvas,camera,transform,closeMenus,deselect:()=>{nodes.forEach(n=>n.selected=false);content.querySelectorAll('.is-selected').forEach(n=>n.classList.remove('is-selected'));wire=null;drawWires();save();},context:e=>{const n=nodes.find(n=>n.id===e.target.closest('[data-element-node]')?.dataset.elementNode);if(n)menu({getBoundingClientRect:()=>({left:e.clientX,bottom:e.clientY})},()=>[{label:'Open agent',action:()=>{n.open=true;update(n);}},{label:'Continue in new window',action:()=>fork(n)},{label:'Fit canvas',action:fit}]);}});
    canvas.addEventListener('click',e=>{const n=nodes.find(n=>n.id===e.target.closest('[data-element-node]')?.dataset.elementNode);if(n)select(n);});
    window.addEventListener('keydown',e=>{if(canvas.hidden)return;if(e.key==='Escape'){wire=null;closeMenus();drawWires();}if(['Delete','Backspace'].includes(e.key)&&wire&&!e.target.closest('textarea,input,[contenteditable]')){e.preventDefault();if(!nodes.some(n=>n.parent===wire.id)){M.cut(wire);wire=null;save();renderAll();}}});
    function renderAll(){nodes.forEach(render);drawWires();}
    window.addEventListener('studio-element-changed',e=>{const visible=e.detail.id===config.id;canvas.hidden=!visible;footer.hidden=!visible;if(visible){renderAll();requestAnimationFrame(fit);}else closeMenus();});
    window.addEventListener('resize',()=>{if(!canvas.hidden)fit();});
    instances.set(config.id,{canvas,footer,renderAll});renderAll();transform();
  }
  for(const config of window.StudioElements||[])init(config);
  document.addEventListener('pointerdown',e=>{if(instances.has(StudioShell.activeId)&&!e.target.closest('.native-menu,.element-model,.element-more'))closeMenus();});
  const requested=new URLSearchParams(location.search).get('tab');if(instances.has(requested))StudioShell.activate(requested);
})();
