/* Slate compact property editors: color_editor.rs, corner_editor and blister.rs. */
(() => {
'use strict';
const B=window.FrameBaseline,R=window.FrameRenderer;
const clamp=(v,a,b)=>Math.max(a,Math.min(b,v)),rgbCSS=c=>'rgb('+c.slice(0,3).join(',')+')';
const rgbaCSS=c=>'rgba('+c.slice(0,3).join(',')+','+(c[3]??255)/255+')';
const number=v=>Number(v.toFixed(3)).toString();
function hsv(rgb,old=[0,0,1]){const c=rgb.slice(0,3).map(v=>v/255),hi=Math.max(...c),lo=Math.min(...c),d=hi-lo;let h=old[0];if(d){const i=c.indexOf(hi);h=((i===0?(c[1]-c[2])/d:i===1?(c[2]-c[0])/d+2:(c[0]-c[1])/d+4)/6+1)%1;}return[h,hi?d/hi:old[1],hi];}
function fromHSV(h,s,v){const k=n=>(n+h*6)%6;return [5,3,1].map(n=>Math.round(255*v*(1-s*Math.max(0,Math.min(k(n),4-k(n),1)))));}
function corner(f){const c=f.corner||{},percent=!!c.percent||String(c.kind).includes('percent'),chamfer=String(c.kind).startsWith('chamfer'),amount=c.radius??c.cut??c.amount??(typeof c.percent==='number'?c.percent:0);return{percent,chamfer,amount,max:percent?100:Math.min(f.rect.w,f.rect.h)/2};}
function create({canvas,getState,getCamera,getTheme,snapshot,preview,commit,restore,repaint,selected,dismissAt,extensions={}}){
 let active=null,closing=null,animation=0,last=performance.now(),reveal=0,root=document.createElement('div'),floating=null,drag=null;
 root.id='frame-property-host';root.className='frame-property-host element-node';root.hidden=true;root.setAttribute('aria-label','Frame property editor');canvas.append(root);root.addEventListener('pointerdown',e=>{if(!e.target.closest('input'))document.activeElement?.blur();},true);
 const node=(tag,cls,parent=root)=>{const n=document.createElement(tag);n.className=cls;if(parent)parent.append(n);return n;};
 const place=(n,x,y,w,h)=>Object.assign(n.style,{left:x+'px',top:y+'px',width:w+'px',height:h+'px'});
 const frames=()=>getState().frames.filter(f=>active?.ids.includes(f.id));
 const first=()=>frames()[0];
 function animate(open){cancelAnimationFrame(animation);last=performance.now();const tick=now=>{const dt=(now-last)/1000;last=now;reveal=clamp(reveal+(open?1:-1)*dt/.12,0,1);getState().panelReveal=reveal;if(!open&&reveal===0&&closing){closing=null;root.hidden=true;root.replaceChildren();delete getState().settingsClosing;}repaint();if((open&&reveal<1)||(!open&&reveal>0))animation=requestAnimationFrame(tick);};animation=requestAnimationFrame(tick);}
 function apply(fn){if(!active)return;preview(()=>{for(const f of frames())if(!f.locked)fn(f);});syncControls();repaint();}
 function close(applyEdits=true){
  if(!active)return;finishField(applyEdits);const a=active;active=null;drag=null;canvas.classList.remove('is-color-sampling');if(a.kind==='settings'){root.style.pointerEvents='none';closing=a;getState().settingsClosing=true;}else{root.hidden=true;root.replaceChildren();}getState().propertyPanel=null;
  if(applyEdits){const f=getState().frames.find(f=>a.ids.includes(f.id));if(f&&['fill','stroke'].includes(a.kind)){const c=(a.kind==='fill'?f.fill:f.stroke.color).slice(0,3),recent=getState().recentColors||[];getState().recentColors=[c,...recent.filter(p=>String(p)!==String(c))].slice(0,6);}commit(a.before,'edit '+a.kind);}else{restore(a.before);repaint();}
  animate(false);
 }
 function toggle(kind){
  const f=selected();if(!f||f.locked)return;const same=active?.kind===kind;close(true);if(same)return;
  if(kind==='settings'){reveal=0;getState().panelReveal=0;}
  closing=null;delete getState().settingsClosing;active={kind,ids:[...getState().selectedIds],before:snapshot(),hsv:hsv(kind==='stroke'?f.stroke.color:f.fill),rails:[],numeric:[]};
  getState().propertyPanel={kind,ids:active.ids};getState().panelHost=f.id;getState().panelLastKind=kind;root.dataset.panel=kind;root.hidden=false;root.replaceChildren();
  root.classList.toggle('frame-media-settings',kind==='settings');if(extensions[kind])active.extension=extensions[kind].build(root,{getState,first,apply,repaint,close});else if(kind==='corners')buildCorners();else buildColor();extensions[kind]?.prepareView?.({frame:f,state:getState(),camera:getCamera(),canvas});syncControls();animate(true);repaint();
 }
 function changeColor(patch){
  if(!active)return;const a=active,base=a.kind==='stroke'?first().stroke.color:first().fill,hs=a.hsv;
  let c=base.slice();if(patch.rgb){c=[...patch.rgb,c[3]??255];a.hsv=hsv(c,hs);}
  if(patch.alpha!==undefined)c[3]=Math.round(clamp(patch.alpha,0,1)*255);
  if(patch.h!==undefined)a.hsv[0]=clamp(patch.h,0,1);
  if(patch.s!==undefined)a.hsv[1]=clamp(patch.s,0,1);
  if(patch.v!==undefined)a.hsv[2]=clamp(patch.v,0,1);
  if(['h','s','v'].some(k=>patch[k]!==undefined))c=[...fromHSV(...a.hsv),c[3]];
  apply(f=>{const old=a.kind==='fill'?f.fill:f.stroke.color,rgbChanged=patch.rgb||['h','s','v'].some(k=>patch[k]!==undefined),next=[...(rgbChanged?c.slice(0,3):old.slice(0,3)),patch.alpha!==undefined?c[3]:old[3]];if(a.kind==='fill'){f.fill=next;f.fill_authored=true;}else f.stroke={...f.stroke,color:next};});
 }
 function fieldText(kind,at,value,range,unit,onChange,inline=false){
  finishField(true);const input=node('input','frame-property-number'+(inline?' inline':''));
  input.type='text';input.inputMode='decimal';input.value=number(value);input.setAttribute('aria-label',kind);input.dataset.kind=kind;place(input,at[0],at[1],at[2]||54,inline?18:22);
  floating={input,value,range,onChange,unit};input.addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();e.stopPropagation();finishField(true);}if(e.key==='Escape'){e.preventDefault();e.stopPropagation();finishField(false);}});
  input.addEventListener('blur',()=>finishField(true));input.focus({preventScroll:true});input.select();
 }
 function finishField(apply=true){if(!floating)return;const f=floating;floating=null;const text=f.input.value.trim(),v=Number(text);f.input.remove();if(apply&&text&&Number.isFinite(v))f.onChange(clamp(v,...f.range));}
 function rail(parent,x,y,w,h,{label,fraction,set,display,range,toFraction=v=>v,fromFraction=v=>v,unit='',capsule=false}){
  const hit=node('div','frame-property-rail'+(capsule?' capsule':''),parent);place(hit,x,y-4,w,h+8);hit.setAttribute('role','slider');hit.setAttribute('aria-label',label);hit.tabIndex=0;
  const track=node('div','frame-property-track',hit),handle=node('span','frame-property-handle',hit);place(track,0,4,w,h);
  const config={hit,track,handle,x,y,w,h,label,fraction,set,display,range,toFraction,fromFraction,unit,capsule};active.rails.push(config);
  const frac=e=>{const b=hit.getBoundingClientRect();return clamp((e.clientX-b.left)/b.width,0,1);};
  hit.addEventListener('pointerdown',e=>{if(e.button!==0)return;finishField(true);e.preventDefault();e.stopPropagation();const was=fraction(),b=hit.getBoundingClientRect(),onHandle=Math.abs(e.clientX-(b.left+was*b.width))<=Math.max(5,getCamera().zoom*(capsule?10:4.5));drag={config,start:[e.clientX,e.clientY],onHandle,moved:false};hit.setPointerCapture(e.pointerId);if(!onHandle)set(frac(e));});
  hit.addEventListener('pointermove',e=>{if(drag?.config!==config)return;if(Math.hypot(e.clientX-drag.start[0],e.clientY-drag.start[1])>4)drag.moved=true;if(drag.moved||!drag.onHandle){set(frac(e));showReadout(config,e);}});
  hit.addEventListener('pointerup',e=>{if(drag?.config!==config)return;const d=drag;drag=null;if(hit.hasPointerCapture(e.pointerId))hit.releasePointerCapture(e.pointerId);root.querySelector('.frame-property-readout')?.remove();if(d.onHandle&&!d.moved)fieldText(label,[x+fraction()*w-27,y-29,54],display(),config.range,config.unit,v=>set(toFraction(v)));});
  hit.addEventListener('pointercancel',()=>{drag=null;root.querySelector('.frame-property-readout')?.remove();});
  hit.addEventListener('keydown',e=>{if(!['ArrowLeft','ArrowRight'].includes(e.key))return;e.preventDefault();set(clamp(fraction()+(e.key==='ArrowLeft'?-.01:.01),0,1));});
  return config;
 }
 function showReadout(c,e){let n=root.querySelector('.frame-property-readout');if(!n)n=node('span','frame-property-readout');const b=root.getBoundingClientRect(),z=getCamera().zoom;n.textContent=number(c.display())+c.unit;n.style.left=((e.clientX-b.left)/z+10)+'px';n.style.top=((e.clientY-b.top)/z-25)+'px';}
 function buildColor(){
  const a=active,f=first(),field=node('div','frame-sv-field');place(field,5,5,396,70);field.setAttribute('aria-label','Saturation and brightness');field.tabIndex=0;
  const white=node('div','sv-white',field),black=node('div','sv-black',field),marker=node('span','sv-marker',field);a.field=field;a.marker=marker;
  const change=e=>{const b=field.getBoundingClientRect();changeColor({s:clamp((e.clientX-b.left)/b.width,0,1),v:1-clamp((e.clientY-b.top)/b.height,0,1)});};
  field.addEventListener('pointerdown',e=>{if(e.button!==0)return;e.preventDefault();e.stopPropagation();field.setPointerCapture(e.pointerId);change(e);});
  field.addEventListener('pointermove',e=>{if(field.hasPointerCapture(e.pointerId))change(e);});field.addEventListener('pointerup',e=>{if(field.hasPointerCapture(e.pointerId))field.releasePointerCapture(e.pointerId);});
  const color=()=>a.kind==='stroke'?first().stroke.color:first().fill;
  rail(root,12,96,396,7,{label:'Opacity',fraction:()=>color()[3]/255,set:t=>changeColor({alpha:t}),display:()=>Math.round(color()[3]/255*100),range:[0,100],toFraction:v=>v/100,unit:'%'});
  rail(root,12,112,396,7,{label:'Saturation',fraction:()=>a.hsv[1],set:t=>changeColor({s:t}),display:()=>Math.round(a.hsv[1]*100),range:[0,100],toFraction:v=>v/100,unit:'%'});
  rail(root,12,128,396,7,{label:'Hue',fraction:()=>a.hsv[0],set:t=>changeColor({h:t}),display:()=>Math.round(a.hsv[0]*360),range:[0,360],toFraction:v=>v/360,unit:'°'});
  if(a.kind==='stroke')rail(root,12,144,396,7,{label:'Stroke width',fraction:()=>Math.log1p(first().stroke.width)/Math.log(10001),set:t=>apply(f=>f.stroke={...f.stroke,width:Math.max(0,Math.exp(t*Math.log(10001))-1)}),display:()=>first().stroke.width,range:[0,10000],toFraction:v=>Math.log1p(v)/Math.log(10001),unit:' u'});
  const y=(a.kind==='stroke'?190:174)-20,drop=node('button','frame-property-dropper');place(drop,9.5,y-11.5,23,23);drop.type='button';drop.setAttribute('aria-label','Sample desktop color');drop.title='Sample anywhere on the desktop';
  drop.innerHTML='<svg viewBox="0 0 24 24"><path d="M4 20 6 14 16 4 20 8 10 18ZM13 5 19 11"/></svg>';
  drop.addEventListener('click',async()=>{if(window.EyeDropper){try{const result=await new EyeDropper().open(),c=result.sRGBHex.slice(1).match(/../g).map(v=>parseInt(v,16));changeColor({rgb:c});}catch{}}else{a.sampling=true;canvas.classList.add('is-color-sampling');drop.title='Click a Frame or canvas color to sample; Escape cancels';}});
  a.recent=node('div','frame-recent-colors');place(a.recent,40,y-11,154,22);
  const divider=node('i','frame-property-divider');place(divider,204,y-10,1,20);
  for(let i=0;i<3;i++){const n=node('button','frame-rgb-number');n.type='button';place(n,215+i*65,y-10,58,20);n.setAttribute('aria-label',['Red','Green','Blue'][i]+' percentage');a.numeric.push(n);n.addEventListener('click',()=>fieldText(n.getAttribute('aria-label'),[235+i*65,y-10,35],Math.round(color()[i]/2.55),[0,100],'%',v=>{const c=color().slice(0,3);c[i]=Math.round(v*2.55);changeColor({rgb:c});},true));}
 }
 function buildCorners(){
  const a=active;
  const makeSegments=(cls,labels,x,w,read,set)=>{const box=node('div','frame-corner-segments '+cls);place(box,x,2.6,w,16.9);labels.forEach((label,i)=>{const b=node('button','',box);b.type='button';b.textContent=label;b.setAttribute('aria-label',label);b.onclick=()=>set(i);});a[cls]={box,read};};
  makeSegments('treatment',['Fillet','Chamfer'],2.6,126,()=>corner(first()).chamfer?1:0,i=>apply(f=>{const c=corner(f);f.corner={kind:(i?'chamfer':'rounded')+(c.percent?'-percent':''),radius:c.amount,percent:c.percent};}));
  makeSegments('units',['%','u'],350,68,()=>corner(first()).percent?0:1,i=>apply(f=>{const c=corner(f),p=i===0,limit=Math.min(f.rect.w,f.rect.h)/2,absolute=c.percent?c.amount*limit/100:c.amount;f.corner={kind:(c.chamfer?'chamfer':'rounded')+(p?'-percent':''),radius:p?(limit?absolute/limit*100:0):absolute,percent:p};}));
  rail(root,134,2.6,210,16.9,{label:'Corner amount',fraction:()=>{const c=corner(first());return c.max?c.amount/c.max:0;},set:t=>apply(f=>{const c=corner(f);f.corner={kind:(c.chamfer?'chamfer':'rounded')+(c.percent?'-percent':''),radius:t*c.max,percent:c.percent};}),display:()=>corner(first()).amount,range:[0,corner(first()).max],toFraction:v=>v/corner(first()).max,unit:corner(first()).percent?'%':' u',capsule:true});
 }
 function syncControls(){
  if(!active||!first())return;const a=active,f=first();if(a.extension){a.extension.sync();return;}
  if(a.kind==='corners'){const c=corner(f);a.rails[0].range=[0,c.max];a.rails[0].unit=c.percent?'%':' u';for(const k of['treatment','units'])[...a[k].box.children].forEach((b,i)=>b.setAttribute('aria-pressed',String(i===a[k].read())));}
  else{
   const c=a.kind==='stroke'?f.stroke.color:f.fill,hue=rgbCSS(fromHSV(a.hsv[0],1,1));
   a.field.style.background=hue;a.marker.style.left=(a.hsv[1]*100)+'%';a.marker.style.top=((1-a.hsv[2])*100)+'%';
   a.rails[0].track.style.background='repeating-conic-gradient(var(--p-border) 0% 25%,var(--p-card) 0% 50%) 0/8px 8px';
   a.rails[1].track.style.background='linear-gradient(90deg,#fff,'+hue+')';a.rails[2].track.style.background='linear-gradient(90deg,red,#ff0,#0f0,#0ff,#00f,#f0f,red)';
   if(a.rails[3])a.rails[3].track.style.background='var(--p-hover)';
   a.numeric.forEach((n,i)=>{n.innerHTML='<span>'+['R','G','B'][i]+' </span><u>'+Math.round(c[i]/2.55)+'%</u>';});
   const recent=getState().recentColors||[];if(a.recent.dataset.colors!==JSON.stringify(recent)){a.recent.dataset.colors=JSON.stringify(recent);a.recent.replaceChildren();recent.slice(0,6).forEach(rgb=>{const b=node('button','',a.recent);b.style.background=rgbCSS(rgb);b.title=rgbCSS(rgb);b.onclick=()=>changeColor({rgb});});}
  }
  for(const r of a.rails){r.handle.style.left=(clamp(r.fraction(),0,1)*100)+'%';r.hit.setAttribute('aria-valuenow',number(r.display()));r.hit.setAttribute('aria-valuemin',r.range[0]);r.hit.setAttribute('aria-valuemax',r.range[1]);}
 }
 function sync(){
  if(!active&&!closing)return;const owner=active||closing,f=getState().frames.find(f=>owner.ids.includes(f.id)),state=getState();if(!f||canvas.hidden||!owner.ids.every(id=>state.selectedIds.includes(id))){close(true);return;}
  const camera=getCamera(),z=camera.zoom,h=extensions[owner.kind]?extensions[owner.kind].height(state):owner.kind==='corners'?22.1:owner.kind==='stroke'?190:174;
  const g=R.propertyGeometry(f,state,camera),x=g.panel[0],y=g.panel[1];
  root.style.left=x+'px';root.style.top=y+'px';root.style.width=(g.panelWidth||420)+'px';root.style.height=h+'px';root.style.transform='scale('+z+')';
  root.hidden=12*z<4;root.dataset.theme=getTheme();if(extensions[owner.kind]?.reveal)extensions[owner.kind].reveal(root,reveal);else root.style.pointerEvents='';if(!active)root.style.pointerEvents='none';if(active)syncControls();
 }
 document.addEventListener('pointerdown',event=>{
  if(event.button!==0||!active)return;
  const target=event.target,insideCanvas=canvas.contains(target);
  if((active.sampling&&insideCanvas)||root.contains(target)||target.closest('.frame-property-strip'))return;
  const grip=target.closest('[data-action=resize],[data-action=rotate],[data-action=fillet]');
  // Closing repaints the SVG and detaches the event target. Claim dismissal first.
  if(insideCanvas&&!grip){event.preventDefault();event.stopImmediatePropagation();}
  finishField(true);
  close(!grip);
  if(insideCanvas&&!grip)dismissAt?.(event);
 },true);
 canvas.addEventListener('pointerdown',e=>{if(!active?.sampling)return;const t=document.elementFromPoint(e.clientX,e.clientY),frameId=t?.closest('[data-frame-id]')?.dataset.frameId,f=getState().frames.find(f=>f.id===frameId);changeColor({rgb:f?(f.fill_authored?f.fill:B.fillFor(f,getTheme())).slice(0,3):(getTheme()==='dark'?[14,16,19]:[246,247,248])});active.sampling=false;canvas.classList.remove('is-color-sampling');e.preventDefault();e.stopImmediatePropagation();},true);
 return{toggle,close,sync,get active(){return active?.kind||null;},get editing(){return !!floating;},get element(){return root;}};
}
window.FramePropertyPanels={create,corner,hsv,fromHSV};
})();
