/* Disposable Frame design interpreter. Production owners remain read-only. */
(() => {
'use strict';
const B=window.FrameBaseline,R=window.FrameRenderer,$=id=>document.getElementById(id),copy=x=>JSON.parse(JSON.stringify(x));
const sample=new URLSearchParams(location.search).has('sample'),storage='slate-frame-studio-v1';
const frame=(id='frame-1',rect={x:0,y:0,w:612,h:792},extra={})=>({id,rect,rotation_deg:0,title:'Slide 1',order:0,fill:[255,255,255,255],fill_authored:false,corner:{kind:'rounded',radius:8},stroke:{width:0,color:[0,0,0,255]},tags:[],...extra});
const Decks = window.FrameDeck;
function presentationFixture(count) {
  const titles = ['Opening', 'Site plan', 'Materials', 'Landscape', 'Next steps'];
  const colors = [[54,105,102,255],[119,144,117,255],[162,112,79,255],[78,119,148,255],[133,108,148,255]];
  const columns = count === 5 ? 3 : 10;
  const frames = [], contents = [];
  for (let index = 0; index < count; index++) {
    const slot = index * (count === 5 ? 3 : 37) % count;
    const x = (slot % columns) * 742, y = Math.floor(slot / columns) * 455;
    const title = titles[index] || 'Slide ' + (index + 1);
    const tint = colors[index % colors.length];
    frames.push(frame('frame-' + (index + 1), {x,y,w:640,h:360}, {title,order:index}));
    contents.push({
      id: 'content-' + (index*3+1), kind: 'shape', fill: tint,
      rect: {x:x+30,y:y+29,w:580,h:246}, radius: 14
    }, {
      id: 'content-' + (index*3+2), kind: 'shape', fill: [242,245,233,205],
      rect: {x:x+70+index%4*59,y:y+68,w:240,h:144}, radius: index%3 ? 12 : 65
    }, {
      id: 'content-' + (index*3+3), kind: 'text', text: title, fontSize: 19,
      rect: {x:x+30,y:y+292,w:580,h:28}
    });
  }
  const frameIds = frames.map(item => item.id);
  if (count === 5) {
    frames.push(frame('frame-6', {x:1484,y:455,w:640,h:360}, {title:'Discussion',order:5}));
    contents.push({id:'content-16',kind:'text',text:'Discussion',fontSize:28,rect:{x:1520,y:504,w:520,h:45}});
  }
  return {name: 'Presentation · ' + count + ' slides', frames, contents, presentations: [
    {id:'presentation-1', name:'Project review', frameIds},
    {id:'presentation-2', name:'Client walkthrough', frameIds:frameIds.slice(0,3).reverse()}
  ]};
}
const fixtures={
baseline:{name:'Native datum',frames:[frame()],contents:[]},
wide:{name:'16:9',frames:[frame('frame-1',{x:0,y:0,w:960,h:540})],contents:[]},
rotated:{name:'Rotated 27°',frames:[frame('frame-1',{x:0,y:0,w:612,h:792},{rotation_deg:27})],contents:[]},
'quarter-turn':{name:'Quarter turn',frames:[frame('frame-1',{x:0,y:0,w:612,h:792},{rotation_deg:90})],contents:[]},
tiny:{name:'Small frame',frames:[frame('frame-1',{x:0,y:0,w:24,h:18})],contents:[]},
overlap:{name:'Overlap',frames:[frame(),frame('frame-2',{x:340,y:160,w:612,h:792},{title:'Slide 2',order:1})],contents:[{id:'content-1',rect:{x:420,y:300,w:120,h:80},kind:'shape',rotation_deg:0,fill:[193,215,225,255]}]},
multiple:{name:'Multiple slides',frames:[0,1,2].map(i=>frame('frame-'+(i+1),{x:i*760,y:0,w:612,h:792},{title:'Slide '+(i+1),order:i})),contents:[]},
'presentation-small':presentationFixture(5),
'presentation-large':presentationFixture(100),
content:{name:'With content',frames:[frame('frame-1',{x:0,y:0,w:960,h:540})],contents:[{id:'content-1',rect:{x:80,y:80,w:260,h:180},kind:'shape',rotation_deg:0,fill:[198,218,222,255]},{id:'content-2',rect:{x:390,y:100,w:340,h:70},kind:'text',rotation_deg:0,text:'Content stays on the canvas',fill:[27,30,34,255]}]},
'presentation-media':{name:'Presentation media · fixture',frames:[0,1].map(i=>frame('frame-'+(i+1),{x:i*1120,y:0,w:960,h:540},{title:'Media slide '+(i+1),order:i})),contents:[0,1].flatMap(i=>[['video','Video · local fixture'],['web','Webpage · fixture bookmark'],['model','3D · fixture interaction']].map(([kind,text],j)=>({id:'media-'+i+'-'+kind,rect:{x:i*1120+40+j*300,y:120,w:270,h:230},kind:'fixture-'+kind,text,rotation_deg:0,fill:[231,237,239,255],autoplay:i===0,looped:i===0})))},
authored:{name:'Authored style',frames:[frame('frame-1',{x:0,y:0,w:960,h:540},{fill:[230,239,234,255],fill_authored:true,corner:{kind:'rounded',radius:32},stroke:{width:2,color:[15,118,110,255]},tags:['Review']})],contents:[]}
};
let state,fixture='baseline',revision=0,sequence=3,undoList=[],redoList=[],drag=null,popup=null,hover=null,raf=0,creation=null;
const camera={x:0,y:0,zoom:.8};
let mediaSession=FramePresentationSettings.session();
let numberEditor=null,numberPress=null,propertyPress=null,panels=null,presentationDrawer=null;
const canvas=document.createElement('main');canvas.id='frame-canvas';canvas.dataset.elementStudio='frame';canvas.hidden=true;canvas.setAttribute('aria-label','Frame design canvas');canvas.innerHTML='<svg id="frame-svg" aria-label="Slate Frame specimen"></svg><div id="frame-message" role="status" aria-live="polite"></div>';document.body.append(canvas);
const svg=$('frame-svg'),footer=document.createElement('footer');footer.id='frame-studio-controls';footer.className='frame-studio-controls';footer.hidden=true;footer.dataset.elementStudio='frame';
footer.innerHTML='<div class="frame-review-label"><strong>Frames</strong><span id="frame-baseline-status">Source-measured datum · draft</span></div><label><span class="sr-only">Frame specimen</span><select id="frame-fixture"></select></label><button id="frame-inspect" aria-expanded="false">Inspect</button><button id="frame-metrics">Metrics</button><button id="frame-duplicate" title="Duplicate selected Frame (Ctrl+D)">Duplicate</button><button id="frame-undo">Undo</button><button id="frame-redo">Redo</button><div class="frame-zoom"><button id="frame-zoom-out" aria-label="Zoom out">−</button><button id="frame-zoom-reset">100%</button><button id="frame-zoom-in" aria-label="Zoom in">+</button><button id="frame-fit">Fit</button><button id="frame-reset">Reset datum</button></div>';document.body.append(footer);
const mediaFeedback=document.createElement('div');mediaFeedback.id='frame-media-feedback';mediaFeedback.hidden=true;mediaFeedback.innerHTML='<p>Local fixture simulation · no video decoding, external webpages or model engine.</p><span id=frame-media-state></span><button id=frame-fixture-video>Play / pause video</button><button id=frame-fixture-web>Move fixture map view</button><button id=frame-fixture-model>Rotate fixture model</button>';footer.append(mediaFeedback);
function mediaFor(f){const video=state.contents.find(n=>n.kind==='fixture-video'&&B.membersOf(state,f.id).includes(n.id));return{video:video?{autoplay:video.autoplay,looped:video.looped}:{},web:{view:{url:'fixture://map',center:[40.7,-74],zoom:12}}};}
function syncMediaFeedback(){const m=mediaSession.current;mediaFeedback.hidden=fixture!=='presentation-media'||!state.presentationId||canvas.hidden;if(mediaFeedback.hidden)return;$('frame-media-state').textContent='Video '+(m.video.playing?'playing':'paused')+' · loop '+(m.video.loop?'on':'off')+' · view '+JSON.stringify(m.web.view)+' · 3D '+(m.model.interactive?'interactive':'locked')+' / yaw '+m.model.yaw;$('frame-fixture-model').disabled=!m.model.interactive;}
$('frame-fixture-video').onclick=()=>{if(mediaSession.current)mediaSession.current.video.playing=!mediaSession.current.video.playing;paint();};
$('frame-fixture-web').onclick=()=>{const view=mediaSession.current?.web.view;if(view)view.center[0]+=0.1;paint();};
$('frame-fixture-model').onclick=()=>{const model=mediaSession.current?.model;if(model?.interactive)model.yaw+=15;paint();};
footer.setAttribute('role','toolbar');footer.setAttribute('aria-label','Frame Studio controls');
const syncFrameChrome=()=>{if(footer.hidden)return;const top=Math.ceil(footer.getBoundingClientRect().bottom);document.documentElement.style.setProperty('--frame-canvas-top',top+'px');requestAnimationFrame(paint);};
new ResizeObserver(syncFrameChrome).observe(footer);
window.addEventListener('studio-element-changed',e=>{if(e.detail.id==='frame')syncFrameChrome();});
const inspector=document.createElement('aside');inspector.id='frame-inspector';inspector.hidden=true;inspector.setAttribute('aria-label','Frame Studio inspector');
inspector.innerHTML='<header><strong>Frame Studio</strong><button id="frame-inspector-close" aria-label="Close inspector">×</button></header><p>Draft specification. Changes here are Studio experiments. Reset restores the native datum.</p><form id="frame-property-form"><fieldset><legend>Selected frame</legend><label>Title<input name="title" maxlength="160"></label><div class="frame-field-pair"><label>Width<input name="w" type="number" min="8" step="any"></label><label>Height<input name="h" type="number" min="8" step="any"></label></div><div class="frame-field-pair"><label>Rotation °<input name="rotation_deg" type="number" step="any"></label><label>Radius<input name="radius" type="number" min="0" step="any"></label></div><div class="frame-field-pair"><label>Fill<input name="fill" type="color"></label><label>Stroke<input name="stroke" type="color"></label></div><label>Stroke width<input name="strokeWidth" type="number" min="0" step="any"></label><label class="frame-check"><input name="themeFill" type="checkbox">Follow theme fill</label><button type="submit">Apply experiment</button></fieldset></form><details><summary>Creation gestures</summary><p>Arm a preset, click or drag on the canvas. Shift draws square in the current native code. Escape cancels.</p><div class="frame-preset-buttons"><button data-preset="letter">Letter</button><button data-preset="tabloid">Tabloid</button><button data-preset="wide">16:9</button></div></details><details><summary>Measurements and acceptance</summary><p>Native screenshot parity is pending. Compact Fill, Stroke, and Corners panels are interactive. Native paired screenshot acceptance remains pending.</p><a href="frame/ACCEPTANCE.md" target="_blank">Acceptance protocol</a><a href="frame/source-baseline.json" target="_blank">Source fingerprints</a><pre id="frame-measurements"></pre></details><label>Design notes<textarea id="frame-review-notes" rows="4" placeholder="Record the next change to explore…"></textarea></label><div class="frame-export-actions"><button id="frame-export">Export experiment</button><button id="frame-export-notes">Export notes</button></div>';document.body.append(inspector);
for(const [id,value] of Object.entries(fixtures)){const o=document.createElement('option');o.value=id;o.textContent=value.name;$('frame-fixture').append(o);}
const theme=()=>document.documentElement.dataset.theme==='dark'?'dark':'light',selected=()=>state.frames.find(f=>state.selectedIds.includes(f.id)),say=s=>$('frame-message').textContent=s;
const persist=()=>{if(!sample)try{localStorage.setItem(storage,JSON.stringify({state,fixture,undoList,redoList,notes:$('frame-review-notes').value}));}catch{}};
function interactionMetrics(){const f=selected(),kind=state.propertyPanel?.kind;return{schema:'slate-frame-interaction-metrics/v1',selectedIds:copy(state.selectedIds),undoDepth:undoList.length,redoDepth:redoList.length,deck:{active:!!state.deckMode,id:state.activePresentationId,order:Decks.slides(state).map(f=>f.id)},presentationId:state.presentationId,panel:kind?{kind,hostId:f?.id,geometry:R.propertyGeometry(f,state,camera)}:null,propertyTokens:{toolbarOrder:copy(R.actions),closedWidth:R.closedWidth,button:30,gap:7,objectGap:14,panelWidth:420,fillHeight:174,strokeHeight:190,cornersHeight:22.1,colorField:[5,5,396,70],rails:{x:12,y:96,pitch:16,width:396,height:7},popupGap:8,popupMargin:8,blister:{height:40,shoulder:84,closedWidth:R.blisterWidths.closed,openWidth:R.blisterWidths.open,inset:42,icon:31.2},transitionSeconds:.12,duplicateOffset:[24,24]}};}
function metrics(){return {schema:'slate-frame-studio-measurements/v1',fixture,revision,theme:theme(),camera:copy(camera),viewport:{width:canvas.clientWidth,height:canvas.clientHeight,dpr:devicePixelRatio},nativePixelParity:'unverified',interaction:interactionMetrics(),canonical:B.metricReport(state,camera,theme()),frames:state.frames.map(f=>({id:f.id,title:f.title,order:f.order,tags:copy(f.tags),locked:!!f.locked,rect:copy(f.rect),rotation_deg:f.rotation_deg,corner:copy(f.corner),fill:copy(f.fill),fill_authored:f.fill_authored,stroke:copy(f.stroke),corners:B.corners(f.rect,f.rotation_deg),upperEdge:B.upperEdge(f.rect,f.rotation_deg),geometry:B.selectionGeometry(f.rect,f.rotation_deg,camera.zoom),members:members([f.id])})),tokens:copy(B.tokens)};}
const hex=c=>'#'+c.slice(0,3).map(v=>Math.round(v).toString(16).padStart(2,'0')).join(''),rgba=s=>(s.slice(1).match(/../g)||[]).map(s=>parseInt(s,16)).concat(255);
function updateInspector(){
const f=selected(),form=$('frame-property-form');form.querySelector('fieldset').disabled=!f;
if(f&&!document.activeElement?.closest('#frame-property-form')){const values={title:f.title,w:f.rect.w,h:f.rect.h,rotation_deg:f.rotation_deg,radius:f.corner.radius||0,fill:hex(f.fill),stroke:hex(f.stroke.color),strokeWidth:f.stroke.width};for(const [k,v] of Object.entries(values))form.elements[k].value=v;form.elements.themeFill.checked=!f.fill_authored;}
if(!inspector.hidden)$('frame-measurements').textContent=JSON.stringify(metrics(),null,2);
}
function paint(){syncMediaFeedback();state.propertyViewport={width:canvas.clientWidth,height:canvas.clientHeight};R.render({svg,state,camera,theme:theme()});panels?.sync();presentationDrawer?.sync();syncNumberEditor();$('frame-duplicate').disabled=!state.selectedIds.length;$('frame-zoom-reset').textContent=Math.round(camera.zoom*100)+'%';$('frame-undo').disabled=!undoList.length;$('frame-redo').disabled=!redoList.length;$('frame-baseline-status').textContent=undoList.length?'Studio experiment · draft':'Source-measured datum · draft';updateInspector();}
const snapshot=()=>copy(state),durable=s=>({frames:s.frames,contents:s.contents,presentations:s.presentations});
function restoreHistory(s){state.frames=copy(s.frames);state.contents=copy(s.contents);state.presentations=s.presentations&&copy(s.presentations);state.activePresentationId=s.activePresentationId;Decks.ensure(state);delete state.deckStroke;state.selectedIds=state.selectedIds.filter(id=>[...state.frames,...state.contents].some(n=>n.id===id));if(!state.frames.some(frame=>frame.id===state.presentationHostId))closePresentationDrawer();}
function commit(before,label){if(JSON.stringify(durable(before))!==JSON.stringify(durable(state))){undoList.push({state:before,label});redoList=[];revision++;persist();}paint();}
function transact(label,fn){const before=snapshot();fn();commit(before,label);}
function undo(){panels?.close(true);if(drag)cancelDrag();const entry=undoList.pop();if(!entry)return;redoList.push({state:snapshot(),label:entry.label});restoreHistory(entry.state);revision++;persist();paint();say('Undid '+entry.label);}
function redo(){panels?.close(true);const entry=redoList.pop();if(!entry)return;undoList.push({state:snapshot(),label:entry.label});restoreHistory(entry.state);revision++;persist();paint();}
function resetState(id){mediaSession=FramePresentationSettings.session();fixture=fixtures[id]?id:'baseline';const f=fixtures[fixture];state={frames:copy(f.frames),contents:copy(f.contents),presentations:f.presentations&&copy(f.presentations),selectedIds:f.presentations?[f.frames[0].id]:[],hoverId:null,hoverProgress:0,deckMode:false,presentationId:null};Decks.ensure(state);undoList=[];redoList=[];drag=null;creation=null;hover=null;sequence=Math.max(state.frames.length,state.contents.length)+1;revision++;$('frame-fixture').value=fixture;}
function fit(frames=state.frames){const points=frames.flatMap(f=>B.corners(f.rect,f.rotation_deg));if(!points.length)return;const xs=points.map(p=>p[0]),ys=points.map(p=>p[1]),x=Math.min(...xs),y=Math.min(...ys),w=Math.max(...xs)-x,h=Math.max(...ys)-y,cw=canvas.clientWidth||innerWidth,ch=canvas.clientHeight||innerHeight-98;camera.zoom=Math.max(1/128,Math.min(.8,(cw-180)/Math.max(1,w),(ch-150)/Math.max(1,h)));camera.x=(cw-w*camera.zoom)/2-x*camera.zoom;camera.y=(ch-h*camera.zoom)/2-y*camera.zoom+4;paint();}
function fitSpecimen() {
  const visibleSample = fixture === 'presentation-large'
    ? state.frames.slice(0,1)
    : state.frames;
  fit(visibleSample.length ? visibleSample : state.frames);
}
function closePopup(){popup?.remove();popup=null;}
function loadFixture(id){panels?.close(false);finishNumber(false);closePopup();resetState(id);fitSpecimen();persist();return snapshot();}
function inspect(open=true){inspector.hidden=!open;$('frame-inspect').setAttribute('aria-expanded',String(open));updateInspector();}
const local=e=>{const r=canvas.getBoundingClientRect();return [e.clientX-r.left,e.clientY-r.top];},point=e=>{const p=local(e);return [(p[0]-camera.x)/camera.zoom,(p[1]-camera.y)/camera.zoom];};
const frameAt=p=>{const id=B.frameAt(state.frames,p[0],p[1]);return state.frames.find(f=>f.id===id);};
const ownerOf=c=>frameAt([c.rect.x+c.rect.w/2,c.rect.y+c.rect.h/2])?.id,members=ids=>state.contents.filter(c=>ids.includes(ownerOf(c))).map(c=>c.id),carried=ids=>state.contents.filter(c=>ids.includes(ownerOf(c))&&!c.hidden&&!c.locked&&c.kind!=='connector').map(c=>c.id);
function removeSelected(){transact('delete frame',()=>{state.frames=state.frames.filter(f=>!state.selectedIds.includes(f.id));state.contents=state.contents.filter(c=>!state.selectedIds.includes(c.id));for(const presentation of state.presentations) presentation.frameIds=presentation.frameIds.filter(id=>!state.selectedIds.includes(id));if(state.selectedIds.includes(state.presentationHostId))closePresentationDrawer();state.selectedIds=[];});}
function duplicate(){transact('duplicate frame',()=>{const ids=[...state.selectedIds],newIds=[];for(const original of [...state.frames,...state.contents].filter(n=>ids.includes(n.id))){const n=copy(original);n.id=(state.frames.includes(original)?'frame-':'content-')+sequence++;n.rect.x+=24;n.rect.y+=24;(state.frames.includes(original)?state.frames:state.contents).push(n);newIds.push(n.id);}state.selectedIds=newIds;});}
function stageGestureCopy(action,id){
const sourceIds=action==='body'?[...state.selectedIds,...carried(state.selectedIds)]:[id],newIds=[],map=new Map();
for(const n of [...state.frames,...state.contents].filter(n=>sourceIds.includes(n.id))){const duplicate=copy(n),isFrame=state.frames.includes(n);duplicate.id=(isFrame?'frame-':'content-')+sequence++;map.set(n.id,duplicate.id);(isFrame?state.frames:state.contents).push(duplicate);if(state.selectedIds.includes(n.id))newIds.push(duplicate.id);}
state.selectedIds=newIds;return {id:map.get(id)||id,members:state.contents.filter(c=>[...map.values()].includes(c.id)).map(c=>c.id)};
}
const ordered=()=>[...state.frames].sort((a,b)=>a.order-b.order||a.id.localeCompare(b.id,undefined,{numeric:true}));
function orderStep(dir){panels?.close(true);const f=selected(),all=ordered(),other=all[all.indexOf(f)+dir];if(other)transact('reorder frame',()=>{[f.order,other.order]=[other.order,f.order];});}
function closePresentationDrawer() {
  state.presentationHostId = null;
  state.deckMode = false;
  delete state.deckHoverId;
  presentationDrawer?.closeMenu();
}
function removePresentationFrames(ids) {
  transact('remove presentation frames', () => Decks.remove(state, ids));
}
function togglePresentationDrawer(id) {
  if (state.presentationHostId === id) { closePresentationDrawer(); paint(); return; }
  panels?.close(true);
  finishNumber(true);
  closePopup();
  if (!Decks.active(state).frameIds.includes(id)) {
    const owner = state.presentations.find(presentation => presentation.frameIds.includes(id));
    if (owner) state.activePresentationId = owner.id;
  }
  state.selectedIds = [id];
  state.presentationHostId = id;
  presentationDrawer.sync({reveal:true});
  paint();
}
function sortPresentationFrames() {
  const frames = Decks.slides(state);
  if (frames.length < 2 || frames.some(frame => frame.locked)) return;
  const bounds = frames.map(frame => R.propertyGeometry(frame, state, camera).bounds);
  const x = Math.min(...bounds.map(rect => rect.x)), y = Math.min(...bounds.map(rect => rect.y));
  const pitchX = Math.max(...bounds.map(rect => rect.w)) + 56;
  const pitchY = Math.max(...bounds.map(rect => rect.h)) + 56;
  const columns = Math.ceil(Math.sqrt(frames.length));
  const contentByFrame = presentationMembers();
  transact('sort presentation frames on canvas', () => {
    frames.forEach((frame, index) => {
      const dx = x + index%columns*pitchX - bounds[index].x;
      const dy = y + Math.floor(index/columns)*pitchY - bounds[index].y;
      const content = (contentByFrame.get(frame.id) || []).filter(item => !item.hidden && !item.locked && item.kind !== 'connector');
      for (const item of [frame, ...content]) {
        item.rect.x += dx;
        item.rect.y += dy;
      }
    });
  });
}
function presentationMembers() {
  const byFrame = new Map();
  for (const item of state.contents) {
    if (item.hidden) continue;
    const owner = ownerOf(item);
    if (!owner) continue;
    if (!byFrame.has(owner)) byFrame.set(owner, []);
    byFrame.get(owner).push(item);
  }
  return byFrame;
}
function present(id) {
  panels?.close(true);
  finishNumber(true);
  const slides = Decks.slides(state);
  if (!slides.length) { say('Add frames to this presentation first'); return; }
  const target = slides.find(frame => frame.id === id) || slides[0];
  closePresentationDrawer();
  document.activeElement?.blur();
  state.presentationId = target.id;
  mediaSession = FramePresentationSettings.enter(mediaSession, target, mediaFor(target));
  closePopup();
  paint();
  say('Presentation preview · arrows change slide · Escape returns');
}
function nextSlide(dir){const all=Decks.slides(state),i=all.findIndex(f=>f.id===state.presentationId);const next=all[Math.max(0,Math.min(all.length-1,i+dir))];if(next&&next.id!==state.presentationId){mediaSession=FramePresentationSettings.enter(mediaSession,next,mediaFor(next));state.presentationId=next.id;}paint();}
const menuEntries=['board.shape.edit','sep:properties','board.to_front','board.to_back','sep:stack','board.lock','sep:delete','board.copy','board.duplicate','board.delete'];
const menuActions={
'board.shape.edit':['Object properties','settings',''],'board.to_front':['Bring to front','front',''],'board.to_back':['Send to back','back',''],'board.lock':['Lock','lock','Ctrl+L'],'board.copy':['Copy','copy','Ctrl+C'],'board.duplicate':['Duplicate','copy','Ctrl+D'],'board.delete':['Delete','trash','Del']};
let clipboard=null;
function copySelection(){clipboard={frames:copy(state.frames.filter(f=>state.selectedIds.includes(f.id))),contents:copy(state.contents.filter(c=>state.selectedIds.includes(c.id)))};say('Copied to the Studio clipboard');}
function pasteSelection(){if(!clipboard)return;transact('paste frames',()=>{const ids=[];for(const f of clipboard.frames){const n=copy(f);n.id='frame-'+sequence++;n.rect.x+=24;n.rect.y+=24;state.frames.push(n);ids.push(n.id);}for(const c of clipboard.contents){const n=copy(c);n.id='content-'+sequence++;n.rect.x+=24;n.rect.y+=24;state.contents.push(n);}state.selectedIds=ids;});}
function runMenu(action){
if(action==='board.shape.edit')panels.toggle('fill');
else if(action==='board.duplicate')duplicate();
else if(action==='board.delete')removeSelected();
else if(action==='board.copy')copySelection();
else if(action==='board.lock')transact('toggle lock',()=>{const f=selected();if(f)f.locked=!f.locked;});
else if(action==='board.to_front'||action==='board.to_back')transact('change stack',()=>{const picked=state.frames.filter(f=>state.selectedIds.includes(f.id)),rest=state.frames.filter(f=>!state.selectedIds.includes(f.id));state.frames=action==='board.to_front'?[...rest,...picked]:[...picked,...rest];});
}
function showFrameContext(e, name, rows) {
  popup = document.createElement('div');
  popup.className = 'frame-context native-menu image-action-menu';
  popup.setAttribute('role','menu');
  popup.setAttribute('aria-label', name);
  popup.style.width = '256px';
  for (const row of rows) {
    if (!row) {
      const rule = document.createElement('div');
      rule.className = 'menu-rule';
      popup.append(rule);
      continue;
    }
    const button = document.createElement('button');
    button.className = 'menu-row';
    button.setAttribute('role','menuitem');
    button.dataset.command = row.key;
    const glyph = document.createElement('span');
    glyph.className = 'menu-glyph';
    if (window.ImageMenuGlyph) glyph.innerHTML = window.ImageMenuGlyph(row.icon);
    const label = document.createElement('span');
    label.textContent = row.label;
    const shortcut = document.createElement('span');
    shortcut.className = 'frame-menu-shortcut';
    shortcut.textContent = row.shortcut || '';
    button.append(glyph,label,shortcut);
    if (row.key === 'board.delete') button.classList.add('danger');
    button.onclick = () => { closePopup(); row.run(); };
    popup.append(button);
  }
  document.body.append(popup);
  popup.style.left = Math.max(8,Math.min(innerWidth-popup.offsetWidth-8,e.clientX))+'px';
  popup.style.top = Math.max(48,Math.min(innerHeight-popup.offsetHeight-8,e.clientY))+'px';
}
function openContext(e) {
  if (canvas.hidden || drag || state.presentationId) return;
  closePopup();
  const tile = e.target?.closest?.('[data-slide-id]');
  if (tile) {
    showFrameContext(e, 'Slide menu', [{
      key:'presentation.remove', label:'Remove frame from presentation', icon:'minus', shortcut:'Del',
      run:() => { removePresentationFrames([tile.dataset.slideId]); presentationDrawer.focusSelector(); }
    }]);
    return;
  }
  if (e.target?.closest?.('.frame-presentation-drawer')) return;
  const id = e.target?.closest?.('[data-frame-id]')?.dataset.frameId || frameAt(point(e))?.id;
  const clicked = state.frames.find(frame => frame.id === id && !frame.hidden);
  if (clicked && !state.presentationHostId && !state.selectedIds.includes(id)) state.selectedIds = [id];
  const target = clicked || selected();
  if (!target) return;
  const rows = menuEntries.map(key => {
    if (key.startsWith('sep:')) return null;
    const [label,icon,shortcut] = menuActions[key];
    return {key, label:key==='board.lock'&&target.locked?'Unlock':label, icon, shortcut, run:() => {
      if (clicked && !state.selectedIds.includes(id)) {
        closePresentationDrawer();
        state.selectedIds = [id];
      }
      if (key === 'board.shape.edit') closePresentationDrawer();
      runMenu(key);
      paint();
    }};
  });
  if (clicked) {
    const included = Decks.active(state).frameIds.includes(id);
    rows.unshift({
      key:'presentation.membership', label:included?'Remove frame from presentation':'Add frame to presentation',
      icon:included?'minus':'plus',
      run:() => transact(included?'remove presentation frame':'add presentation frame', () => Decks.toggle(state,id))
    }, null);
  }
  showFrameContext(e,'Frame menu',rows);
  paint();
}
const nav=window.StudioCanvasNavigation.bind({canvas,camera,transform:paint,closeMenus(){closePopup();presentationDrawer?.closeMenu();},deselect(){if(!creation&&!state.deckMode){closePresentationDrawer();state.selectedIds=[];paint();}},context:openContext});

function syncNumberEditor(){
  if(!numberEditor)return;
  const edit=state.numberEdit,f=edit&&state.frames.find(f=>f.id===edit.frameId);
  if(!f||!state.selectedIds.includes(f.id)||canvas.hidden){finishNumber(false);return;}
  const externalStrip=!R.propertyGeometry(f,state,camera).eligible;
  const d=R.dimensionGeometry(f,edit.kind,camera,edit.text,externalStrip),form=numberEditor.form,input=numberEditor.input;
  form.style.left=d.screen.numericCenter[0]+'px';form.style.top=d.screen.numericCenter[1]+'px';
  form.style.width=Math.max(.01,d.numericWidth)+'px';form.style.height=d.textHeight+'px';
  form.style.transform='translate(-50%,-50%) rotate('+d.angle+'deg) scale('+camera.zoom+')';
  form.style.setProperty('--frame-number-caret',B.tokens.themes[theme()].select);
  form.style.setProperty('--frame-number-selection',theme()==='dark'?'#2b5c8a':'#d7e8ff');
  input.style.width=Math.max(.01,d.numericWidth)+'px';input.style.height=d.textHeight+'px';
  input.style.lineHeight=d.textHeight+'px';input.setAttribute('aria-invalid',String(edit.error));
}
function finishNumber(apply){
  if(!numberEditor)return true;
  const edit=state.numberEdit,f=edit&&state.frames.find(f=>f.id===edit.frameId);
  let value;
  if(apply){
    const text=edit.text.trim(),syntax=/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i;
    value=syntax.test(text)?Math.fround(Number(text)):NaN;
    if(!Number.isFinite(value)||value<1.1754943508222875e-38||value>3.4028234663852886e38){
      edit.error=true;paint();numberEditor.input.focus();numberEditor.input.select();return false;
    }
  }
  const form=numberEditor.form;numberEditor=null;delete state.numberEdit;form.remove();
  if(apply&&f&&!f.locked){
    const axis=edit.kind==='width'?'w':'h',origin=axis==='w'?'x':'y';
    if(f.rect[axis]!==value)transact('edit '+edit.kind,()=>{f.rect[origin]-=(value-f.rect[axis])/2;f.rect[axis]=value;});
    else paint();
  }else paint();
  return true;
}
function editNumber(kind,frameId){
  const f=state.frames.find(f=>f.id===frameId);if(!f||f.locked)return;
  if(numberEditor&&!finishNumber(true))return;
  closePopup();state.numberEdit={frameId:f.id,kind,text:String(f.rect[kind==='width'?'w':'h']),error:false};
  const form=document.createElement('form'),input=document.createElement('input');
  form.className='frame-number-editor element-node';form.setAttribute('aria-label','Edit Frame '+kind);
  input.type='text';input.inputMode='decimal';input.spellcheck=false;input.autocomplete='off';input.setAttribute('aria-label','Frame '+kind);input.value=state.numberEdit.text;
  form.append(input);canvas.append(form);numberEditor={form,input};
  form.addEventListener('submit',e=>{e.preventDefault();finishNumber(true);});
  input.addEventListener('input',()=>{if(!state.numberEdit)return;state.numberEdit.text=input.value;state.numberEdit.error=false;paint();});
  input.addEventListener('keydown',e=>{if(e.key==='Escape'){e.preventDefault();e.stopPropagation();finishNumber(false);}else if(e.key==='Enter'){e.preventDefault();e.stopPropagation();finishNumber(true);}});
  paint();input.focus({preventScroll:true});input.select();
}
document.addEventListener('pointerdown',e=>{
  if(!numberEditor||numberEditor.form.contains(e.target))return;
  const hit=e.target.closest('.frame-number-hit');
  if(hit&&hit.dataset.frameId===state.numberEdit.frameId&&hit.dataset.action===state.numberEdit.kind){numberEditor.input.focus({preventScroll:true});return;}
  if(!finishNumber(true)){e.preventDefault();e.stopImmediatePropagation();}
},true);

function hoverTo(id){if(state.frames.find(f=>f.id===id)?.locked)id=null;if(id===hover)return;hover=id;if(id)state.hoverId=id;cancelAnimationFrame(raf);let previous=performance.now();const tick=now=>{const dt=(now-previous)/1000;previous=now;const target=hover?1:0,duration=target ? .12 : .22;state.hoverProgress+=Math.sign(target-state.hoverProgress)*Math.min(Math.abs(target-state.hoverProgress),dt/duration);if(!state.hoverProgress)state.hoverId=null;paint();if(Math.abs(target-state.hoverProgress)>.0001)raf=requestAnimationFrame(tick);};raf=requestAnimationFrame(tick);}
canvas.addEventListener('pointerdown',e=>{
if(e.button!==0||state.presentationId||e.target.closest('.frame-number-editor,#frame-property-host,.frame-presentation-drawer'))return;if(!e.target.closest('input,textarea,[contenteditable=true]'))document.activeElement?.blur();const target=(document.elementFromPoint(e.clientX,e.clientY)||e.target).closest('[data-action]'),action=target?.dataset.action;let id=target?.closest('[data-frame-id]')?.dataset.frameId;
if(state.deckMode&&!target?.closest('.frame-property-strip')) {
  const hitId = id || frameAt(point(e))?.id;
  drag={action:'deck',before:snapshot(),pointerId:e.pointerId,screen:local(e),points:[point(e)],hitId,moved:false};
  canvas.setPointerCapture(e.pointerId);
  return;
}
if(state.presentationHostId && action!=='deck') closePresentationDrawer();
if(creation&&!target){drag={action:'create',before:snapshot(),pointerId:e.pointerId,p:point(e),screen:local(e),moved:false};canvas.setPointerCapture(e.pointerId);return;}
if(['body','resize','rotate','fillet'].includes(action)){
e.preventDefault();const before=snapshot(),f=state.frames.find(f=>f.id===id);if(!f||f.locked)return;
if((e.ctrlKey||e.shiftKey)&&state.selectedIds.includes(id)&&action==='body'){state.selectedIds=state.selectedIds.filter(i=>i!==id);paint();return;}
if(!state.selectedIds.includes(id))state.selectedIds=(e.ctrlKey||e.shiftKey)?[...state.selectedIds,id]:[id];
const contentsMarquee=action==='body'&&f.rect.w*camera.zoom>=canvas.clientWidth&&f.rect.h*camera.zoom>=canvas.clientHeight;
let staged=null;if(!contentsMarquee&&e.altKey&&['body','resize'].includes(action)){staged=stageGestureCopy(action,id);id=staged.id;}
const p=point(e),ids=[...state.selectedIds];drag={action:contentsMarquee?'marquee':action,id,handle:target.dataset.handle,before,pointerId:e.pointerId,p,screen:local(e),moved:false,frames:copy(state.frames),contents:copy(state.contents),ids,stagedCopy:!!staged,memberIds:staged?staged.members:carried(ids),startAngle:Math.atan2(p[1]-f.rect.y-f.rect.h/2,p[0]-f.rect.x-f.rect.w/2)*180/Math.PI};canvas.setPointerCapture(e.pointerId);paint();return;}
if(!target){paint();return;}e.preventDefault();if(action==='prev')orderStep(-1);else if(action==='next')orderStep(1);else if(action==='present')present(id);else if(action==='deck')togglePresentationDrawer(id);else if(['width','height'].includes(action)){if(numberEditor&&state.numberEdit.frameId===id&&state.numberEdit.kind===action){numberEditor.input.focus();return;}numberPress={pointerId:e.pointerId,frameId:id,kind:action,start:local(e)};canvas.setPointerCapture(e.pointerId);}else if(['fill','stroke','corners','settings'].includes(action)){finishNumber(true);propertyPress={pointerId:e.pointerId,kind:action,start:local(e)};canvas.setPointerCapture(e.pointerId);paint();}
});
canvas.addEventListener('pointermove',e=>{
if(numberPress||propertyPress)return;
if(!drag){const t=e.target.closest('[data-action]'),id=t?.closest('[data-frame-id]')?.dataset.frameId,hot=t?.dataset.action;
 if(state.deckMode){
  const frameId = !e.target.closest('.frame-presentation-drawer') && (id || (hot==='content' ? frameAt(point(e))?.id : null));
  if((frameId||null)!==(state.deckHoverId||null)){state.deckHoverId=frameId||null;paint();}
  return;
 }
 const next=hot==='fillet'?'fillet':null,changed=state.hotAction!==next;state.hotAction=next;hoverTo(hot==='body'&&!state.selectedIds.includes(id)?id:null);if(changed)paint();return;}
const s=local(e);if(Math.hypot(s[0]-drag.screen[0],s[1]-drag.screen[1])>(['create','deck'].includes(drag.action)?4:6))drag.moved=true;if(!drag.moved)return;const p=point(e),delta=[p[0]-(drag.p?.[0]||0),p[1]-(drag.p?.[1]||0)];
if(drag.action==='deck'){drag.points.push(p);state.deckStroke=drag.points;paint();return;}
if(drag.action==='marquee'){state.marquee={x:Math.min(p[0],drag.p[0]),y:Math.min(p[1],drag.p[1]),w:Math.abs(p[0]-drag.p[0]),h:Math.abs(p[1]-drag.p[1])};paint();return;}
if(drag.action==='create'){
const sizes=creation==='tabloid'?[1224,792]:creation==='wide'?[960,540]:[612,792],ratio=sizes[0]/sizes[1];let rect;
if(e.shiftKey){const side=Math.max(Math.abs(delta[0]),Math.abs(delta[1]));rect={x:Math.min(drag.p[0],p[0]),y:Math.min(drag.p[1],p[1]),w:side,h:side};}
else {const w=Math.max(8,Math.abs(delta[0])),h=Math.max(8,w/ratio);rect={x:delta[0]<0?drag.p[0]-w:drag.p[0],y:delta[1]<0?drag.p[1]-h:drag.p[1],w,h};}
state.frames=copy(drag.before.frames);state.frames.push(frame('frame-draft',rect,{order:Math.max(-1,...state.frames.map(f=>f.order))+1,title:'Slide '+sequence}));state.selectedIds=['frame-draft'];paint();return;
}
state.frames=copy(drag.frames);state.contents=copy(drag.contents);const f=state.frames.find(f=>f.id===drag.id),original=drag.frames.find(f=>f.id===drag.id);
if(drag.action==='body'){for(const n of [...state.frames.filter(f=>drag.ids.includes(f.id)),...state.contents.filter(c=>drag.memberIds.includes(c.id))]){n.rect.x+=delta[0];n.rect.y+=delta[1];}}
else if(drag.action==='resize')f.rect=B.resizeRect(original.rect,original.rotation_deg,drag.handle,delta,{shift:e.shiftKey,ctrl:e.ctrlKey,alt:e.altKey});
else if(drag.action==='rotate'){const a=Math.atan2(p[1]-original.rect.y-original.rect.h/2,p[0]-original.rect.x-original.rect.w/2)*180/Math.PI;f.rotation_deg=B.snapRotation(original.rotation_deg+a-drag.startAngle,e.altKey);}
else if(drag.action==='fillet'){const t=original.rotation_deg*Math.PI/180;f.corner.radius=Math.max(0,Math.min(Math.min(f.rect.w,f.rect.h)/2,(original.corner.radius||0)+delta[0]*Math.cos(t)+delta[1]*Math.sin(t)));}
paint();
});
function cancelDrag(){if(!drag)return;const d=drag;drag=null;state=d.before;if(canvas.hasPointerCapture(d.pointerId))canvas.releasePointerCapture(d.pointerId);paint();say('Gesture cancelled');}
function endDrag(e){if(propertyPress&&propertyPress.pointerId===e.pointerId){const n=propertyPress;propertyPress=null;if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);const p=local(e);if(e.type!=='pointercancel'&&Math.hypot(p[0]-n.start[0],p[1]-n.start[1])<=6)panels.toggle(n.kind);return;}if(numberPress&&numberPress.pointerId===e.pointerId){const n=numberPress;numberPress=null;if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);const p=local(e);if(e.type!=='pointercancel'&&Math.hypot(p[0]-n.start[0],p[1]-n.start[1])<=6)editNumber(n.kind,n.frameId);return;}if(!drag||drag.pointerId!==e.pointerId)return;if(e.type==='pointercancel'){cancelDrag();return;}const d=drag;drag=null;if(d.stagedCopy&&!d.moved){state=d.before;if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);paint();return;}
if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);
if(d.action==='deck'){
  if (!d.moved && !d.hitId) {
    closePresentationDrawer();
    state.selectedIds = [];
    paint();
    return;
  }
  const included = Decks.active(state).frameIds.includes(d.hitId);
  if (d.moved) Decks.append(state, Decks.along(state.frames,[...d.points,point(e)]));
  else if (d.hitId) Decks.toggle(state, d.hitId);
  delete state.deckStroke;
  commit(d.before,!d.moved&&included?'remove presentation frame':'add presentation frames');
  return;
}
if(d.action==='marquee'){const m=state.marquee;state.selectedIds=m?state.contents.filter(c=>ownerOf(c)===d.id&&c.rect.x+c.rect.w/2>=m.x&&c.rect.x+c.rect.w/2<=m.x+m.w&&c.rect.y+c.rect.h/2>=m.y&&c.rect.y+c.rect.h/2<=m.y+m.h).map(c=>c.id):[];delete state.marquee;paint();return;}
if(d.action==='create'){if(!d.moved){const size=creation==='tabloid'?[1224,792]:creation==='wide'?[960,540]:[612,792];state.frames.push(frame('frame-'+sequence++,{x:d.p[0]-size[0]/2,y:d.p[1]-size[1]/2,w:size[0],h:size[1]},{title:'Slide '+(sequence-1),order:Math.max(-1,...state.frames.map(f=>f.order))+1}));state.selectedIds=[state.frames.at(-1).id];}else{const f=state.frames.find(f=>f.id==='frame-draft');if(f){f.id='frame-'+sequence++;state.selectedIds=[f.id];}}creation=null;}
if(d.moved||d.action==='create')commit(d.before,d.action+' frame');else paint();
}
canvas.addEventListener('pointerup',endDrag);canvas.addEventListener('pointercancel',endDrag);canvas.addEventListener('pointerleave',()=>{if(drag)return;if(state.deckHoverId){delete state.deckHoverId;paint();}hoverTo(null);});window.addEventListener('blur',cancelDrag);
window.addEventListener('keydown',e=>{if(canvas.hidden||e.target.closest('input,textarea,select,[contenteditable="true"]'))return;
if(e.key==='Escape'){e.preventDefault();if(panels?.active){panels.close(false);return;}if(drag)cancelDrag();else if(state.presentationId){mediaSession=FramePresentationSettings.leave(mediaSession);state.presentationId=null;paint();}else if(creation){creation=null;say('Creation cancelled');}else if(state.presentationHostId){closePopup();closePresentationDrawer();paint();}else closePopup();return;}
if(state.presentationId){if(e.target.closest('#frame-media-feedback'))return;if(['ArrowRight','ArrowDown','PageDown',' '].includes(e.key)){e.preventDefault();nextSlide(1);}if(['ArrowLeft','ArrowUp','PageUp'].includes(e.key)){e.preventDefault();nextSlide(-1);}return;}
if(e.ctrlKey||e.metaKey){if(e.key.toLowerCase()==='z'){e.preventDefault();e.shiftKey?redo():undo();}else if(e.key.toLowerCase()==='y'){e.preventDefault();redo();}else if(e.key.toLowerCase()==='d'){e.preventDefault();duplicate();}else if(e.key.toLowerCase()==='c'){e.preventDefault();copySelection();}else if(e.key.toLowerCase()==='l'){e.preventDefault();runMenu('board.lock');}else if(e.key.toLowerCase()==='v'){e.preventDefault();pasteSelection();}return;}
if(['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(e.key)){e.preventDefault();const dx=e.key==='ArrowLeft'?-1:e.key==='ArrowRight'?1:0,dy=e.key==='ArrowUp'?-1:e.key==='ArrowDown'?1:0;if(state.selectedIds.length){const ids=[...state.selectedIds,...carried(state.selectedIds)],step=e.shiftKey?10:1;transact('nudge frame',()=>{for(const n of [...state.frames,...state.contents].filter(n=>ids.includes(n.id)&&!n.locked)){n.rect.x+=dx*step;n.rect.y+=dy*step;}});}else{camera.x+=dx*(e.shiftKey?240:60);camera.y+=dy*(e.shiftKey?240:60);paint();}return;}
if(['Delete','Backspace'].includes(e.key)){e.preventDefault();removeSelected();}else if(e.key==='F5'){e.preventDefault();present();}
});
document.addEventListener('pointerdown',e=>{if(popup&&!popup.contains(e.target)&&!e.target.closest('[data-action="width"],[data-action="height"]'))closePopup();},true);
$('frame-property-form').onsubmit=e=>{e.preventDefault();const f=selected(),form=e.currentTarget;if(!f)return;const n=['w','h','rotation_deg','radius','strokeWidth'].map(k=>Number(form.elements[k].value));if(n.some(v=>!Number.isFinite(v))||n[0]<8||n[1]<8)return;transact('edit frame specification',()=>{f.title=form.elements.title.value;[f.rect.w,f.rect.h,f.rotation_deg]=n;f.corner.radius=Math.max(0,Math.min(Math.min(f.rect.w,f.rect.h)/2,n[3]));f.fill=rgba(form.elements.fill.value);f.fill_authored=!form.elements.themeFill.checked;f.stroke={width:Math.max(0,n[4]),color:rgba(form.elements.stroke.value)};});document.activeElement?.blur();updateInspector();};
function download(name,value,type='application/json'){const b=new Blob([typeof value==='string'?value:JSON.stringify(value,null,2)],{type}),a=document.createElement('a');a.href=URL.createObjectURL(b);a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000);}
$('frame-fixture').onchange=e=>loadFixture(e.target.value);$('frame-reset').onclick=()=>loadFixture(fixture);$('frame-inspect').onclick=()=>inspect(inspector.hidden);$('frame-inspector-close').onclick=()=>inspect(false);$('frame-duplicate').onclick=()=>{panels?.close(true);duplicate();};$('frame-undo').onclick=undo;$('frame-redo').onclick=redo;
$('frame-zoom-in').onclick=()=>nav.zoom(camera.zoom*1.25);$('frame-zoom-out').onclick=()=>nav.zoom(camera.zoom/1.25);$('frame-zoom-reset').onclick=()=>nav.zoom(1);$('frame-fit').onclick=()=>fit();
$('frame-metrics').onclick=()=>download('slate-frame-metrics.json',metrics());$('frame-export').onclick=()=>download('slate-frame-experiment.json',{schema:'slate-frame-studio-experiment/v1',fixture,state:snapshot(),metrics:metrics(),notes:$('frame-review-notes').value});$('frame-export-notes').onclick=()=>download('slate-frame-notes.txt',$('frame-review-notes').value,'text/plain');$('frame-review-notes').oninput=persist;
inspector.querySelectorAll('[data-preset]').forEach(b=>b.onclick=()=>{creation=b.dataset.preset;inspect(false);say('Frame '+creation+' armed · click or drag · Escape cancels');});
document.fonts.load('300 12px FrameUbuntu').then(()=>document.fonts.ready).then(paint);
new MutationObserver(paint).observe(document.documentElement,{attributes:true,attributeFilter:['data-theme']});window.addEventListener('resize',paint);
window.addEventListener('studio-element-changed',e=>{footer.hidden=e.detail.id!=='frame';if(footer.hidden){mediaSession=FramePresentationSettings.leave(mediaSession);state.presentationId=null;mediaFeedback.hidden=true;inspector.hidden=true;closePopup();cancelDrag();closePresentationDrawer();}else requestAnimationFrame(()=>!camera.x&&!camera.y?fitSpecimen():paint());});
panels=window.FramePropertyPanels.create({canvas,getState:()=>state,getCamera:()=>camera,getTheme:theme,snapshot,preview:fn=>fn(),commit,restore:before=>{state.frames=copy(before.frames);state.contents=copy(before.contents);},repaint:paint,selected,extensions:{settings:FramePresentationSettings.extension({captureView:f=>fixture==='presentation-media'?mediaSession.current?.frameId===f.id?mediaSession.current.web.view:mediaSession.lastViews[f.id]||{url:'fixture://map',center:[40.7,-74],zoom:12}:null})},dismissAt:e=>{const f=frameAt(point(e));state.selectedIds=f?[f.id]:[];paint();}});
presentationDrawer=window.FramePresentationDrawer.create({
  canvas, getState:()=>state, getCamera:()=>camera, getRevision:()=>revision, getTheme:theme,
  transact, sort:sortPresentationFrames, members:presentationMembers, remove:removePresentationFrames,
  dismiss(){closePopup();closePresentationDrawer();paint();},
  change(fn, save=true){
    fn(state);
    if(save){persist();presentationDrawer.sync({reveal:true});}
    paint();
  }
});
window.addEventListener('keydown',e=>{if(canvas.hidden||!(e.ctrlKey||e.metaKey)||!['z','y','d'].includes(e.key.toLowerCase()))return;const t=e.target;if(t.closest('textarea,[contenteditable=true],input:not([type=range]):not([type=color])'))return;e.preventDefault();e.stopImmediatePropagation();if(e.key.toLowerCase()==='z')e.shiftKey?redo():undo();else if(e.key.toLowerCase()==='y')redo();else{panels.close(true);duplicate();}},true);
window.FrameStudio={presentationPreview:()=>copy(mediaSession),getState:()=>copy({state,camera,fixture,revision}),loadFixture,metrics,reset:()=>loadFixture(fixture),undo,redo,select(ids){state.selectedIds=ids.filter(id=>state.frames.some(f=>f.id===id));paint();},setCamera(next){Object.assign(camera,next);camera.zoom=window.StudioCanvasNavigation.clamp(camera.zoom);paint();},setTheme(value){document.documentElement.dataset.theme=value==='dark'?'dark':'light';paint();},saveCandidate:()=>copy({state,camera,fixture,metrics:metrics()})};
resetState(sample?new URLSearchParams(location.search).get('fixture'):'baseline');if(!sample)try{const saved=JSON.parse(localStorage.getItem(storage)||'null');if(Array.isArray(saved?.state?.frames)&&saved.state.frames.every(f=>f.rect&&Number.isFinite(f.rect.w)&&Number.isFinite(f.rect.h))){state=saved.state;state.selectedIds||=[];undoList=Array.isArray(saved.undoList)?saved.undoList:[];redoList=Array.isArray(saved.redoList)?saved.redoList:[];delete state.propertyPanel;delete state.panelHost;state.panelReveal=0;delete state.numberEdit;state.hoverId=null;state.hoverProgress=0;state.presentationId=null;fixture=fixtures[saved.fixture]?saved.fixture:'baseline';$('frame-fixture').value=fixture;$('frame-review-notes').value=saved.notes||'';}}catch{}
Decks.ensure(state);
closePresentationDrawer();
sequence=Math.max(1,...[...state.frames,...state.contents].map(n=>Number(String(n.id).split('-').at(-1))||0))+1;
paint();if(new URLSearchParams(location.search).get('tab')==='frame')window.StudioShell.activate('frame');
})();
