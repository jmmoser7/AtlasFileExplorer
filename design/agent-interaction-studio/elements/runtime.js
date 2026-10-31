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
    if(window.ElementNativeIcons?.[name]||window.ImageStudioIcons?.[name]){const def=ElementNativeIcons[name]||ImageStudioIcons[name],svg=document.createElementNS('http://www.w3.org/2000/svg','svg'),path=document.createElementNS(svg.namespaceURI,'path');svg.setAttribute('viewBox',(def.view_box||[0,0,24,24]).join(' '));svg.setAttribute('aria-hidden','true');path.setAttribute('d',def.path);path.setAttribute('fill',def.filled?'currentColor':'none');path.setAttribute('stroke',def.filled?'none':'currentColor');path.setAttribute('stroke-width',def.stroke_width||.85);path.setAttribute('stroke-linecap','round');path.setAttribute('stroke-linejoin','round');svg.append(path);return svg;}
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
      b.prepend(make('span','element-menu-check',row.checked?'✓':''));if(row.expand){b.setAttribute('aria-expanded',String(!!row.expanded));const arrow=make('span','element-menu-arrow');arrow.append(icon('chevron'));b.append(arrow);}el.append(b);
    }el.style.left=Math.max(8,Math.min(innerWidth-240,box.left))+'px';el.style.top=Math.max(8,Math.min(innerHeight-el.offsetHeight-8,box.bottom+4))+'px';};
    $('menus').append(el);draw();
  }
  function init(config){
    const storage='slate-element-'+config.id+'-v1',saved=load(storage,null);
    const nodes=saved?.nodes?.length?saved.nodes:[M.create(config)];
    for(const n of nodes)if(n.chromeVersion!==2){n.chromeVersion=2;n.width=config.id==='text-agent'?480:680;n.height=config.id==='text-agent'?280:360;n.interactive=false;n.preserve=false;n.activity=false;n.activityFold=0;n.open=n===nodes[0];n.selected=n===nodes[0];n.data&&delete n.data.selectedWidget;}
    const tools=config.propertyTools||(config.id==='text-agent'?[{id:'fill',label:'Fill',icon:'Fill'},{id:'stroke',label:'Stroke',icon:'Ellipse'},{id:'agent',label:'Agent',icon:'Agent'}]:[{id:'fill',label:'Fill color',icon:'Fill'},{id:'stroke',label:'Stroke color and width',icon:'Ellipse'},...(config.id==='text-agent'?[{id:'text-format',label:'Text formatting',icon:'Sliders'}]:[{id:'corners',label:'Corners and crop',icon:'Corners'}]),...(config.nativeTools||[]),{id:'agent',label:'Agent',icon:'Agent'}]);
    const rect=(el,r)=>Object.assign(el.style,{left:r[0]+'px',top:r[1]+'px',width:r[2]+'px',height:r[3]+'px'});
    const layoutFor=n=>ImageStudioLayout.layout(n.width,n.height,false,n.open,n.revision+1,Math.min(120,90),tools.length);
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
    function fit(target){
      const framed=target&&typeof target==='object'&&target.width?[target]:nodes;
      const left=Math.min(...framed.map(n=>n.x))-40,top=Math.min(...framed.map(n=>n.y))-35;
      const right=Math.max(...framed.map(n=>n.x+n.width))+40,bottom=Math.max(...framed.map(n=>n.y+(n._visualHeight||n.height+290)))+30;
      camera.zoom=Math.min(1,(canvas.clientWidth-35)/(right-left),(canvas.clientHeight-25)/(bottom-top));camera.x=(canvas.clientWidth-(right-left)*camera.zoom)/2-left*camera.zoom;camera.y=15-top*camera.zoom;transform();save();
    }
    function drawWires(){
      wires.replaceChildren();for(const n of nodes){const upstream=nodes.find(p=>p.id===(n.source||n.parent));if(upstream){const data=document.createElementNS(wires.namespaceURI,'path');data.classList.add('element-data-wire');data.setAttribute('d',ImageStudioLayout.bezier([upstream.x+upstream.width+ImageStudioLayout.portOffset,upstream.y+48],[n.x-ImageStudioLayout.portOffset,n.y+48]));wires.append(data);}if(!n.parent)continue;const p=nodes.find(p=>p.id===n.parent);if(!p)continue;
        const path=document.createElementNS(wires.namespaceURI,'path'),a=[p.x+p.width-6,p.y+12],b=[n.x+6,n.y+12],dx=Math.max(60,(b[0]-a[0])*.5);
        path.setAttribute('d',`M${a} C${a[0]+dx},${a[1]} ${b[0]-dx},${b[1]} ${b}`);wires.append(path);
        const hit=path.cloneNode();hit.classList.add('wire-hit');hit.setAttribute('role','button');hit.setAttribute('aria-label','Select history connection to '+n.id);hit.setAttribute('tabindex','0');
        hit.onclick=e=>{e.stopPropagation();document.activeElement?.blur();wire=n;wires.querySelectorAll('.selected').forEach(p=>p.classList.remove('selected'));path.classList.add('selected');};wires.append(hit);
      }
    }
    const nodeRoot=n=>content.querySelector(`[data-element-node="${n.id}"]`);
    function select(n){for(const other of nodes){if(other!==n&&other.selected){other.selected=false;other.open=false;render(other);}}n.selected=true;nodeRoot(n)?.classList.add('is-selected');wire=null;}
    function update(n,options){save();if(options?.render===false)return;render(n);drawWires();}
    function setSelection(n,selection){n.selection=selection;save();renderDock(n);}
    function fork(n){
      if(n.running)return;const next=M.branch(n,'node-'+Date.now());next.source=n.id;next.interactive=n.interactive;next.open=true;n.open=false;select(n);n.selected=false;nodes.push(next);render(n);render(next);drawWires();save();fit();
      nodeRoot(next)?.querySelector('textarea')?.focus({preventScroll:true});return next;
    }
    function options(n,anchor){
      let page='';menu(anchor,()=>{
        const settings={label:'Settings',expand:true,expanded:page==='settings',hint:'Native generator actions.',stay:true,action:()=>{page=page==='settings'?'':'settings';}};
        if(page==='settings')return [settings,null,
          ...(n.data?.pending?(config.tools||[]).filter(t=>['apply-preview','discard-preview'].includes(t.id)).map(t=>({label:t.label,action:()=>{t.action(n);update(n);}})):[]),
          {label:'Fork',action:()=>fork(n)},
          {label:'Fresh eyes',hint:'Start without inherited model context.',action:()=>{M.cut(n);n.interactive=false;update(n);}},
          {label:'Inspect memory',action:()=>showHistory(n)},
          {label:'Open source',hint:'Inspect generated source in this preview.',action:()=>showSource(n)}];
        return [settings,null,
          {label:'Preserve model history',checked:n.interactive,stay:true,hint:'Keep a dialogue here and carry its context downstream.',action:()=>{n.interactive=!n.interactive;n.preserve=n.interactive;update(n);}},
          ...(config.id==='text-agent'?[{label:'Create chat train',hint:'Continue this text in a downstream conversation.',action:()=>{n.preserve=true;const child=fork(n);if(child){child.interactive=true;child.preserve=true;update(child);}}}]:[]),null,
          {label:'Full access',checked:preferences.access==='full',stay:true,hint:'Persistent preview access preference; no real application permissions change.',action:()=>{preferences.access=preferences.access==='full'?'workspace':'full';persistPreferences();}},
          ...(config.id==='rhino-agent'?[{label:'View generated script',action:()=>showSource(n)}]:[])
        ];
      });
    }
    function showSource(n){const cover=make('div','modal-cover'),dialog=make('section','reference-dialog element-source-dialog');dialog.setAttribute('role','dialog');dialog.setAttribute('aria-label','Generated source');dialog.append(make('h2','','Generated source'),button('Close','Close source',()=>cover.remove()),make('pre','',n.script||'No generated source yet.'));cover.append(dialog);$('dialogs').replaceChildren(cover);}
    function accessMenu(n,anchor){menu(anchor,()=>['ask','workspace','full'].map(id=>({label:{ask:'Ask before actions',workspace:'Workspace access',full:'Full access'}[id],checked:preferences.access===id,stay:true,hint:'Preview preference only; does not grant this page access to files or applications.',action:()=>{preferences.access=id;persistPreferences();renderDock(n);}})));}
    function showHistory(n){const cover=make('div','modal-cover'),dialog=make('section','reference-dialog element-history-dialog');dialog.setAttribute('role','dialog');dialog.setAttribute('aria-label','Inherited conversation');dialog.append(make('h2','','Inherited conversation'),button('Close','Close history',()=>cover.remove()));
      if(!n.inherited?.length)dialog.append(make('p','','No inherited messages.'));for(const t of n.inherited||[])dialog.append(make('p',t.role==='user'?'is-user':'',t.text));cover.append(dialog);$('dialogs').replaceChildren(cover);}
    function renderDock(n){
      const root=nodeRoot(n);if(!root)return;
      const previous=root.querySelector('.element-agent'),oldComposer=previous?.querySelector('textarea'),focused=oldComposer===document.activeElement,caret=focused?[oldComposer.selectionStart,oldComposer.selectionEnd]:null,oldTranscript=previous?.querySelector('.element-transcript'),reader=oldTranscript&&oldTranscript.scrollTop+oldTranscript.clientHeight<oldTranscript.scrollHeight-3?oldTranscript.scrollTop:null;const oldActivity=previous?.querySelector('.element-activity-output'),activityScroll=oldActivity&&oldActivity.scrollTop+oldActivity.clientHeight<oldActivity.scrollHeight-3?oldActivity.scrollTop:null;previous?.remove();
      const l=layoutFor(n);n.layout=l;
      if(!n.open||!n.selected){n._visualHeight=n.height+(l.attached?24:50);return;}
      const dock=make('section','element-agent image-editor'+(l.attached?'':' is-detached'));dock.setAttribute('aria-label',config.title+' agent');Object.assign(dock.style,{left:l.panelX+'px',top:l.panelY+'px',width:l.panelWidth+'px'});
      const header=make('header','element-agent-header'),model=ChatAgentCatalog.normalize(preferences.config),p=ChatAgentCatalog.providers.find(p=>p.id===model.provider),m=p.models.find(m=>m.id===model.model);
      const picker=button(p.label+' · '+m.label,'Provider, model, effort and speed; API key uses its own chevron.',b=>ChatAgentPicker.open({anchor:b,card:preferences.config,onChange:c=>{preferences.config=c;persistPreferences();renderDock(n);},onClose:()=>{},onStatus:t=>note.textContent=t}),'image-control image-field element-model');picker.append(make('span','image-caret'));rect(picker,l.controls.model);
      const more=button('Agent options','Settings and model history.',b=>options(n,b),'image-control image-more element-more');more.innerHTML='<i></i><i></i><i></i>';rect(more,l.controls.more);
      const play=button(n.running?'Stop preview':'Send message',n.running?'Stop this preview run.':'Generate the next revision.',()=>n.running?stop(n):run(n),'image-control generate-image element-play');play.textContent='';play.append(n.running?make('span','element-stop'):make('span','image-play'));rect(play,l.controls.play);
      header.append(picker,more,play);
      n.versions||=[M.copy(n.data)];const revisions=make('div','element-revisions');revisions.setAttribute('aria-label','Revision history');const start=l.controls.more[0]+24;rect(revisions,[start,2,l.controls.play[0]-start-8,24]);
      n.versions.forEach((_,i)=>{const chip=button('Revision '+(i+1),'View revision '+(i+1),()=>{n.data=M.copy(n.versions[i]);n.selectedRevision=i;update(n);},'element-revision');chip.textContent=String(i+1);chip.setAttribute('aria-pressed',String((n.selectedRevision??n.versions.length-1)===i));revisions.append(chip);});header.append(revisions);dock.append(header);
      const body=make('div','element-agent-body');
      if(n.selection?.text){const selection=make('div','element-selection');selection.append(make('q','',n.selection.text),button('Clear text selection','Stop targeting this text.',()=>{n.selection=null;update(n);},'element-clear','close'));body.append(selection);}
      let transcript=null;
      if(n.interactive){transcript=make('div','element-transcript');transcript.dataset.captureWheel='true';transcript.setAttribute('aria-label','Conversation');transcript.tabIndex=0;for(const t of n.turns){const turn=make('p','element-turn '+(t.role==='user'?'is-user':'is-agent'),t.text);if(t.selection?.text)turn.prepend(make('q','element-quoted-selection',t.selection.text));transcript.append(turn);}body.append(transcript);}
      if(n.running||n.events.length){
        const summary=make('div','element-summary'),text=make('span','element-run-status',n.running?n.events.at(-1)?.label||'Starting…':n.data?.pending?'Preview ready · Settings: Apply or Discard':n.events.at(-1)?.label||'Complete');text.setAttribute('role','status');text.setAttribute('aria-live','polite');summary.append(text);
        for(const full of [false,true]){const fold=button(full?'Full activity':'Expand activity',full?'Expand the full public activity.':'Expand or collapse public activity.',()=>{n.activityFold=n.activityFold===(full?2:1)?0:full?2:1;renderDock(n);},'element-fold');fold.innerHTML='<svg viewBox="0 0 14 14" aria-hidden="true"><path d="M3 4l4 4 4-4'+(full?'M3 8l4 4 4-4':'')+'"/></svg>';summary.append(fold);}body.append(summary);
        if(n.activityFold){const activity=make('div','element-activity-output');activity.dataset.captureWheel='true';activity.style.maxHeight=n.activityFold===2?'480px':'96px';activity.setAttribute('aria-label','Public activity');for(const e of n.events){const row=make('div','element-event');row.append(make('span','',e.label),make('p','',e.detail));if(e.code)row.append(make('pre','',e.code));activity.append(row);}body.append(activity);StudioScroll.attach(activity,dock);requestAnimationFrame(()=>activity.scrollTop=activityScroll??activity.scrollHeight);}
      }
      const composer=make('textarea','element-composer image-prompt');composer.rows=1;composer.value=n.prompt;composer.placeholder=config.id==='dashboard-builder'?'Describe the dashboard or the next change':config.id==='text-agent'?(n.selection?'Ask about the selected text…':'Describe the text or the next change'):'Describe the model or the next change';composer.setAttribute('aria-label',config.title+' prompt');if(n.interactive)composer.dataset.captureWheel='true';composer.onpointerdown=e=>e.stopPropagation();
      function fitPrompt(){composer.style.height='0px';composer.style.height=Math.max(n.interactive?20:32,Math.min(n.interactive?100:128,composer.scrollHeight))+'px';n._visualHeight=n.height+52+dock.offsetHeight;}
      composer.oninput=()=>{n.prompt=composer.value;fitPrompt();save();};composer.onkeydown=e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();run(n);}if(e.key==='Escape')composer.blur();};body.append(composer);dock.append(body);root.append(dock);fitPrompt();
      requestAnimationFrame(()=>{if(!root.isConnected)return;if(transcript){transcript.scrollTop=reader??transcript.scrollHeight;StudioScroll.attach(transcript,dock);}if(focused){composer.focus({preventScroll:true});composer.setSelectionRange(...caret);}StudioScroll.attach(composer,dock);n._visualHeight=l.panelY+dock.offsetHeight;drawWires();});
    }
    function render(n){
      let root=nodeRoot(n);if(!root){root=make('section','element-node');root.dataset.elementNode=n.id;root.setAttribute('aria-label',config.title+' window');root.onpointerdown=e=>beginDrag(e,n);root.addEventListener('pointerdown',e=>{if(!n.selected&&e.button===0&&!e.target.closest('button'))beginDrag(e,n);},true);content.append(root);}
      root.replaceChildren();root.classList.toggle('is-selected',n.selected);Object.assign(root.style,{left:n.x+'px',top:n.y+'px',width:n.width+'px',height:n.height+'px'});
      const l=layoutFor(n);n.layout=l;
      const defsSvg=document.createElementNS('http://www.w3.org/2000/svg','svg');defsSvg.classList.add('element-mask');const clip=document.createElementNS(defsSvg.namespaceURI,'clipPath');clip.id=config.id+'-clip-'+n.id;clip.setAttribute('clipPathUnits','userSpaceOnUse');
      const path=document.createElementNS(defsSvg.namespaceURI,'path');let d=`M8 0H${n.width-8}Q${n.width} 0 ${n.width} 8V${n.height-8}Q${n.width} ${n.height} ${n.width-8} ${n.height}H8Q0 ${n.height} 0 ${n.height-8}V8Q0 0 8 0Z`;
      if(n.selected){for(const [i,port] of ImageStudioLayout.ports(n.width).entries())d+=ImageStudioLayout.notch(n.width,port[1],i===3);if(l.attached&&!n.open){const x=(n.width-l.bandWidth)/2,y=n.height;d+=`M${x} ${y}C${x+42} ${y} ${x+42} ${y-20} ${x+84} ${y-20}H${x+l.bandWidth-84}C${x+l.bandWidth-42} ${y-20} ${x+l.bandWidth-42} ${y} ${x+l.bandWidth} ${y}Z`;}}
      path.setAttribute('d',d);path.setAttribute('clip-rule','evenodd');clip.append(path);defsSvg.append(clip);root.append(defsSvg);
      const preview=make('div','element-preview');preview.style.clipPath=`url(#${clip.id})`;preview.style.height=n.height+'px';preview.append(config.renderPreview({node:n,make,update:opts=>update(n,opts),setSelection:q=>setSelection(n,q)}));root.append(preview);
      n.versions||=[M.copy(n.data)];
      if(n.selected){
        const portNames=['Primary input','Text input','Style input','Output'];ImageStudioLayout.ports(n.width).forEach((r,i)=>{const port=button(portNames[i],portNames[i],()=>{},'image-port port-'+i);port.textContent='';rect(port,[r[0]-6,r[1]-6,12,12]);if(i===3)port.onpointerdown=e=>beginDrag(e,n,'branch');root.append(port);});
        tools.forEach((tool,i)=>{const b=button(tool.label,tool.hint||tool.label,anchor=>{if(tool.id==='agent'){n.open=!n.open;update(n);return;}if(tool.choices){menu(anchor,()=>tool.choices.map(value=>({label:value,checked:n.data.display===value,action:()=>{tool.action(n,value);update(n);}})));return;}if(tool.action){tool.action(n);update(n);}else note.textContent=tool.label+' is unchanged in Slate; this is its placement reference.';},'image-tool '+(l.attached?'attached-tool':'detached-tool')+(tool.id==='agent'&&n.open?' active':''),tool.icon);rect(b,l.buttons[i]);root.append(b);});
      }
      if(n.preserve||n.parent){const out=button('Drag history to a new window','Carry model history downstream.',()=>{},'element-history-out');out.textContent='';out.onpointerdown=e=>beginDrag(e,n,'branch');root.append(out);}if(n.parent)root.append(make('i','element-history-in'));
      const resize=make('div','element-resize');resize.setAttribute('role','separator');resize.setAttribute('aria-label','Resize '+config.title+' window');resize.onpointerdown=e=>beginDrag(e,n,'resize');root.append(resize);renderDock(n);
    }
    function beginDrag(e,n,kind='move'){
      if(e.button!==0)return;if(kind==='move'&&(e.target.closest('button,textarea,input,select')||(n.selected&&e.target.closest('[contenteditable=true],.element-preview'))))return;
      e.preventDefault();e.stopPropagation();drag={node:n,kind,x:e.clientX,y:e.clientY,ox:n.x,oy:n.y,ow:n.width,moved:false};canvas.setPointerCapture(e.pointerId);
    }
    canvas.addEventListener('pointermove',e=>{if(!drag)return;const dx=(e.clientX-drag.x)/camera.zoom,dy=(e.clientY-drag.y)/camera.zoom;if(Math.hypot(e.clientX-drag.x,e.clientY-drag.y)>4)drag.moved=true;if(!drag.moved)return;const n=drag.node;if(drag.kind==='move'){n.x=drag.ox+dx;n.y=drag.oy+dy;nodeRoot(n).style.left=n.x+'px';nodeRoot(n).style.top=n.y+'px';drawWires();}else if(drag.kind==='resize'){n.width=Math.max(420,Math.min(1100,drag.ow+dx));render(n);drawWires();}});
    const endDrag=e=>{if(!drag)return;const d=drag;drag=null;if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);if(e.type==='pointercancel'){d.node.x=d.ox;d.node.y=d.oy;d.node.width=d.ow;render(d.node);}else if(d.kind==='branch'&&d.moved){const box=canvas.getBoundingClientRect(),drop={x:(e.clientX-box.left-camera.x)/camera.zoom,y:(e.clientY-box.top-camera.y)/camera.zoom};const next=fork(d.node);if(next){next.x=drop.x;next.y=drop.y;render(next);drawWires();fit();}}else if(d.kind==='move'){select(d.node);render(d.node);}save();};canvas.addEventListener('pointerup',endDrag);canvas.addEventListener('pointercancel',endDrag);
    function stop(n){clearInterval(timers.get(n.id));timers.delete(n.id);M.stop(n);update(n);}
    function run(source){
      if(source.running||!source.prompt.trim())return;
      if(source.selection&&source.selection.revision!==(source.data.textRevision||0)){note.textContent='The selected text changed. Select the passage again before sending.';return;}
      const prompt=source.prompt,selection=source.selection?M.copy(source.selection):null;
      let n=source;if(source.mode==='train'&&source.turns.some(t=>t.role==='user')){n=fork(source);source.prompt='';n.prompt=prompt;n.selection=selection;}
      if(!M.begin(n,prompt,selection))return;n.activityFold=1;
      n.startedAt=Date.now();update(n);
      const advance=()=>{
        n.stage++;const stage=config.stages[n.stage];
        if(stage){n.events.push({label:stage.label,detail:stage.detail,code:stage.code});if(stage.code)n.script+=(n.script?'\n\n':'')+stage.code;renderDock(n);save();return;}
        clearInterval(timers.get(n.id));timers.delete(n.id);config.applyResult(n,prompt);n.versions||=[];n.versions.push(M.copy(n.data));n.selectedRevision=n.versions.length-1;
        M.finish(n,config.id==='text-agent'?'The proposed revision is ready. Review it before applying it to the selected text.':config.id==='rhino-agent'?'The model preview and generated Rhino script are ready to inspect. This Studio has not executed commands in Rhino.':'The dashboard preview is ready. Inspect the changes, then apply or discard them.');
        n.events.push({label:'Complete',detail:'Preview finished. No external tool was called.'});update(n);requestAnimationFrame(fit);
      };
      advance();requestAnimationFrame(fit);timers.set(n.id,setInterval(advance,1000));
    }
    const nav=StudioCanvasNavigation.bind({canvas,camera,transform,closeMenus,deselect:()=>{nodes.forEach(n=>{n.selected=false;if(n.open){n.dismissedAt=Date.now();n.open=false;}render(n);});wire=null;drawWires();save();},context:e=>{const n=nodes.find(n=>n.id===e.target.closest('[data-element-node]')?.dataset.elementNode);if(n)menu({getBoundingClientRect:()=>({left:e.clientX,bottom:e.clientY})},()=>[{label:'Open agent',action:()=>{n.open=true;update(n);}},{label:'Continue in new window',action:()=>fork(n)},{label:'Fit canvas',action:fit}]);}});
    canvas.addEventListener('click',e=>{const n=nodes.find(n=>n.id===e.target.closest('[data-element-node]')?.dataset.elementNode);if(n&&!n.selected){select(n);if(Date.now()-(n.dismissedAt||0)<1500)n.open=true;render(n);}});
    window.addEventListener('keydown',e=>{if(canvas.hidden)return;if(e.key==='Escape'){wire=null;closeMenus();drawWires();}if(['Delete','Backspace'].includes(e.key)&&wire&&!e.target.closest('textarea,input,[contenteditable]')){e.preventDefault();if(!nodes.some(n=>n.parent===wire.id)){M.cut(wire);wire=null;save();renderAll();}}});
    function renderAll(){nodes.forEach(render);drawWires();}
    window.addEventListener('studio-element-changed',e=>{const visible=e.detail.id===config.id;canvas.hidden=!visible;footer.hidden=!visible;if(visible){renderAll();requestAnimationFrame(()=>fit(nodes[0]));}else closeMenus();});
    window.addEventListener('resize',()=>{if(!canvas.hidden)fit();});
    instances.set(config.id,{canvas,footer,renderAll});renderAll();transform();
  }
  for(const config of window.StudioElements||[])init(config);
  document.addEventListener('pointerdown',e=>{if(instances.has(StudioShell.activeId)&&!e.target.closest('.native-menu,.element-model,.element-more'))closeMenus();});
  const requested=new URLSearchParams(location.search).get('tab');if(instances.has(requested))StudioShell.activate(requested);
})();
