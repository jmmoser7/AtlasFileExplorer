/* Static browser canvas: native measurements, editable DOM/SVG. */

(() => {

  'use strict';

  const $=id=>document.getElementById(id),catalog=ImageAgentCatalog;

  const base=(id,label,x,y,width,height,extra={})=>({id,label,x,y,width,height,prompt:'',expanded:true,selected:true,config:catalog.normalize({}),rememberedPanel:'agent',aspect:0,count:1,seed:false,live:false,history:8,menu:'',...extra});

  const nodes=[

    base('small','Small image',90,70,220,260,{history:12}),

    base('medium','Medium image',980,70,360,360),

    base('large','Large image',1850,70,680,300),

    base('input-source','Source image',20,2060,300,210,{selected:false,expanded:false,generator:false,rememberedPanel:null}),

    base('style-one','Style image 1',390,1990,220,150,{selected:false,expanded:false,generator:false,rememberedPanel:null,history:0}),
    base('style-two','Style image 2',390,2250,220,150,{selected:false,expanded:false,generator:false,rememberedPanel:null,history:0}),
    base('input-result','Connected image',740,2070,680,300,{connected:true,prompt:'Keep the architecture; add lush planting and warm evening light.'}),

    base('series-1','First generation',20,4190,620,300,{prompt:'A timber pavilion in a planted courtyard.',history:3,contextOut:true}),

    base('series-2','Second generation',950,4190,620,300,{prompt:'Keep the same architecture. Move to late-afternoon light.',contextIn:true,contextOut:true}),

    base('series-3','Third generation',1880,4190,620,300,{prompt:'Continue from the previous image. Add a few people.',contextIn:true})

  ];

  const scenes={small:{x:-130,y:-10,w:720,h:660,title:'Small image',note:'220 × 260 · detached controls'},medium:{x:760,y:-10,w:760,h:730,title:'Medium image',note:'360 × 360 · detached controls'},large:{x:1760,y:-10,w:860,h:600,title:'Large image',note:'680 × 300 · joined image and generator'},inputs:{x:-60,y:1970,w:1600,h:720,title:'Image + text inputs',note:'Native primary-image and text ports; no duplicate input controls'},series:{x:-50,y:4090,w:2700,h:730,title:'A sequence of image generations',note:'Fourth wire carries chat history above the primary-image wire'}};

  let connections=[{from:'input-source',to:'input-result',kind:'image'},{from:'input-text',to:'input-result',kind:'text'},{from:'style-one',to:'input-result',kind:'style'},{from:'style-two',to:'input-result',kind:'style'},{from:'series-1',to:'series-2',kind:'image'},{from:'series-1',to:'series-2',kind:'history'},{from:'series-2',to:'series-3',kind:'image'},{from:'series-2',to:'series-3',kind:'history'}];

  const savedKey='slate-image-browser-studio-v4';

  try{const saved=JSON.parse(localStorage.getItem(savedKey)||'null');for(const extra of saved?.nodes||[])if((extra.id?.startsWith('copy-')||extra.id?.startsWith('branch-'))&&!nodes.some(n=>n.id===extra.id))nodes.push(base(extra.id,extra.label,extra.x,extra.y,extra.width,extra.height,{history:extra.id.startsWith('branch-')?1:0,generator:extra.id.startsWith('branch-'),expanded:false,rememberedPanel:null}));for(const node of nodes){const s=saved?.nodes?.find(n=>n.id===node.id);if(s)for(const field of ['x','y','width','height','prompt','selected','expanded','aspect','count','seed','live','config','customRatio','result','locked','rememberedPanel','selectedResults','selectionAnchor','bodySelected','historyEnabled','interactive','conversation','inheritedHistory','pendingHistory','historySource','historyConnected','sourceRevision','revision','contextIn','contextOut','history','bodySelected','blank','grafted','historyCut','resultJobs','historyArchive'])if(s[field]!==undefined)node[field]=s[field];}if(Array.isArray(saved?.connections)){connections=saved.connections;for(const id of ['style-one','style-two'])if(!connections.some(w=>w.from===id))connections.push({from:id,to:'input-result',kind:'style'});}}catch{}

  const save=()=>{try{localStorage.setItem(savedKey,JSON.stringify({nodes:nodes.map(({element,layout,url,hover,version,...n})=>n),connections}));}catch{}};

  const status=text=>{$('image-native-status').textContent=text;};

  const make=(tag,cls,text)=>{const e=document.createElement(tag);if(cls)e.className=cls;if(text!==undefined)e.textContent=text;return e;};

  const historyY=()=>10;

  let camera={x:0,y:0,zoom:1},drag=null,scene='small',statusTimer;

  const transient=text=>{clearTimeout(statusTimer);status(text);statusTimer=setTimeout(()=>status(''),4500);};

  let paintNode,selectedHistoryWire=null;

  const menus=createImageMenus({make,save,transient,repaint:node=>paintNode(node),historyActions:makeHistoryActions()});

  paintNode=createImageStudioView({make,catalog,save,transient,beginDrag,showHistoryMenu,drawWires,menus,selectNode,submitPrompt,inputCount:(n,k)=>connections.filter(w=>w.to===n.id&&w.kind===k).length,beginOutputDrag:(e,node)=>beginDrag(e,node,'output')});

  function selectNode(node,restore=true){
    const selecting=!node.selected||node.bodySelected===false;
    if(restore)node.bodySelected=true;else if(!node.selected)node.bodySelected=false;
    for(const other of nodes)if(other!==node&&(other.selected||other.expanded)){
      if(other.expanded){other.rememberedPanel='agent';other.dismissedAt=Date.now();}other.selected=false;other.bodySelected=false;other.expanded=false;other.activeTool=null;other.element.classList.remove('is-hovering');paintNode(other);
    }
    node.selected=true;
    if(restore&&selecting&&node.rememberedPanel==='agent'&&Date.now()-(node.dismissedAt||0)<=1500&&node.generator!==false){node.expanded=true;node.activeTool=6;}
    node.element.dataset.selected='true';
  }
  const request=node=>paintNode(node);

  const isVisible=()=>true;

  function showHistoryMenu(node,side,anchor){
    menus.close();const panel=make('div','native-menu'),box=anchor.getBoundingClientRect();panel.setAttribute('role','menu');panel.style.left=Math.min(innerWidth-240,Math.max(8,box.left))+'px';panel.style.top=Math.min(innerHeight-180,box.bottom+4)+'px';
    const wire=connections.find(w=>w.kind==='history'&&(side==='in'?w.to===node.id:w.from===node.id));
    function action(label,fn){const b=make('button','menu-row',label);b.setAttribute('role','menuitem');b.onclick=()=>{$('menus').replaceChildren();fn();};panel.append(b);}
    action('View carried history',()=>{const cover=make('div','modal-cover'),dialog=make('div','reference-dialog');dialog.setAttribute('role','dialog');dialog.setAttribute('aria-label','Carried conversation history');dialog.append(make('h2','','Carried conversation history'));const turns=ImageConversation.turns(node);if(!turns.length)dialog.append(make('p','','No messages carried yet.'));for(const turn of turns)dialog.append(make('p','',`${turn.role==='user'?'You':'Agent'}: ${turn.text}`));const close=make('button','','Close');close.onclick=()=>cover.remove();dialog.append(close);cover.append(dialog);$('dialogs').replaceChildren(cover);});
    if(wire){if(ImageConversation.canCut(wire,connections))action('Cut model history',()=>cutHistory(wire));if(side==='in')action('Refresh inherited history',()=>{const source=nodes.find(n=>n.id===wire.from);ImageConversation.connect(source,node);save();paintNode(node);});}
    else if(node.historySource)action('Reconnect chat history',()=>{const source=nodes.find(n=>n.id===node.historySource);if(source){ImageConversation.connect(source,node);connections.push({from:source.id,to:node.id,kind:'history'});save();nodes.forEach(paintNode);}});
    if(side==='out')action('New downstream image',()=>spawnDownstream(node));$('menus').replaceChildren(panel);
  }
  function spawnDownstream(source,position){
    const siblings=nodes.filter(n=>n.historySource===source.id).length;
    const node=base('branch-'+Date.now(),'Downstream image '+(siblings+1),position?.x??source.x+source.width+180,position?.y??source.y+siblings*620,source.width,source.height,{interactive:!!source.historyEnabled,historyEnabled:!!source.historyEnabled,contextIn:!!source.historyEnabled,contextOut:!!source.historyEnabled,history:0,conversation:[],revision:0,blank:true,expanded:false,selected:false,bodySelected:false,activeTool:null,rememberedPanel:null});
    menus.close();if(source.historyEnabled){ImageConversation.connect(source,node);source.contextOut=true;connections.push({from:source.id,to:node.id,kind:'history'});}nodes.push(node);mountNode(node);connections.push({from:source.id,to:node.id,kind:'image'});save();nodes.forEach(paintNode);
    const canvas=$('image-generator-canvas'),left=Math.min(source.x,node.x)-40,top=Math.min(source.y,node.y)-60,right=Math.max(source.x+source.width,node.x+node.width)+60,bottom=Math.max(source.y+source.height,node.y+node.height)+380;camera.zoom=Math.min(1,(canvas.clientWidth-60)/(right-left),(canvas.clientHeight-40)/(bottom-top));camera.x=30-left*camera.zoom;camera.y=20-top*camera.zoom;transform();
    return node;
  }
  function makeHistoryActions(){return {
    conversation(node){node.interactive=!node.interactive;node.conversation||=[];node.historyEnabled=true;node.contextOut=true;save();paintNode(node);},
    preserve(node){node.historyEnabled=!node.historyEnabled;node.interactive=node.historyEnabled;node.conversation||=[];save();paintNode(node);},
    branch:spawnDownstream,
    inspect(node,anchor){showHistoryMenu(node,node.contextIn?'in':'out',anchor);}
  };}
  function submitPrompt(node){const text=node.prompt.trim();if(!text)return;
    const inputs=Object.fromEntries(['image','text','style'].map(k=>[k,connections.filter(w=>w.to===node.id&&w.kind===k).map(w=>w.from)])),jobs=ImageGraft.jobs(inputs,node.grafted);
    if(!jobs.length){transient('A grafted input is empty. Connect an input before generating.');return;}
    if(node.historyEnabled){ImageConversation.submit(node,text);node.contextOut=true;node.interactive=true;}
    for(const child of nodes)if(child.historySource===node.id&&child.historyConnected)child.historyStale=true;
    node.resultJobs=jobs;node.history=(node.history||0)+jobs.length*node.count;node.result=node.history-1;node.blank=false;delete node.dialogueScroll;node.selectedResults=[node.result];if(node.interactive)node.prompt='';save();nodes.forEach(paintNode);transient(`Preview generated ${jobs.length*node.count} image(s); no provider was contacted.`);}

  function lineage(id,seen=new Set()){if(seen.has(id))return[];seen.add(id);const wire=connections.find(w=>w.kind==='history'&&w.to===id),source=wire&&nodes.find(n=>n.id===wire.from);return source?[...lineage(source.id,seen),source]:[];}

  function point(id,side,kind){

    if(id==='input-text')return [310,2480];const n=nodes.find(n=>n.id===id);if(!n)return[0,0];

    if(kind==='history')return[n.x+(side==='from'?n.width-6:6),n.y+historyY(n)];

    const sites=n.layout?.ports;if(sites?.length){const p=side==='from'?sites.at(-1):sites[kind==='text'?1:kind==='style'?2:0];if(p)return[n.x+p[0]/n.layout.zoom,n.y+p[1]/n.layout.zoom];}

    return[n.x+(side==='from'?n.width+ImageStudioLayout.portOffset:-ImageStudioLayout.portOffset),n.y+n.height*(side==='from'?.5:kind==='text'?.5:.25)];

  }

  function cutHistory(wire){
    if(!ImageConversation.cut(wire,nodes,connections)){transient('Only the history connection into the last image in a chain can be cut.');return;}
    connections=connections.filter(w=>w!==wire);selectedHistoryWire=null;save();nodes.forEach(paintNode);
  }
  function inputPoint(wire){
    const end=point(wire.to,'to',wire.kind),target=nodes.find(n=>n.id===wire.to);
    if(target?.grafted?.[wire.kind]){const peers=connections.filter(w=>w.to===wire.to&&w.kind===wire.kind);end[1]+=ImageGraft.port(peers.length,true).offset(peers.indexOf(wire));}
    return end;
  }

  function drawWires(){

    const snapshot=connections.slice(),paths=snapshot.map(w=>ImageStudioLayout.bezier(point(w.from,'from',w.kind),inputPoint(w)));

    const svg=$('image-wires');svg.replaceChildren();snapshot.forEach((wire,index)=>{const path=document.createElementNS('http://www.w3.org/2000/svg','path');path.setAttribute('d',paths[index]);if(wire.kind==='history')path.classList.add('history-wire');svg.append(path);if(wire.kind==='history'){const hit=path.cloneNode();hit.classList.add('wire-hit');hit.setAttribute('role','button');hit.setAttribute('tabindex','0');hit.setAttribute('aria-label',`Select history wire to ${wire.to}`);if(selectedHistoryWire?.from===wire.from&&selectedHistoryWire?.to===wire.to)path.classList.add('is-selected');hit.addEventListener('click',e=>{e.stopPropagation();document.activeElement?.blur();selectedHistoryWire=wire;drawWires();});svg.append(hit);}});

    for(const n of nodes)for(const port of n.element?.querySelectorAll('.image-context-port')||[]){port.classList.toggle('is-disconnected',port.dataset.direction==='in'&&!connections.some(w=>w.kind==='history'&&(port.dataset.direction==='in'?w.to===n.id:w.from===n.id)));port.dataset.hint=`${lineage(n.id).length} upstream prompt(s) carried through the history wire. Click to inspect or disconnect.`;}

  }

  function transform(){ $('image-world').style.transform=`translate(${camera.x}px,${camera.y}px) scale(${camera.zoom})`;$('image-zoom-reset').textContent=`${Math.round(camera.zoom*100)}%`; }

  function focus(name){scene=name;let s=scenes[name];const node=nodes.find(n=>n.id===name),canvas=$('image-generator-canvas');if(node){const panel=node.element.querySelector('.image-editor');s={x:node.x-Math.max(50,(420-node.width)/2+24),y:node.y-55,w:Math.max(node.width,420)+100,h:node.height+(panel?Number(panel.style.height.slice(0,-2))+65:110)+80};}camera.zoom=Math.min(1,(canvas.clientWidth-75)/s.w,(canvas.clientHeight-50)/s.h);camera.x=(canvas.clientWidth-s.w*camera.zoom)/2-s.x*camera.zoom;camera.y=(canvas.clientHeight-s.h*camera.zoom)/2-s.y*camera.zoom;for(const b of document.querySelectorAll('[data-image-scene]'))b.setAttribute('aria-current',String(b.dataset.imageScene===name));transform();}

  function beginDrag(e,node,kind){if(e.button!==0||node?.locked)return;if(e.target.closest('button,textarea')&&!['resize','output'].includes(kind))return;e.preventDefault();e.stopPropagation();if(kind==='output')$('image-generator-canvas').classList.add('is-output-drag');drag={kind,node,sx:e.clientX,sy:e.clientY,x:node?.x,y:node?.y,width:node?.width,moved:false,camera:{...camera}};$('image-generator-canvas').setPointerCapture(e.pointerId);}

  const hoverTimers=new Map();
  function mountNode(node){const root=make('section','native-image-node');root.dataset.nodeId=node.id;root.setAttribute('aria-label',node.label);node.element=root;root.addEventListener('pointerdown',e=>beginDrag(e,node,'move'));root.addEventListener('pointerenter',()=>{clearTimeout(hoverTimers.get(node.id));root.classList.add('is-hovering');});root.addEventListener('pointerleave',()=>{hoverTimers.set(node.id,setTimeout(()=>root.classList.remove('is-hovering'),180));});$('image-nodes').append(root);}
  nodes.forEach(mountNode);
  for(const [id,s] of Object.entries(scenes)){const label=make('div','image-scene-label');label.style.left=`${s.x+50}px`;label.style.top=`${s.y}px`;label.append(make('strong','',s.title),make('span','',s.note));$('image-scenes').append(label);}

  const text=make('div','native-input-text','Keep the architecture. Add lush planting, warm evening light, and a quiet place to sit.');text.style.left='20px';text.style.top='2430px';text.setAttribute('aria-label','Incoming text prompt');$('image-scenes').append(text);

  let copiedImage=null;
  function imageZip(files){
    const local=[],central=[];let offset=0;
    const header=(size,fields)=>{const b=new Uint8Array(size),v=new DataView(b.buffer);for(const [at,value,bytes] of fields)bytes===2?v.setUint16(at,value,true):v.setUint32(at,value,true);return b;};
    for(const file of files){const name=new TextEncoder().encode(file.name),data=file.data;let crc=0xffffffff;for(const byte of data){crc^=byte;for(let i=0;i<8;i++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}crc=(crc^0xffffffff)>>>0;
      const h=header(30,[[0,0x04034b50,4],[4,20,2],[6,0x800,2],[14,crc,4],[18,data.length,4],[22,data.length,4],[26,name.length,2]]);
      local.push(h,name,data);central.push(header(46,[[0,0x02014b50,4],[4,20,2],[6,20,2],[8,0x800,2],[16,crc,4],[20,data.length,4],[24,data.length,4],[28,name.length,2],[42,offset,4]]),name);offset+=h.length+name.length+data.length;
    }
    const size=central.reduce((n,b)=>n+b.length,0),end=header(22,[[0,0x06054b50,4],[8,files.length,2],[10,files.length,2],[12,size,4],[16,offset,4]]);return new Blob([...local,...central,end],{type:'application/zip'});
  }
  async function imageBlob(node,index){const image=new Image();image.src='image-generator/assets/pavilion.png';await image.decode();const surface=document.createElement('canvas');surface.width=node.width;surface.height=node.height;const ctx=surface.getContext('2d');ctx.filter=index%3===1?'sepia(.18) saturate(.8)':index%3===2?'saturate(1.25) contrast(1.03)':'none';ctx.drawImage(image,0,0,surface.width,surface.height);return new Promise(resolve=>surface.toBlob(resolve,'image/png'));}
  async function runImageAction(id,node,label){
    if(id.startsWith("custom:"))id=label||id;
    if(id.startsWith('input.graft.')){const kind=id.split('.').at(-1);node.grafted||={};node.grafted[kind]=!node.grafted[kind];save();paintNode(node);transient(`${kind==='image'?'Primary image':kind==='style'?'Style image':'Text'} graft ${node.grafted[kind]?'enabled':'disabled'}.`);return;}
    if(!node){transient('Select an image to try this action.');return;}
    const ids=node.selectedResults?.length?node.selectedResults:[node.result||0];
    if(/save/i.test(id)){
      const files=[];for(const index of ids){const blob=await imageBlob(node,index),number=Number(localStorage.getItem('slate-image-save-number')||0)+1;localStorage.setItem('slate-image-save-number',number);files.push({name:`image-${String(number).padStart(4,'0')}.png`,data:new Uint8Array(await blob.arrayBuffer())});}
      const a=document.createElement('a');a.href=URL.createObjectURL(files.length===1?new Blob([files[0].data],{type:'image/png'}):imageZip(files));a.download=files.length===1?files[0].name:`images-${files[0].name.slice(6,-4)}-${files.at(-1).name.slice(6,-4)}.zip`;a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000);transient(`Saved ${files.length} numbered image(s)${files.length>1?' in one ZIP':''}.`);return;
    }
    if(/copy/i.test(id)) {copiedImage={width:node.width,height:node.height,result:node.result||0};try{await navigator.clipboard.write([new ClipboardItem({'image/png':await imageBlob(node,node.result||0)})]);transient('Active image copied.');}catch{transient('Active image copied inside the Studio; browser image clipboard is unavailable.');}return;}
    if(/duplicate/i.test(id)){
      const n=base('copy-'+Date.now(),'Duplicated image',node.x+node.width+70,node.y,node.width,node.height,{history:0,generator:false,expanded:false,selected:false,rememberedPanel:null,result:node.result||0});n.element=make('section','native-image-node');n.element.dataset.nodeId=n.id;n.element.setAttribute('aria-label',n.label);n.element.onpointerdown=e=>beginDrag(e,n,'move');$('image-nodes').append(n.element);nodes.push(n);paintNode(n);save();transient('Duplicated only the active image as a plain canvas image.');return;
    }
    transient('This action is a review placeholder.');
  }
  function contextTarget(target){
    const input=target.closest('[data-input-kind]');if(input)return 'input-'+input.dataset.inputKind;
    if(target.closest('.model-name,.agent-picker'))return 'model';
    if(target.closest('[data-result-index]'))return 'result';
    if(target.closest('.image-prompt,.image-custom-ratio'))return 'prompt';
    if(target.closest('.image-port,.image-context-port,#image-wires'))return 'wire';
    if(target.closest('.image-count'))return 'count';
    if(target.closest('.image-more'))return 'options';
    if(target.closest('.image-tool,.image-editor,.image-action-menu'))return 'controls';
    if(target.closest('.native-image-node'))return 'body';
    if(target.closest('.native-input-text'))return 'input-text';
    return 'canvas';
  }
  function openRightClickStudio(e){let target=contextTarget(e.target);const root=e.target.closest('.native-image-node');let node=root?nodes.find(n=>n.id===root.dataset.nodeId):target==='input-text'?nodes.find(n=>n.id==='input-result'):null;const sourceLink=node?.generator===false&&connections.find(w=>w.from===node.id&&['image','text','style'].includes(w.kind));if(sourceLink){target='input-'+sourceLink.kind;node=nodes.find(n=>n.id===sourceLink.to);}const tile=e.target.closest('[data-result-index]');if(node&&tile&&!node.selectedResults?.includes(+tile.dataset.resultIndex)){node.result=+tile.dataset.resultIndex;node.selectedResults=[node.result];}menus.close();ImageMenuStudio.open(target,{node,run:runImageAction});}
  const canvas=$('image-generator-canvas');canvas.setAttribute('aria-description','Right-drag, middle-drag or Space plus left-drag to pan. Wheel to zoom. Shift plus wheel to pan horizontally.');

  const navigation=StudioCanvasNavigation.bind({canvas,camera,transform,closeMenus:()=>menus.close(),deselect:()=>{selectedHistoryWire=null;for(const node of nodes){if(node.expanded){node.rememberedPanel='agent';node.dismissedAt=Date.now();}node.selected=false;node.bodySelected=false;node.expanded=false;node.activeTool=null;node.element.classList.remove('is-hovering');paintNode(node);}save();},context:openRightClickStudio});

  // Suppress native/host context-menu bubbling before it reaches editable controls.
  // Right-drag remains owned by the navigation controller; stationary right-click
  // opens the Studio directly. The editor itself does not recursively reopen.
  let popupRightClick=false;
  window.addEventListener('pointerdown',e=>{if(StudioShell.activeId!=='image-generator'||e.button!==2||canvas.contains(e.target)||e.target.closest('.menu-studio-panel'))return;e.preventDefault();e.stopImmediatePropagation();popupRightClick=true;},true);
  window.addEventListener('pointerup',e=>{if(!popupRightClick||e.button!==2)return;popupRightClick=false;e.preventDefault();e.stopImmediatePropagation();openRightClickStudio(e);},true);
  window.addEventListener('contextmenu',e=>{if(StudioShell.activeId!=='image-generator')return;e.preventDefault();e.stopImmediatePropagation();if(e.target.closest('.menu-studio-panel'))return;if(canvas.contains(e.target))navigation.contextMenu(e);},true);
  canvas.addEventListener('pointermove',e=>{if(!drag)return;const dx=(e.clientX-drag.sx)/camera.zoom,dy=(e.clientY-drag.sy)/camera.zoom;if(Math.hypot(e.clientX-drag.sx,e.clientY-drag.sy)>4)drag.moved=true;if(!drag.moved)return;if(drag.kind==='output'){const box=canvas.getBoundingClientRect(),end=[(e.clientX-box.left-camera.x)/camera.zoom,(e.clientY-box.top-camera.y)/camera.zoom];let ghost=$('image-output-preview');if(!ghost){ghost=document.createElementNS('http://www.w3.org/2000/svg','path');ghost.id='image-output-preview';$('image-wires').append(ghost);}ghost.setAttribute('d',ImageStudioLayout.bezier(point(drag.node.id,'from','image'),end));drag.drop={x:end[0],y:end[1]-48};const port=document.elementFromPoint(e.clientX,e.clientY)?.closest('[data-input-kind]');drag.input=port?{to:port.closest('[data-node-id]').dataset.nodeId,kind:port.dataset.inputKind}:null;return;}if(drag.kind==='resize'){drag.node.width=Math.round(Math.max(120,Math.min(1000,drag.width+dx)));request(drag.node);}else{drag.node.x=drag.x+dx;drag.node.y=drag.y+dy;drag.node.element.style.left=`${drag.node.x}px`;drag.node.element.style.top=`${drag.node.y}px`;drawWires();}});

  const finish=e=>{if(!drag)return;const node=drag.node;if(drag.kind==='output'){canvas.classList.remove('is-output-drag');const drop=drag.drop,moved=e.type!=='pointercancel'&&drag.moved,input=drag.input;drag=null;$('image-output-preview')?.remove();if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);if(moved&&input&&input.to!==node.id){if(!connections.some(w=>w.from===node.id&&w.to===input.to&&w.kind===input.kind))connections.push({from:node.id,...input});save();nodes.forEach(paintNode);}else if(moved&&drop)spawnDownstream(node,drop);return;}if(!drag.moved){selectNode(node);if(node.generator===false)node.expanded=false;}drag=null;save();if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);request(node);};canvas.addEventListener('pointerup',finish);canvas.addEventListener('pointercancel',finish);

  const zoom=navigation.zoom;

  for(const b of document.querySelectorAll('[data-image-scene]'))b.addEventListener('click',()=>focus(b.dataset.imageScene));

  $('image-zoom-out').addEventListener('click',()=>zoom(camera.zoom/1.2));$('image-zoom-in').addEventListener('click',()=>zoom(camera.zoom*1.2));$('image-zoom-reset').addEventListener('click',()=>zoom(1));

  $('image-fit').addEventListener('click',()=>{const box=canvas.getBoundingClientRect(),left=Math.min(-180,...nodes.map(n=>n.x-200)),top=Math.min(-60,...nodes.map(n=>n.y-100)),right=Math.max(2700,...nodes.map(n=>n.x+n.width+200)),bottom=Math.max(...nodes.map(n=>n.y+n.height+350));camera.zoom=Math.min((box.width-70)/(right-left),(box.height-70)/(bottom-top),1);camera.x=(box.width-(right-left)*camera.zoom)/2-left*camera.zoom;camera.y=(box.height-(bottom-top)*camera.zoom)/2-top*camera.zoom;transform();});

  new MutationObserver(()=>nodes.filter(isVisible).forEach(request)).observe(document.documentElement,{attributes:true,attributeFilter:['data-theme']});

  window.addEventListener('resize',()=>{if(!canvas.hidden)focus(scene);});window.addEventListener('studio-element-changed',e=>{if(e.detail.id==='image-generator'){requestAnimationFrame(()=>focus(scene));}});

  window.addEventListener('keydown',e=>{if(StudioShell.activeId!=='image-generator'||!['Delete','Backspace'].includes(e.key)||e.target.closest('input,textarea,[contenteditable=true]')||!selectedHistoryWire)return;e.preventDefault();cutHistory(selectedHistoryWire);});
  window.addEventListener('keydown',e=>{if(StudioShell.activeId==='image-generator'&&e.key==='Escape'){selectedHistoryWire=null;drawWires();menus.close();document.activeElement?.blur();}});

  document.addEventListener('pointerdown',e=>{if(StudioShell.activeId==='image-generator'&&!e.target.closest('.native-menu,.image-menu-trigger,.model-name'))menus.close();});

  for(const node of nodes){if(node.interactive)node.historyEnabled=true;node.interactive=!!node.historyEnabled;}
  for(const node of nodes.filter(n=>n.id.startsWith('series-'))){if(!node.conversation){node.conversation=[{role:'user',text:node.prompt},{role:'assistant',text:'Preview response: I will retain the pavilion design while refining this image.'},{role:'user',text:'Keep the materials consistent and soften the light.'},{role:'assistant',text:'Preview response: The timber, proportions and setting remain in this conversation.'}];node.revision=2;if(node.id!=='series-3')node.interactive=true;}}
  for(const wire of connections.filter(w=>w.kind==='history')){const source=nodes.find(n=>n.id===wire.from),target=nodes.find(n=>n.id===wire.to);if(source&&target&&!target.historySource){ImageConversation.connect(source,target);if(target.pendingHistory){target.inheritedHistory=target.pendingHistory;delete target.pendingHistory;}}if(source){source.contextOut=true;if(target)target.historyStale=target.sourceRevision!==(source.revision||0);}if(target)target.contextIn=true;}
  for(const node of nodes)node.interactive=!!node.historyEnabled;
  StudioShell.activate(new URLSearchParams(location.search).get('tab')||'image-generator');nodes.forEach(paintNode);document.fonts.ready.then(()=>nodes.forEach(paintNode));focus('small');status('');

})();
