/* Draft per-Frame presentation policy and local fixture signals; not a media engine. */
(function(root){
'use strict';
const copy=x=>JSON.parse(JSON.stringify(x));
function settings(frame){const p=frame.presentationMedia||{},v=p.video||{},w=p.web||{},m=p.model||{};return{...p,video:{...v,start:['inherit','on-click','automatic'].includes(v.start)?v.start:'inherit',loop:typeof v.loop==='boolean'?v.loop:'inherit'},web:{...w,onEntry:w.onEntry==='last-view'?'last-view':'saved-view',savedView:w.savedView?copy(w.savedView):null},model:{...m,interactive:m.interactive!==false}};}
function patch(frame,category,values){const p=settings(frame);p[category]={...p[category],...copy(values)};frame.presentationMedia=p;}
function resolveVideo(frame,media={}){const v=settings(frame).video;return{automatic:v.start==='inherit'?media.autoplay!==false:v.start==='automatic',loop:v.loop==='inherit'?media.looped!==false:v.loop};}
const session=()=>({current:null,lastViews:{},events:[]});
function leave(runtime){if(!runtime.current)return runtime;const next=copy(runtime),old=next.current;if(old.web.view)next.lastViews[old.frameId]=copy(old.web.view);next.events.push({kind:'exit',frameId:old.frameId,videoPlaying:false});next.current=null;return next;}
function enter(runtime,frame,media={}){const next=leave(runtime),p=settings(frame),v=resolveVideo(frame,media.video),saved=p.web.savedView||media.web?.view||null,last=next.lastViews[frame.id];next.current={frameId:frame.id,video:{playing:v.automatic,loop:v.loop},web:{view:copy(p.web.onEntry==='last-view'?(last||saved):saved)},model:{interactive:p.model.interactive,yaw:0}};next.events.push({kind:'enter',frameId:frame.id,videoPlaying:v.automatic,modelInteractive:p.model.interactive});return next;}
const height=()=>136;
function extension({captureView=()=>null}){
 return{height,prepareView({frame,state,camera,canvas}){state.propertyViewport={width:canvas.clientWidth,height:canvas.clientHeight};const g=root.FrameRenderer.propertyGeometry(frame,state,camera),overflow=g.panel[1]+height()*camera.zoom-(canvas.clientHeight-8);if(overflow>0)camera.y-=overflow;},reveal(host,t){host.style.height=(height()*t)+'px';host.style.pointerEvents=t>.98?'auto':'none';const body=host.querySelector('.frame-media-drawer-content');if(body)body.style.transform='translateY('+(-height()*(1-t))+'px)';},build(host,api){
  host.onkeydown=event=>{if(event.key==='Escape'&&api.getState().propertyPanel?.kind==='settings'&&!host.closest('[data-element-studio]')?.hidden){event.preventDefault();event.stopPropagation();api.close(false);}};
  host.classList.add('frame-media-settings');const controls=[];
  const node=(tag,cls,text,parent)=>{const n=document.createElement(tag);n.className=cls;if(text!==undefined)n.textContent=text;parent.append(n);return n;};
  const body=node('div','frame-media-drawer-content',undefined,host),grid=node('div','frame-media-grid',undefined,body);
  function label(text){node('span','frame-media-category-label',text,grid);}
  function select(accessible,category,key,choices,prefix=''){const input=node('select','frame-media-capsule',undefined,grid);input.setAttribute('aria-label',accessible);for(const[value,name]of choices){const option=node('option','',prefix+name,input);option.value=value;}input.addEventListener('change',()=>api.apply(frame=>patch(frame,category,{[key]:input.value==='true'?true:input.value==='false'?false:input.value})));controls.push({input,read:()=>String(settings(api.first())[category][key])});}
  label('Video');select('Start','video','start',[['inherit','Media setting'],['on-click','On click'],['automatic','Automatically']],'Start · ');select('Loop','video','loop',[['inherit','Media setting'],['true','On'],['false','Off']],'Loop · ');
  label('Webpages');select('On entry','web','onEntry',[['saved-view','Saved view'],['last-view','Last view']]);const save=node('button','frame-media-capsule frame-media-save','Save current view',grid);save.type='button';save.title='Save a supported URL/view bookmark. The local fixture does not capture arbitrary webpage state, cookies or logins.';save.addEventListener('click',()=>{const view=captureView(api.first());if(view)api.apply(frame=>patch(frame,'web',{savedView:view}));});
  label('3D models');const row=node('label','frame-media-capsule frame-media-interactive',undefined,grid),input=node('input','',undefined,row);input.type='checkbox';input.setAttribute('aria-label','Interactive');node('span','','Interactive',row);input.addEventListener('change',()=>api.apply(frame=>patch(frame,'model',{interactive:input.checked})));controls.push({input,read:()=>settings(api.first()).model.interactive});node('span','frame-media-unused','',grid);
  return{sync(){for(const c of controls){if(c.input.type==='checkbox')c.input.checked=c.read();else c.input.value=c.read();}save.disabled=!captureView(api.first());}};
 }};
}
root.FramePresentationSettings={settings,patch,resolveVideo,session,enter,leave,height,extension};
})(typeof window!=='undefined'?window:globalThis);
