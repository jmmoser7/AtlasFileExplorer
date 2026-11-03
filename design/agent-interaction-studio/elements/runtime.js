/* One runtime owns shared chrome. Element modules supply only their content and preview stages. */
(() => {
  'use strict';
  const M=StudioSession,instances=new Map(),$=id=>document.getElementById(id);
  const make=(tag,cls,text)=>{const e=document.createElement(tag);if(cls)e.className=cls;if(text!=null)e.textContent=text;return e;};
  const load=(key,fallback)=>{try{return JSON.parse(localStorage.getItem(key))||fallback;}catch{return fallback;}};
  const prefKey='slate-element-agent-preferences-v1';
  const preferences=load(prefKey,{config:ChatAgentCatalog.normalize({provider:'codex',model:'Sol'}),access:'workspace'});
  function closeMenus(){StudioGeneratorUI.closeMenus();}
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
    const nodes=saved?.nodes?.length?saved.nodes:[M.create(config)],connections=saved?.connections||[];
    const inputKinds=['image','text','style'];
    const inputRows=n=>Object.fromEntries(inputKinds.map(kind=>[kind,[...(kind==='image'&&n.source?[n.source]:[]),...connections.filter(w=>w.to===n.id&&w.kind===kind).map(w=>w.from)]]));
    for(const n of nodes){if(n.inputKind){n.chromeVersion=2;n.width=n.inputKind==='text'?260:220;n.height=n.inputKind==='text'?130:160;continue;}if(n.chromeVersion!==2){n.chromeVersion=2;n.width=config.id==='text-agent'?480:680;n.height=config.id==='text-agent'?280:360;n.interactive=false;n.preserve=false;n.activity=false;n.activityFold=0;n.open=n===nodes[0];n.selected=n===nodes[0];n.data&&delete n.data.selectedWidget;}}
    const tools=config.propertyTools||(config.id==='text-agent'?[{id:'fill',label:'Fill',icon:'Fill'},{id:'stroke',label:'Stroke',icon:'Ellipse'},{id:'agent',label:'Agent',icon:'Agent'}]:[{id:'fill',label:'Fill color',icon:'Fill'},{id:'stroke',label:'Stroke color and width',icon:'Ellipse'},...(config.id==='text-agent'?[{id:'text-format',label:'Text formatting',icon:'Sliders'}]:[{id:'corners',label:'Corners and crop',icon:'Corners'}]),...(config.nativeTools||[]),{id:'agent',label:'Agent',icon:'Agent'}]);
    const rect=(el,r)=>Object.assign(el.style,{left:r[0]+'px',top:r[1]+'px',width:r[2]+'px',height:r[3]+'px'});
    const layoutFor=n=>ImageStudioLayout.layout(n.width,n.height,false,n.open,n.revision+1,120,tools.length);
    nodes.forEach(n=>{if(n.running){n.running=false;n.events.push({label:'Interrupted',detail:'The page closed before this preview finished.'});}});
    let camera=saved?.camera||{x:0,y:0,zoom:1},drag=null,wire=null,activePopup=null,menuReturn=null;
    const timers=new Map(),canvas=make('main','element-canvas'),world=make('div','element-world'),wires=document.createElementNS('http://www.w3.org/2000/svg','svg'),content=make('div','element-nodes');
    canvas.id=config.id+'-canvas';canvas.hidden=true;canvas.dataset.elementStudio=config.id;canvas.setAttribute('aria-label',config.title+' canvas');
    wires.classList.add('element-wires');world.append(wires,content);canvas.append(world);
    const footer=make('footer','studio-bottom element-footer');footer.hidden=true;footer.dataset.elementStudio=config.id;
    const note=make('span','','Interactive design preview · no live tools');
    const zoomLabel=button('100%','Reset canvas zoom',()=>nav.zoom(1));const actions=make('div');
    actions.append(button('−','Zoom out',()=>nav.zoom(camera.zoom/1.2)),zoomLabel,button('+','Zoom in',()=>nav.zoom(camera.zoom*1.2)),button('Fit','Fit this Studio canvas',fit));footer.append(note,actions);document.body.append(canvas,footer);
    const save=()=>{try{localStorage.setItem(storage,JSON.stringify({nodes,camera,connections}));}catch{note.textContent='Browser storage is full; keep this page open to retain changes.';}};
    const persistPreferences=()=>{localStorage.setItem(prefKey,JSON.stringify(preferences));};
    function transform(){world.style.transform=`translate(${camera.x}px,${camera.y}px) scale(${camera.zoom})`;zoomLabel.textContent=Math.round(camera.zoom*100)+'%';}
    function fit(target){
      const framed=target&&typeof target==='object'&&target.width?[target]:nodes;
      const left=Math.min(...framed.map(n=>n.x))-40,top=Math.min(...framed.map(n=>n.y))-35;
      const right=Math.max(...framed.map(n=>n.x+n.width))+40,bottom=Math.max(...framed.map(n=>n.y+(n._visualHeight||n.height+290)))+30;
      camera.zoom=Math.min(1,(canvas.clientWidth-35)/(right-left),(canvas.clientHeight-25)/(bottom-top));camera.x=(canvas.clientWidth-(right-left)*camera.zoom)/2-left*camera.zoom;camera.y=15-top*camera.zoom;transform();save();
    }
    function drawWires(){
      wires.replaceChildren();for(const connection of connections){const source=nodes.find(n=>n.id===connection.from),target=nodes.find(n=>n.id===connection.to);if(!source||!target)continue;const index=inputKinds.indexOf(connection.kind),group=connections.filter(w=>w.to===target.id&&w.kind===connection.kind),offset=ImageGraft.port(group.length,!!target.grafted?.[connection.kind]).offset(group.indexOf(connection));const path=document.createElementNS(wires.namespaceURI,'path');path.classList.add('element-data-wire');path.setAttribute('d',ImageStudioLayout.bezier([source.x+source.width+ImageStudioLayout.portOffset,source.y+48],[target.x-ImageStudioLayout.portOffset,target.y+48+index*42+offset]));wires.append(path);}for(const n of nodes){const upstream=nodes.find(p=>p.id===(n.source||n.parent));if(upstream){const data=document.createElementNS(wires.namespaceURI,'path');data.classList.add('element-data-wire');data.setAttribute('d',ImageStudioLayout.bezier([upstream.x+upstream.width+ImageStudioLayout.portOffset,upstream.y+48],[n.x-ImageStudioLayout.portOffset,n.y+48]));wires.append(data);}if(!n.parent)continue;const p=nodes.find(p=>p.id===n.parent);if(!p)continue;
        const path=document.createElementNS(wires.namespaceURI,'path'),a=[p.x+p.width-6,p.y+12],b=[n.x+6,n.y+12],dx=Math.max(60,(b[0]-a[0])*.5);
        path.setAttribute('d',`M${a} C${a[0]+dx},${a[1]} ${b[0]-dx},${b[1]} ${b}`);wires.append(path);
        const hit=path.cloneNode();hit.classList.add('wire-hit');hit.setAttribute('role','button');hit.setAttribute('aria-label','Select history connection to '+n.id);hit.setAttribute('tabindex','0');
        hit.onclick=e=>{e.stopPropagation();document.activeElement?.blur();wire=n;wires.querySelectorAll('.selected').forEach(p=>p.classList.remove('selected'));path.classList.add('selected');};wires.append(hit);
      }
    }
    const nodeRoot=n=>content.querySelector(`[data-element-node="${n.id}"]`);
    function select(n,body=true){for(const other of nodes){if(other!==n&&other.selected){other.selected=false;other.open=false;render(other);}}n.selected=true;n.bodySelected=body;nodeRoot(n)?.classList.add('is-selected');wire=null;}
    document.addEventListener('studio-outside-menu-dismiss',e=>{if(!canvas.hidden&&canvas.contains(e.detail.target)&&activePopup){menuReturn={...activePopup,at:Date.now()};activePopup=null;}});
    function rebound(n){if(menuReturn?.id!==n.id)return;const remembered=menuReturn;menuReturn=null;if(M.canRebound(remembered.at))nodeRoot(n)?.querySelector(remembered.kind==='model'?'.element-model':'.element-more')?.click();}
    function chooseResult(n,index,circle){select(n,circle);n.data=M.copy(n.versions[index]);n.selectedRevision=index;n.blank=false;if(!circle)n.open=false;update(n);}
    function miniature(n,data,index){const sample={...n,id:n.id+'-thumb-'+index,data:M.copy(data),selection:null};return config.renderThumbnail?config.renderThumbnail({node:sample,make}):config.renderPreview({node:sample,make,update:()=>{},setSelection:()=>{}});}
    function openAnswer(n,answer){select(n);n.open=true;n.focusAnswerId=answer.id;render(n);save();}
    function update(n,options){save();if(options?.render===false)return;render(n);drawWires();}
    function setSelection(n,selection){n.selection=selection;save();renderDock(n);}
    function addInput(kind,position){
      const n=M.create(config,'input-'+Date.now());Object.assign(n,{inputKind:kind,x:position.x,y:position.y,width:kind==='text'?260:220,height:kind==='text'?130:160,open:false,selected:false,preserve:false,data:{inputText:'Keep the composition and materials consistent.'}});nodes.push(n);render(n);save();
    }
    function inputContext(n,kind){ImageMenuStudio.open('input-'+kind,{node:n,run:id=>{if(!id.startsWith('input.graft.'))return;n.grafted||={};n.grafted[kind]=!n.grafted[kind];note.textContent=(n.grafted[kind]?'Grafted ':'Combined ')+kind+' inputs';update(n);}});}
    function fork(n){
      if(n.running)return;const next=n.inputKind?M.create(config,'node-'+Date.now()):M.branch(n,'node-'+Date.now());if(n.inputKind){Object.assign(next,{x:n.x+n.width+110,y:n.y,blank:true,open:false,selected:false});connections.push({from:n.id,to:next.id,kind:n.inputKind});}if(!n.inputKind)next.source=n.id;next.interactive=n.interactive;n.open=false;n.selected=false;nodes.push(next);render(n);render(next);drawWires();save();fit();return next;
    }
    function options(n,anchor){
      menu(anchor,()=>[
        {label:'Preserve model history',checked:!!n.preserve,stay:true,hint:'Keep the dialogue here; its history accompanies a downstream wire.',action:()=>{n.preserve=!n.preserve;n.interactive=n.preserve;update(n);}}
      ]);
    }
    function renderDock(n){
      const root=nodeRoot(n);if(!root)return;
      const previous=root.querySelector('.element-agent'),oldComposer=previous?.querySelector('textarea'),focused=oldComposer===document.activeElement,caret=focused?[oldComposer.selectionStart,oldComposer.selectionEnd]:null,oldTranscript=previous?.querySelector('.generator-transcript'),reader=oldTranscript?(oldTranscript.scrollTop+oldTranscript.clientHeight<oldTranscript.scrollHeight-3?oldTranscript.scrollTop:null):n.dialogueScroll;previous?.remove();
      const l=layoutFor(n);n.layout=l;
      if(!n.open||!n.selected){n._visualHeight=n.height+(l.attached?24:50);return;}
      const dock=make('section','element-agent image-editor'+(l.attached?'':' is-detached'));dock.setAttribute('aria-label',config.title+' agent');Object.assign(dock.style,{left:l.panelX+'px',top:l.panelY+'px',width:l.panelWidth+'px'});
      const header=make('header','element-agent-header'),model=ChatAgentCatalog.normalize(preferences.config),p=ChatAgentCatalog.providers.find(p=>p.id===model.provider),m=p.models.find(m=>m.id===model.model);
      const picker=button(p.label+' · '+m.label,'Provider, model, effort and speed; API key uses its own chevron.',b=>{if(document.querySelector('.agent-picker')){closeMenus();activePopup=null;return;}ChatAgentPicker.open({anchor:b,card:preferences.config,onChange:c=>{preferences.config=c;persistPreferences();renderDock(n);},onClose:()=>{},onStatus:t=>note.textContent=t});activePopup={id:n.id,kind:'model'};},'image-control image-field element-model');StudioGeneratorUI.modelLabel(picker,p.label+' · '+m.label,model,ChatAgentCatalog);rect(picker,l.controls.model);
      const more=button('Agent options','Preserve model history.',b=>{if(document.querySelector('.element-menu')){closeMenus();activePopup=null;}else{options(n,b);activePopup={id:n.id,kind:'options'};}},'image-control image-more element-more');more.innerHTML='<i></i><i></i><i></i>';rect(more,l.controls.more);
      const play=button(n.running?'Stop preview':'Send message',n.running?'Stop this preview run.':'Generate the next options.',()=>n.running?stop(n):run(n),'image-control generate-image element-play');play.textContent='';play.append(n.running?make('span','element-stop'):make('span','image-play'));rect(play,l.controls.play);
      header.append(picker,more,play);
      const count=StudioGeneratorUI.count(n.count||1,config.title,value=>{n.count=value;save();});rect(count,l.controls.count);header.append(count);
      const folded=n.detailFold??1,fold=make('div','fold-controls element-header-fold');fold.style.right='auto';fold.style.left=(l.controls.count[0]-22)+'px';
      const offers=folded===0?[['expand',1],['expand',2]]:folded===1?[['collapse',1],['expand',1]]:[['collapse',2],['collapse',1]];
      for(const [direction,step] of offers){const up=direction==='collapse',label=up?'Collapse':'Expand',hit=button(label+(step===2?' fully':' one step'),label+' dialogue.',()=>{n.detailFold=Math.max(0,Math.min(2,folded+(up?-step:step)));renderDock(n);save();},'fold-hit');const d=up?'M1.6 4.35L4 1.65L6.4 4.35':'M1.6 1.65L4 4.35L6.4 1.65';hit.innerHTML='<svg viewBox="0 0 8 7" aria-hidden="true"><path d="'+d+'"/>'+(step===2?'<path transform="translate(0 '+(up?2:-2)+')" d="'+d+'"/>':'')+'</svg>';fold.append(hit);}header.append(fold);
      const busy=make('span','element-busy');busy.hidden=!n.running;busy.setAttribute('role','status');busy.setAttribute('aria-label',n.events.at(-1)?.label||'Working');busy.title=n.events.at(-1)?.label||'Working';busy.style.left=(l.controls.more[0]+20)+'px';header.append(busy);
      n.versions||=[M.copy(n.data)];const start=l.controls.more[0]+38,width=Math.max(0,l.controls.count[0]-28-start),revisions=StudioResultPreviews.history({node:n,width,render:(data,index)=>miniature(n,data,index),onSelect:(index,circle)=>chooseResult(n,index,circle)});rect(revisions,[start,2,width,24]);revisions.addEventListener('page-results',()=>renderDock(n));header.append(revisions);dock.append(header);root.append(dock);
      let leading=null;if(n.selection?.text){leading=make('div','element-selection');leading.append(make('q','',n.selection.text),button('Clear text selection','Stop targeting this text.',()=>{n.selection=null;update(n);},'element-clear','close'));}
      const turns=n.focusAnswerId?n.turns.slice():n.preserve?M.contextTurns(n):[];
      if(n.running&&n.events.length){const event=n.events.at(-1);turns.push({role:'assistant',text:event.detail,code:event.code});}
      const shared=StudioGeneratorUI.conversation(dock,{interactive:!!n.preserve||!!n.running||!!n.focusAnswerId,turns,leading,prompt:n.prompt,label:config.title,placeholder:n.preserve?'Message…':config.id==='dashboard-builder'?'Describe the dashboard or the next change':config.id==='text-agent'?(n.selection?'Ask about the selected text…':'Describe the text or the next change'):'Describe the model or the next change',maxHeight:folded===0?0:folded===1?220:480,scrollTop:reader,onScroll:value=>{n.dialogueScroll=value;},onInput:value=>{n.prompt=value;save();},onSubmit:()=>run(n),onMeasure:height=>{n._visualHeight=l.panelY+height;drawWires();}});
      if(n.focusAnswerId){const target=shared.transcript?.querySelector('[data-turn-id="'+CSS.escape(n.focusAnswerId)+'"]');if(target){target.classList.add('answer-destination');shared.transcript.scrollTop=target.offsetTop;}}
      if(focused){shared.composer.focus({preventScroll:true});shared.composer.setSelectionRange(...caret);}
    }

    function render(n){
      let root=nodeRoot(n);if(!root){root=make('section','element-node');root.dataset.elementNode=n.id;root.setAttribute('aria-label',config.title+' window');root.onpointerdown=e=>beginDrag(e,n);root.onpointerenter=()=>{clearTimeout(root._hoverTimer);root.classList.add('is-hovering');};root.onpointerleave=()=>{root._hoverTimer=setTimeout(()=>root.classList.remove('is-hovering'),180);};root.addEventListener('pointerdown',e=>{if(!n.selected&&e.button===0&&!e.target.closest('button,a'))beginDrag(e,n);},true);content.append(root);}
      root.replaceChildren();root.classList.toggle('is-selected',n.selected);root.dataset.panelOpen=String(!!n.open&&!!n.selected);root.dataset.bodySelected=String(n.bodySelected!==false);Object.assign(root.style,{left:n.x+'px',top:n.y+'px',width:n.width+'px',height:n.height+'px'});
      if(n.inputKind){const preview=make('div','element-preview element-input-source');if(n.inputKind==='text'){const text=make('textarea','element-source-text');text.value=n.data.inputText;text.setAttribute('aria-label','Text input');text.oninput=()=>{n.data.inputText=text.value;save();};text.onpointerdown=e=>e.stopPropagation();preview.append(text);}else{const image=make('img');image.src='image-generator/assets/pavilion.png';image.alt=n.inputKind==='style'?'Style image input':'Primary image input';image.draggable=false;preview.append(image);}root.append(preview);const output=button('Output','Drag to connect this input.',()=>{},'image-port port-3');output.textContent='';rect(output,[n.width+ImageStudioLayout.portOffset-6,42,12,12]);output.onpointerdown=e=>beginDrag(e,n,'branch');root.append(output);return;}
      const l=layoutFor(n);n.layout=l;
      const defsSvg=document.createElementNS('http://www.w3.org/2000/svg','svg');defsSvg.classList.add('element-mask');const clip=document.createElementNS(defsSvg.namespaceURI,'clipPath');clip.id=config.id+'-clip-'+n.id;clip.setAttribute('clipPathUnits','userSpaceOnUse');
      const path=document.createElementNS(defsSvg.namespaceURI,'path');let d=`M8 0H${n.width-8}Q${n.width} 0 ${n.width} 8V${n.height-8}Q${n.width} ${n.height} ${n.width-8} ${n.height}H8Q0 ${n.height} 0 ${n.height-8}V8Q0 0 8 0Z`;
      if(n.selected&&n.bodySelected!==false){for(const [i,port] of ImageStudioLayout.ports(n.width).entries())if(i===3||!n.grafted?.[inputKinds[i]])d+=ImageStudioLayout.notch(n.width,port[1],i===3);if(l.attached&&!n.open){const x=(n.width-l.bandWidth)/2,y=n.height;d+=`M${x} ${y}C${x+42} ${y} ${x+42} ${y-20} ${x+84} ${y-20}H${x+l.bandWidth-84}C${x+l.bandWidth-42} ${y-20} ${x+l.bandWidth-42} ${y} ${x+l.bandWidth} ${y}Z`;}}
      path.setAttribute('d',d);path.setAttribute('clip-rule','evenodd');clip.append(path);defsSvg.append(clip);root.append(defsSvg);
      const preview=make('div','element-preview');preview.style.clipPath=`url(#${clip.id})`;preview.style.height=n.height+'px';if(!n.blank)preview.append(config.renderPreview({node:n,make,update:opts=>update(n,opts),setSelection:q=>setSelection(n,q),openAnswer:a=>openAnswer(n,a)}));root.append(preview);
      n.versions||=[M.copy(n.data)];
        const portNames=['Primary input','Text input','Style input','Output'];ImageStudioLayout.ports(n.width).forEach((r,i)=>{const port=button(portNames[i],portNames[i],()=>{},'image-port port-'+i);port.textContent='';const kind=inputKinds[i],geometry=ImageGraft.port(kind?inputRows(n)[kind].length:1,!!n.grafted?.[kind]);rect(port,[r[0]-6,r[1]-geometry.height/2,12,geometry.height]);if(kind){port.dataset.inputKind=kind;port.classList.toggle('is-grafted',!!n.grafted?.[kind]);}if(i===3)port.onpointerdown=e=>beginDrag(e,n,'branch');root.append(port);});
      if(n.selected&&n.bodySelected!==false){
        tools.forEach((tool,i)=>{const b=button(tool.label,tool.hint||tool.label,anchor=>{if(tool.id==='agent'){n.open=!n.open;update(n);return;}if(tool.choices){menu(anchor,()=>tool.choices.map(value=>({label:value,checked:n.data.display===value,action:()=>{tool.action(n,value);update(n);}})));return;}if(tool.action){tool.action(n);update(n);}else note.textContent=tool.label+' is unchanged in Slate; this is its placement reference.';},'image-tool '+(l.attached?'attached-tool':'detached-tool')+(tool.id==='agent'&&n.open?' active':''),tool.icon);rect(b,l.buttons[i]);root.append(b);});
      }
      const previews=StudioResultPreviews.grid({node:n,top:n.height+(n.selected&&n.bodySelected!==false?(l.attached?28:52):8),render:(data,index)=>miniature(n,data,index),onSelect:(index,circle)=>chooseResult(n,index,circle)});root.append(previews.group,previews.bridge);
      if(n.preserve||n.parent){const out=button('Drag history to a new window','Carry model history downstream.',()=>{},'element-history-out');out.textContent='';out.onpointerdown=e=>beginDrag(e,n,'branch');root.append(out);}if(n.parent)root.append(make('i','element-history-in'));
      const resize=make('div','element-resize');resize.setAttribute('role','separator');resize.setAttribute('aria-label','Resize '+config.title+' window');resize.onpointerdown=e=>beginDrag(e,n,'resize');root.append(resize);renderDock(n);
    }
    function beginDrag(e,n,kind='move'){
      if(e.button!==0)return;if(kind==='move'&&(e.target.closest('button,a,textarea,input,select')||(n.selected&&e.target.closest('[contenteditable=true],.element-preview'))))return;
      e.preventDefault();e.stopPropagation();if(kind==='branch')canvas.classList.add('is-output-drag');drag={node:n,kind,x:e.clientX,y:e.clientY,ox:n.x,oy:n.y,ow:n.width,moved:false};canvas.setPointerCapture(e.pointerId);
    }
    canvas.addEventListener('pointermove',e=>{if(!drag)return;const dx=(e.clientX-drag.x)/camera.zoom,dy=(e.clientY-drag.y)/camera.zoom;if(Math.hypot(e.clientX-drag.x,e.clientY-drag.y)>4)drag.moved=true;if(!drag.moved)return;const n=drag.node;if(drag.kind==='branch'){const box=canvas.getBoundingClientRect(),point=[(e.clientX-box.left-camera.x)/camera.zoom,(e.clientY-box.top-camera.y)/camera.zoom];let ghost=wires.querySelector('.element-wire-preview');if(!ghost){ghost=document.createElementNS(wires.namespaceURI,'path');ghost.classList.add('element-wire-preview');wires.append(ghost);}ghost.setAttribute('d',ImageStudioLayout.bezier([n.x+n.width+ImageStudioLayout.portOffset,n.y+48],point));const port=document.elementFromPoint(e.clientX,e.clientY)?.closest('[data-input-kind]');drag.target=port?{to:port.closest('[data-element-node]').dataset.elementNode,kind:port.dataset.inputKind}:null;}else if(drag.kind==='move'){n.x=drag.ox+dx;n.y=drag.oy+dy;nodeRoot(n).style.left=n.x+'px';nodeRoot(n).style.top=n.y+'px';drawWires();}else if(drag.kind==='resize'){n.width=Math.max(420,Math.min(1100,drag.ow+dx));render(n);drawWires();}});
    const endDrag=e=>{if(!drag)return;const d=drag;drag=null;canvas.classList.remove('is-output-drag');wires.querySelector('.element-wire-preview')?.remove();if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);if(e.type==='pointercancel'){d.node.x=d.ox;d.node.y=d.oy;d.node.width=d.ow;render(d.node);}else if(d.kind==='branch'&&d.moved&&d.target&&d.target.to!==d.node.id){if(!connections.some(w=>w.from===d.node.id&&w.to===d.target.to&&w.kind===d.target.kind))connections.push({from:d.node.id,...d.target});renderAll();}else if(d.kind==='branch'&&d.moved){const box=canvas.getBoundingClientRect(),drop={x:(e.clientX-box.left-camera.x)/camera.zoom,y:(e.clientY-box.top-camera.y)/camera.zoom};const next=fork(d.node);if(next){next.x=drop.x;next.y=drop.y;render(next);drawWires();fit();}}else if(d.kind==='move'){select(d.node);if(!d.moved&&M.canRebound(d.node.dismissedAt))d.node.open=true;render(d.node);if(!d.moved)rebound(d.node);}save();};canvas.addEventListener('pointerup',endDrag);canvas.addEventListener('pointercancel',endDrag);
    function stop(n){clearInterval(timers.get(n.id));timers.delete(n.id);M.stop(n);update(n);}
    function run(source){
      if(source.running||!source.prompt.trim())return;
      if(source.selection&&source.selection.revision!==(source.data.textRevision||0)){note.textContent='The selected text changed. Select the passage again before sending.';return;}
      const jobs=ImageGraft.jobs(inputRows(source),source.grafted);if(!jobs.length){note.textContent='Connect an input to the empty grafted port before generating.';return;}const prompt=source.prompt,selection=source.selection?M.copy(source.selection):null;
      const n=source;
      if(!n.preserve){n.inherited=[];n.contextStart=n.turns.length;}if(!M.begin(n,prompt,selection))return;n.runCount=n.count||1;n.runJobs=M.copy(jobs);n.detailFold=1;
      n.startedAt=Date.now();update(n);
      const advance=()=>{
        n.stage++;const stage=config.stages[n.stage];
        if(stage){n.events.push({label:stage.label,detail:stage.detail,code:stage.code});if(stage.code)n.script+=(n.script?'\n\n':'')+stage.code;renderDock(n);save();return;}
        clearInterval(timers.get(n.id));timers.delete(n.id);const options=n.runJobs.flatMap((job,branch)=>M.options(n,n.runCount,(candidate,index)=>{candidate.data.inputJob=M.copy(job);candidate.data.variant=(n.data.variant||0)+branch*n.runCount+index;config.applyResult(candidate,prompt);}));n.versions||=[];n.selectedRevision=n.versions.length;n.versions.push(...options);n.data=M.copy(options[0]);n.blank=false;
        M.finish(n,config.completionText?config.completionText(n):config.id==='rhino-agent'?'The model preview and generated Rhino script are ready to inspect. This Studio has not executed commands in Rhino.':'The dashboard preview is ready.');if(n.data.review?.answerId)n.turns.at(-1).id=n.data.review.answerId;
        if(!n.preserve)n.contextStart=n.turns.length;n.events.push({label:'Complete',detail:'Preview finished. No external tool was called.'});update(n);requestAnimationFrame(fit);
      };
      advance();requestAnimationFrame(fit);timers.set(n.id,setInterval(advance,1000));
    }
    const nav=StudioCanvasNavigation.bind({canvas,camera,transform,closeMenus,deselect:()=>{nodes.forEach(n=>{n.selected=false;if(n.open){n.dismissedAt=Date.now();n.open=false;}render(n);});wire=null;drawWires();save();},context:e=>{const n=nodes.find(n=>n.id===e.target.closest('[data-element-node]')?.dataset.elementNode),kind=e.target.closest('[data-input-kind]')?.dataset.inputKind;if(n&&kind){inputContext(n,kind);return;}if(n?.inputKind){const receiver=connections.find(w=>w.from===n.id);if(receiver)inputContext(nodes.find(target=>target.id===receiver.to),receiver.kind);return;}if(!n){const rect=canvas.getBoundingClientRect(),position={x:(e.clientX-rect.left-camera.x)/camera.zoom,y:(e.clientY-rect.top-camera.y)/camera.zoom};menu({getBoundingClientRect:()=>({left:e.clientX,bottom:e.clientY})},()=>inputKinds.map(kind=>({label:'Add '+(kind==='image'?'image':kind==='style'?'style image':'text')+' input',action:()=>addInput(kind,position)})));}else ImageMenuStudio.open(e.target.closest('[data-result-index]')?'result':'body',{node:n,resultIndex:Number(e.target.closest('[data-result-index]')?.dataset.resultIndex||0)});}});
    canvas.addEventListener('click',e=>{const n=nodes.find(n=>n.id===e.target.closest('[data-element-node]')?.dataset.elementNode);if(n&&e.target.closest('.element-preview')){if(!n.selected||n.bodySelected===false){select(n);if(M.canRebound(n.dismissedAt))n.open=true;render(n);}rebound(n);}});
    window.addEventListener('keydown',e=>{if(canvas.hidden)return;if(e.key==='Escape'){wire=null;closeMenus();drawWires();}if(['Delete','Backspace'].includes(e.key)&&wire&&!e.target.closest('textarea,input,[contenteditable]')){e.preventDefault();if(!nodes.some(n=>(n.source||n.parent)===wire.id)){M.cut(wire);wire=null;save();renderAll();}}});
    function renderAll(){nodes.forEach(render);drawWires();}
    window.addEventListener('studio-element-changed',e=>{const visible=e.detail.id===config.id;canvas.hidden=!visible;footer.hidden=!visible;if(visible){renderAll();requestAnimationFrame(()=>fit(nodes[0]));}else closeMenus();});
    window.addEventListener('resize',()=>{if(!canvas.hidden)fit();});
    instances.set(config.id,{canvas,footer,renderAll});renderAll();transform();
  }
  for(const config of window.StudioElements||[])init(config);
  // Shared generator-ui capture handler owns outside-click dismissal.
  const requested=new URLSearchParams(location.search).get('tab');if(instances.has(requested))StudioShell.activate(requested);
})();
