/* Source-measured Frame SVG interpreter. Native raster acceptance remains separate. */
(function(root){
'use strict';
const B=()=>root.FrameBaseline,NS='http://www.w3.org/2000/svg',HANDLES=['NW','N','NE','E','SE','S','SW','W'];
const ICONS={settings:'M4 7H20M4 17H20M8 4V10M16 14V20',fill:'M5 10 12 3 20 11 12 19 4 11ZM5 11H20M19 16Q15 21 19 21Q23 21 19 16M9 3 14 8',stroke:'M20 12a8 8 0 1 1-16 0 8 8 0 1 1 16 0',corners:'M5 19V11Q5 5 11 5H19M13 19H19V13',prev:'M15 5 8 12 15 19',next:'M9 5 16 12 9 19',present:'M2 12Q12-1 22 12Q12 25 2 12ZM15 12a3 3 0 1 1-6 0 3 3 0 1 1 6 0',deck:'M9 5H21M9 12H21M9 19H21M3 4l1-1v4M3 10q3-1 2 1l-2 3h3M3 17h2l-1 2q3 0 1 2H3'};
const ACTIONS=['fill','stroke','corners','deck','settings','present'],CLOSED_WIDTH=ACTIONS.length*30+(ACTIONS.length-1)*7;
const BLISTER_HEIGHT=40, BLISTER_SHOULDER=84, BLISTER_INSET=42;
const BLISTER_CLOSED_WIDTH=ACTIONS.length*48+2*BLISTER_SHOULDER, BLISTER_OPEN_WIDTH=ACTIONS.length*48+96;
const LABELS={settings:'Presentation media settings',fill:'Fill',stroke:'Stroke',corners:'Corners: fillet / chamfer',prev:'Move earlier in the deck',next:'Move later in the deck',present:'Present from this slide',deck:'Create Presentation'};
const EXTRA={light:{card_hover:'#fbfcfd',border_strong:'#c7cdd4',select_fill:'#d7e8ff',select_stroke:'#1f6fb2'},dark:{card_hover:'#242931',border_strong:'#4a525c',select_fill:'#2b5c8a',select_stroke:'#c0deff'}};
let ctx;
const add=(a,b)=>[a[0]+b[0],a[1]+b[1]],sub=(a,b)=>[a[0]-b[0],a[1]-b[1]],mul=(a,k)=>[a[0]*k,a[1]*k],len=a=>Math.hypot(...a),mid=(a,b)=>mul(add(a,b),.5),unit=a=>len(a)>1e-8?mul(a,1/len(a)):[0,0],pts=a=>a.map(p=>p.join(',')).join(' ');
function el(tag,attrs={},parent){const n=document.createElementNS(NS,tag);for(const[k,v]of Object.entries(attrs))if(v!==undefined&&v!==null)n.setAttribute(k,String(v));if(parent)parent.appendChild(n);return n;}
function rgba(c){return Array.isArray(c)?'rgba('+c[0]+','+c[1]+','+c[2]+','+(c[3]??255)/255+')':c||'none';}
const rotate=(p,c,d)=>B().rotatePoint(p,c,d),project=(p,c)=>[p[0]*c.zoom+c.x,p[1]*c.zoom+c.y];
function textMetrics(text,size){ctx||=document.createElement('canvas').getContext('2d');ctx.font='300 '+size+'px FrameUbuntu';return{width:ctx.measureText(text).width,height:size*1.149,ascent:size*.932};}
function textAt(parent,text,center,size,color,angle=0,attrs={}){const m=textMetrics(text,size),t=el('text',{x:0,y:m.ascent-m.height*.5,'font-size':size,fill:color,'text-anchor':'middle',transform:'translate('+center+') rotate('+angle+')',...attrs},parent);t.textContent=text;return t;}
function outline(f,z,joinedAt){
 const deg=f.rotation_deg||0,center=B().center(f.rect);
 const squareCorners=joinedAt===undefined?null:new Set(B().corners(f.rect,deg)
  .flatMap((p,i)=>Math.abs(p[1]-joinedAt)<1e-6?[i]:[]));
 return B().cornerOutline(f.rect,f.corner,z,squareCorners).map(p=>rotate(p,center,deg));
}
function openLowerEdge(poly,lower){
 const seam=poly.findIndex((p,i)=>Math.abs(p[1]-lower)<1e-6&&Math.abs(poly[(i+1)%poly.length][1]-lower)<1e-6);
 return seam<0?null:'M'+[...poly.slice(seam+1),...poly.slice(0,seam+1)].join(' L');
}
function deckHighlight(state,id,included,palette,attached=false){
 const hovered=state.deckMode&&state.deckHoverId===id,eligible=included||state.deckMode;
 return {stroke:!eligible?palette.border:included||hovered?palette.accent:palette.accent+'80',
  width:attached?2:hovered?2.6:included?2:1.5,hovered};
}
function label(parent,f,text,size,color,right,camera,klass,lift=6){if(size*camera.zoom<4)return;const m=textMetrics(text,size),p=B().labelPlacement(f.rect,f.rotation_deg||0,m.width,m.height,right);if(lift!==6)p.center=add(p.center,mul(B().upperEdge(f.rect,f.rotation_deg||0).outward,lift-6));return textAt(parent,text,p.center,size,color,p.angle,{class:klass,'pointer-events':'none'});}
function screenGeom(f,camera){const g=B().selectionGeometry(f.rect,f.rotation_deg||0,camera.zoom),offset=p=>add(p,[camera.x,camera.y]);return{corners:g.corners.map(offset),controls:g.controlCorners.map(offset),resize:g.prominence.resize,rotate:g.prominence.rotate,shortest:g.projectedShortest};}
function hitTargets(parent,f,camera,g){const cs=g.controls,id=f.id,edge=6*g.resize,corner=7*g.resize;if(edge>=2)cs.forEach((a,i)=>el('line',{x1:a[0],y1:a[1],x2:cs[(i+1)%4][0],y2:cs[(i+1)%4][1],stroke:'transparent','stroke-width':edge*2,'pointer-events':'stroke','data-frame-id':id,'data-action':'resize','data-handle':HANDLES[i*2+1],class:'frame-hit resize-'+HANDLES[i*2+1]},parent));if(corner>=2)cs.forEach((a,i)=>el('circle',{cx:a[0],cy:a[1],r:corner,fill:'transparent','data-frame-id':id,'data-action':'resize','data-handle':HANDLES[i*2],class:'frame-hit resize-'+HANDLES[i*2]},parent));const reach=9*g.rotate,radius=Math.max(12,18*g.rotate),center=mid(cs[0],cs[2]);if(reach>=2)cs.forEach((a,i)=>{const bearing=Math.atan2(a[1]-center[1],a[0]-center[0]),p=[a[0]+Math.cos(bearing-Math.PI/4)*radius,a[1]+Math.sin(bearing-Math.PI/4)*radius],q=[a[0]+Math.cos(bearing+Math.PI/4)*radius,a[1]+Math.sin(bearing+Math.PI/4)*radius];el('path',{d:'M'+p+'A'+radius+','+radius+',0,0,1,'+q,fill:'none',stroke:'transparent','stroke-width':reach*2,'pointer-events':'stroke','data-frame-id':id,'data-action':'rotate','data-handle':i,class:'frame-hit rotate-hit'},parent);});}
function filletGrip(parent,f,camera,g,theme,state){if(9*g.resize<2||f.locked)return;const r=f.rect,raw=f.corner||{},amount=raw.radius??raw.cut??raw.amount??0,radius=Math.min(Math.max(0,raw.percent?amount*Math.min(r.w,r.h)/200:amount),Math.min(r.w,r.h)/2),travel=Math.max(radius,Math.min(10,r.w/2));let p=project(rotate([r.x+travel,r.y],B().center(r),f.rotation_deg||0),camera);if(g.controls.some((a,i)=>len(sub(a,g.corners[i]))>.5))p=add(g.controls[0],mul(unit(sub(g.controls[1],g.controls[0])),Math.min(Math.max(10,travel*camera.zoom),len(sub(g.controls[1],g.controls[0]))/3)));const half=1.6*g.resize,stroke=g.resize;el('rect',{x:p[0]-half+stroke/2,y:p[1]-half+stroke/2,width:half*2-stroke,height:half*2-stroke,fill:state.hotAction==='fillet'?theme.select_fill:'#fff',stroke:theme.select,'stroke-width':stroke,'data-frame-id':f.id,'data-action':'fillet',class:'frame-fillet-grip'},parent);el('circle',{cx:p[0],cy:p[1],r:9*g.resize,fill:'transparent','data-frame-id':f.id,'data-action':'fillet',class:'frame-hit fillet-hit'},parent);}
function squircle(x,y,size){return Array.from({length:32},(_,i)=>{const a=Math.PI*2*i/32,c=Math.cos(a),s=Math.sin(a);return[x+size/2+size/2*Math.sign(c)*Math.pow(Math.abs(c),.8),y+size/2+size/2*Math.sign(s)*Math.pow(Math.abs(s),.8)];});}
function stripBottom(f,poly,width){const xs=poly.map(p=>p[0]),x=(Math.min(...xs)+Math.max(...xs))/2,left=x-width/2,right=x+width/2;let bottom=-Infinity;poly.forEach((a,i)=>{const b=poly[(i+1)%poly.length],lo=Math.max(Math.min(a[0],b[0]),left),hi=Math.min(Math.max(a[0],b[0]),right);if(lo>hi)return;if(Math.abs(b[0]-a[0])<1e-7)bottom=Math.max(bottom,a[1],b[1]);else for(const at of[lo,hi])bottom=Math.max(bottom,a[1]+(b[1]-a[1])*(at-a[0])/(b[0]-a[0]));});return[x,Number.isFinite(bottom)?bottom:f.rect.y+f.rect.h];}
function drawerGeometry(geometry, camera, viewportWidth) {
 const {bounds, eligible, y, height} = geometry;
 const {x, y:cameraY, zoom} = camera;
 const available = viewportWidth / zoom - 16;
 const width = eligible ? bounds.w : Math.max(460, Math.min(bounds.w, available));
 let left = (bounds.x + (bounds.w-width)/2)*zoom + x;
 if (!eligible && width <= available) left = Math.max(8*zoom,Math.min(left,viewportWidth-(width+8)*zoom));
 const lower = bounds.y+bounds.h;
 const top = eligible ? lower-1 : Math.max(lower,y+height)+8;
 return {panel:[left,top*zoom+cameraY],panelWidth:width,drawer:true};
}
function propertyGeometry(f,state,camera={x:0,y:0,zoom:1}){
 // Use one world outline for both states. Render tessellation can vary with zoom.
 const poly=outline(f,1),xs=poly.map(p=>p[0]),ys=poly.map(p=>p[1]);
 const lower=Math.max(...ys),bounds={x:Math.min(...xs),y:Math.min(...ys),w:Math.max(...xs)-Math.min(...xs),h:lower-Math.min(...ys)};
 const center=bounds.x+bounds.w/2,left=center-BLISTER_CLOSED_WIDTH/2,right=center+BLISTER_CLOSED_WIDTH/2;
 const flatEdge=poly.some((a,i)=>{const b=poly[(i+1)%poly.length];return Math.abs(a[1]-lower)<1e-6&&Math.abs(b[1]-lower)<1e-6&&Math.min(a[0],b[0])<=left+1e-6&&Math.max(a[0],b[0])>=right-1e-6;});
 const eligible=flatEdge&&BLISTER_CLOSED_WIDTH*BLISTER_HEIGHT<=bounds.w*bounds.h*.2;
 const bottom=eligible?[center,lower]:stripBottom(f,poly,CLOSED_WIDTH);
 const t=eligible && state.presentationHostId===f.id ? 1
   : eligible && state.panelHost===f.id ? Math.max(0,Math.min(1,state.panelReveal||0)) : 0;
 const width=eligible?BLISTER_CLOSED_WIDTH+(BLISTER_OPEN_WIDTH-BLISTER_CLOSED_WIDTH)*t:CLOSED_WIDTH;
 const height=eligible?BLISTER_HEIGHT:30,x=bottom[0]-width/2,y=eligible?bottom[1]-height*(.5+.5*t):bottom[1]+14;
 const shoulder=BLISTER_SHOULDER+(BLISTER_INSET-BLISTER_SHOULDER)*t;
 const pitch=(width-2*shoulder)/ACTIONS.length;
 const iconSize=Math.min(height*.78,pitch*.82);
 const iconRects=ACTIONS.map((_,i)=>eligible?[x+shoulder+pitch*(i+.5)-iconSize/2,y+(height-iconSize)/2,iconSize,iconSize]:[x+37*i+7,y+7,16,16]);
 const kind=state.propertyPanel?.kind||state.panelLastKind||'fill',panelHeight=kind==='settings'?root.FramePresentationSettings.height(state):kind==='corners'?22.1:kind==='stroke'?190:174,z=camera.zoom,gap=8,anchor=eligible&&state.propertyPanel?{x:bottom[0]-BLISTER_OPEN_WIDTH/2,y:bottom[1]-BLISTER_HEIGHT,w:BLISTER_OPEN_WIDTH,h:BLISTER_HEIGHT}:{x,y,w:width,h:height},v=state.propertyViewport||{width:innerWidth,height:innerHeight},viewport={x:-camera.x/z,y:-camera.y/z,w:v.width/z,h:v.height/z};
 if(kind==='settings'&&(state.propertyPanel||state.settingsClosing)){
  const geometry={bounds,bottom,t,eligible,x,y,width,height,shoulder,iconRects,panelHeight};
  return {...geometry,...drawerGeometry(geometry,camera,v.width)};
 }
 const intersects=(a,b)=>a.x<b.x+b.w&&a.x+a.w>b.x&&a.y<b.y+b.h&&a.y+a.h>b.y;
 const visible={x:Math.max(bounds.x,viewport.x),y:Math.max(bounds.y,viewport.y),w:Math.max(0,Math.min(bounds.x+bounds.w,viewport.x+viewport.w)-Math.max(bounds.x,viewport.x)),h:Math.max(0,Math.min(bounds.y+bounds.h,viewport.y+viewport.h)-Math.max(bounds.y,viewport.y))},mid=[visible.x+visible.w/2,visible.y+visible.h/2],pw=420,ph=panelHeight,bx=x+width/2-pw/2,sideY=mid[1]-ph/2;
 const preferred=anchor.y+anchor.h/2>=bounds.y+bounds.h?[bx,y+height+gap]:[bx,y-ph-gap],candidates=[preferred,[mid[0]-pw/2,bounds.y+bounds.h+gap],[bounds.x+bounds.w+gap,sideY],[bounds.x-gap-pw,sideY],[bx,y+height+gap],[x+width+gap,y],[x-gap-pw,y]];
 const fits=p=>p[0]>=viewport.x+8&&p[1]>=viewport.y+8&&p[0]+pw<=viewport.x+viewport.w-8&&p[1]+ph<=viewport.y+viewport.h-8&&!intersects({x:p[0],y:p[1],w:pw,h:ph},anchor);
 let at=candidates.find(p=>fits(p)&&!intersects({x:p[0],y:p[1],w:pw,h:ph},bounds));
 if(!at&&anchor.y+anchor.h/2<bounds.y+bounds.h)at=candidates.find(fits);at||=preferred;
 return{bounds,bottom,t,eligible,x,y,width,height,shoulder,iconRects,panel:[at[0]*z+camera.x,at[1]*z+camera.y],panelHeight};
}
function displayedOutline(poly,strip){
 const depth=BLISTER_HEIGHT*.5*(1-(strip?.t??1));
 if(!strip?.eligible||depth<=1e-6)return null;
 const [center,bottom]=strip.bottom,left=center-BLISTER_CLOSED_WIDTH/2,right=center+BLISTER_CLOSED_WIDTH/2;
 const near=(a,b)=>Math.abs(a-b)<1e-6;
 const points=poly.flatMap((a,i)=>{
  const b=poly[(i+1)%poly.length];
  if(!near(a[1],bottom)||!near(b[1],bottom)||Math.min(a[0],b[0])>left+1e-6||Math.max(a[0],b[0])<right-1e-6)return[a];
  const shoulder=(start,direction)=>Array.from({length:17},(_,j)=>{
   const t=j/16,ease=t*t*t*(t*(t*6-15)+10);
   return[start+direction*BLISTER_SHOULDER*t,bottom-depth*ease];
  });
  const notch=[...shoulder(left,1),...shoulder(right,-1).reverse()];
  return[a,...(a[0]<b[0]?notch:notch.reverse())];
 });
 return 'M'+points.join(' L')+' Z';
}
function paintOutline(parent,poly,path,attrs){
 return path?el('path',{d:path,...attrs},parent):el('polygon',{points:pts(poly),...attrs},parent);
}
function propertyStrip(parent,f,g,theme,state){
 const actions=ACTIONS,group=el('g',{class:'element-node frame-property-strip'+(g.eligible?' frame-blister-menu':''),'data-frame-id':f.id,'data-presentation':g.eligible?'blister':'squircles'},parent);
 actions.forEach((action,i)=>{const x=g.bottom[0]-CLOSED_WIDTH/2+37*i,y=g.bottom[1]+14,active=(action==='deck'&&state.presentationHostId===f.id)||state.propertyPanel?.kind===action,item=el('g',{'data-frame-id':f.id,'data-action':action,class:'frame-strip-button'+(active?' active':''),role:'button','aria-label':LABELS[action]},group);
 if(!g.eligible)el('polygon',{points:pts(squircle(x,y,30)),fill:active?theme.select_fill:theme.card,stroke:active?theme.select:theme.border_strong,'stroke-width':.8,class:'frame-button-surface'},item);
 const r=g.iconRects[i];el('rect',{x:r[0]-5,y:r[1]-5,width:r[2]+10,height:r[3]+10,fill:'transparent','data-action':action,'data-frame-id':f.id},item);
 el('path',{d:ICONS[action],transform:'translate('+r[0]+','+r[1]+') scale('+(r[2]/24)+')',fill:'none',stroke:active?theme.select_stroke:theme.ink,'stroke-width':1.5,'stroke-linecap':'round','stroke-linejoin':'round','pointer-events':'none'},item);el('title',{},item).textContent=LABELS[action];});
}

function dimensionLane(home,outward,opposite,externalStrip){const along=sub(home[1],home[0]),lane=Math.abs(along[0])>=Math.abs(along[1])?[0,1]:[1,0],offset=mul(unit(outward),externalStrip&&Math.abs(outward[1])>=Math.abs(outward[0])?68:24);return outward[0]*lane[0]+outward[1]*lane[1]<-.0001?{ends:opposite,offset:mul(offset,-1)}:{ends:home,offset};}
// These strings and leaders follow selection_tools::{number,stringer,inline_number}.
// The editor keeps the numerical substring separate from the always-painted suffix.
function dimensionGeometry(f,kind,camera={x:0,y:0,zoom:1},editingText,externalStrip=false){
  const p=B().corners(f.rect,f.rotation_deg||0),ux=unit(sub(p[1],p[0])),uy=unit(sub(p[2],p[1]));
  const lane=kind==='width'?dimensionLane([p[3],p[2]],uy,[p[0],p[1]],externalStrip):dimensionLane([p[1],p[2]],ux,[p[0],p[3]],externalStrip);
  const a=add(lane.ends[0],lane.offset),b=add(lane.ends[1],lane.offset),direction=unit(sub(b,a)),value=f.rect[kind==='width'?'w':'h'];
  const text=editingText===undefined?value.toFixed(3).replace(/\.?0+$/,''):String(editingText),suffix=' u',label=text+suffix;
  const numeric=textMetrics(text,12),post=textMetrics(suffix,12),whole=textMetrics(label,12),totalWidth=numeric.width+post.width,halfGap=whole.width/2+5;
  let angle=Math.atan2(direction[1],direction[0])*180/Math.PI,center=mid(a,b),segments;
  if(angle>90-.001*180/Math.PI)angle-=180;if(angle<-90)angle+=180;
  if(len(sub(b,a))>halfGap*2)segments=[[a,sub(center,mul(direction,halfGap))],[add(center,mul(direction,halfGap)),b]];
  else{const leader=add(b,mul(direction,8));segments=[[a,leader]];center=add(leader,mul(direction,halfGap));}
  const axis=[Math.cos(angle*Math.PI/180),Math.sin(angle*Math.PI/180)],numericCenter=sub(center,mul(axis,post.width/2)),suffixCenter=add(center,mul(axis,(totalWidth-post.width)/2));
  const zoom=camera.zoom??1;
  return {frameId:f.id,kind,value,text,suffix,label,a,b,direction,center,angle,segments,numericCenter,numericWidth:numeric.width,suffixCenter,totalWidth,textHeight:numeric.height,hit:{width:totalWidth+8,height:22},screen:{center:project(center,camera),numericCenter:project(numericCenter,camera),numericWidth:numeric.width*zoom,fontSize:12*zoom,lineHeight:numeric.height*zoom,angle}};
}
function dimension(parent,f,kind,theme,camera,state,externalStrip){
  if(12*camera.zoom<4)return;
  const edit=state.numberEdit?.frameId===f.id&&state.numberEdit.kind===kind?state.numberEdit:null,d=dimensionGeometry(f,kind,camera,edit?.text,externalStrip);
  const attrs={'data-action':kind,'data-frame-id':f.id},g=el('g',{class:'element-node frame-dimension',...attrs,'data-value':d.value},parent);
  for(const p of[d.a,d.b])el('circle',{cx:p[0],cy:p[1],r:1.8,fill:theme.sub,'fill-opacity':.72,'pointer-events':'none'},g);
  for(const[p,q]of d.segments)el('line',{x1:p[0],y1:p[1],x2:q[0],y2:q[1],stroke:theme.sub,'stroke-opacity':.72,'stroke-width':.7,'pointer-events':'none'},g);
  textAt(g,d.suffix,d.suffixCenter,12,theme.sub,d.angle,{...attrs,'fill-opacity':.72,class:'frame-dimension-suffix'});
  if(!edit)textAt(g,d.text,d.numericCenter,12,theme.sub,d.angle,{...attrs,'fill-opacity':.72,class:'frame-dimension-number'});
  el('rect',{x:-d.hit.width/2,y:-d.hit.height/2,width:d.hit.width,height:d.hit.height,transform:'translate('+d.center+') rotate('+d.angle+')',fill:'transparent',...attrs,'data-center':d.center.join(','),'data-numeric-center':d.numericCenter.join(','),'data-angle':d.angle,'data-numeric-width':d.numericWidth,'data-text-height':d.textHeight,class:'frame-number-hit'},g);
}
function dimensions(parent,f,theme,camera,state,externalStrip){dimension(parent,f,'width',theme,camera,state,externalStrip);dimension(parent,f,'height',theme,camera,state,externalStrip);}
function content(parent,n,theme,selected){const r=n.rect,c=B().center(r),g=el('g',{class:'frame-content','data-content-id':n.id,'data-action':'content',transform:'rotate('+(n.rotation_deg||0)+' '+c+')'},parent);if(n.kind!=='text')el('rect',{x:r.x,y:r.y,width:r.w,height:r.h,rx:n.radius??8,fill:rgba(n.fill||theme.accent)},g);if(n.text){const t=el('text',{x:r.x+(n.kind==='text'?0:16),y:r.y+(n.kind==='text'?14:28),'font-size':n.fontSize||14,fill:n.textColor||theme.ink},g);n.text.split('\n').forEach((line,i)=>{const span=el('tspan',{x:r.x+(n.kind==='text'?0:16),dy:i?20:0},t);span.textContent=line;});}if(selected)el('rect',{x:r.x,y:r.y,width:r.w,height:r.h,fill:theme.select,'fill-opacity':.16,stroke:theme.select,'stroke-width':1.5,'pointer-events':'none'},g);}
function slidePreview(frame, contents, theme) {
 const rect = frame.rect, palette = B().tokens.themes[theme];
 const preview = el('svg',{viewBox:[rect.x,rect.y,rect.w,rect.h].join(' '),preserveAspectRatio:'xMidYMid slice','aria-hidden':'true'});
 el('rect',{x:rect.x,y:rect.y,width:rect.w,height:rect.h,fill:frame.fill_authored?rgba(frame.fill):palette.card},preview);
 for (const item of contents) content(preview,item,palette,false);
 return preview;
}
function membershipBadge(parent, frame, included, hovered, palette) {
 const edge = B().upperEdge(frame.rect, frame.rotation_deg);
 const radius = Math.min(11, frame.rect.w/4, frame.rect.h/4);
 const inset = radius + 5;
 const center = sub(sub(edge.b, mul(edge.tangent,inset)),mul(edge.outward,inset));
 const name = (included?'Remove ':'Add ') + frame.title + (included?' from presentation':' to presentation');
 const group = el('g',{class:'frame-membership-badge','data-frame-id':frame.id,'data-action':'membership',
  'data-member':String(included),role:'button','aria-label':name,transform:'translate('+center+')'},parent);
 el('circle',{r:radius,fill:hovered?palette.accent:palette.card,stroke:palette.accent,'stroke-width':1},group);
 const size = radius*.4;
 el('path',{d:'M'+(-size)+' 0H'+size+(included?'':'M0 '+(-size)+'V'+size),
  fill:'none',stroke:hovered?palette.card:palette.accent,'stroke-width':1.7,'stroke-linecap':'round',
  'pointer-events':'none'},group);
 el('title',{},group).textContent=name;
}
function render({svg,state,camera,theme='light'}){
if(!svg||!root.FrameBaseline)return;camera={x:0,y:0,zoom:1,...camera};const palette={...EXTRA[theme==='dark'?'dark':'light'],...B().tokens.themes[theme]};svg.replaceChildren();svg.classList.add('frame-canvas');svg.dataset.theme=theme;
let frames=(state.frames||[]).filter(f=>!f.hidden),contents=(state.contents||[]).filter(n=>!n.hidden);const presenting=state.presentationId&&frames.find(f=>f.id===state.presentationId);
if(presenting){frames=[presenting];const r=presenting.rect,w=svg.clientWidth,h=svg.clientHeight,z=Math.min(w/r.w,h/r.h);camera={x:(w-r.w*z)/2-r.x*z,y:(h-r.h*z)/2-r.y*z,zoom:z};contents=contents.filter(n=>B().membersOf(state,presenting.id).includes(n.id));const defs=el('defs',{},svg),clip=el('clipPath',{id:'frame-present-clip'},defs);el('rect',{x:r.x,y:r.y,width:r.w,height:r.h},clip);}
const world=el('g',{class:'frame-world',transform:'translate('+camera.x+' '+camera.y+') scale('+camera.zoom+')','clip-path':presenting?'url(#frame-present-clip)':undefined},svg),selected=new Set(state.selectedIds||[]),metrics=[];
const positions = new Map(root.FrameDeck.slides(state).map((frame,index)=>[frame.id,index+1]));
const stripHost=!presenting&&selected.size===1?frames.find(f=>selected.has(f.id)):null;
const strip=stripHost?propertyGeometry(stripHost,state):null,paths=new Map();
const attached=strip?.eligible&&state.presentationHostId===stripHost.id,joinedAt=attached?strip.bottom[1]:undefined;
for(const f of frames){const joined=attached&&f===stripHost,poly=outline(f,camera.zoom,joined?joinedAt:undefined),path=f===stripHost?displayedOutline(poly,strip):null,g=el('g',{class:'element-node frame-node','data-frame-id':f.id},world),fill=f.fill_authored?rgba(f.fill):palette.card;paths.set(f.id,path);paintOutline(g,poly,path,{fill,'fill-opacity':f.opacity??1,'data-frame-id':f.id,'data-action':'body',class:'frame-plate'});if(f.stroke?.width>0)paintOutline(g,poly,joined?openLowerEdge(poly,joinedAt):path,{fill:'none',stroke:rgba(f.stroke.color),'stroke-width':f.stroke.width,'stroke-linejoin':'round','pointer-events':'none'});const order=positions.get(f.id);if(!presenting){label(g,f,(order?order+' · ':'')+f.title,12,palette.sub,false,camera,'frame-title');if(f.tags?.length)label(g,f,'\u2b26 '+f.tags.join(', '),10.5,palette.accent,true,camera,'frame-tags');}metrics.push({id:f.id,outline:poly,fill,order});}
for(const n of contents)content(world,n,palette,!presenting&&selected.has(n.id));if(presenting)return metrics;
for(const f of frames){const poly=metrics.find(m=>m.id===f.id).outline,joined=attached&&f===stripHost,included=positions.has(f.id);
 if(state.presentationHostId&&(joined||included||state.deckMode)) {
  const ring=deckHighlight(state,f.id,included,palette,joined);
  paintOutline(world,poly,joined?openLowerEdge(poly,joinedAt):paths.get(f.id),{fill:ring.hovered?palette.accent:'none','fill-opacity':.07,stroke:ring.stroke,
    'stroke-width':ring.width,'pointer-events':'none',class:joined&&!included&&!state.deckMode?'frame-deck-host':included?'frame-deck-member':'frame-deck-eligible'});
 }else if(selected.has(f.id)&&!state.propertyPanel)paintOutline(world,poly,paths.get(f.id),{fill:palette.select,'fill-opacity':.16*(f.locked ? .45:1),stroke:palette.select,'stroke-opacity':f.locked ? .45:1,'stroke-width':1.5,'pointer-events':'none',class:'frame-selection'});else if(!f.locked&&state.hoverId===f.id)paintOutline(world,poly,paths.get(f.id),{fill:'none',stroke:palette.select,'stroke-width':1.25,'stroke-opacity':.55*B().ease(state.hoverProgress??1),'pointer-events':'none',class:'frame-hover'});}
const hits=el('g',{class:'element-node frame-hit-fields'},svg);for(const f of frames){if(f.locked)continue;const g=screenGeom(f,camera);hitTargets(hits,f,camera,g);if(selected.has(f.id)&&!state.propertyPanel){if(g.controls.some((p,i)=>len(sub(p,g.corners[i]))>.5))el('polygon',{points:pts(g.controls),fill:'none',stroke:palette.select,'stroke-width':1,'stroke-opacity':g.resize*.45,'pointer-events':'none',class:'frame-compact-controls'},svg);filletGrip(hits,f,camera,g,palette,state);}}
if(state.deckMode){
 const badges=el('g',{class:'element-node',transform:'translate('+camera.x+' '+camera.y+') scale('+camera.zoom+')'},svg);
 for(const frame of frames) membershipBadge(badges,frame,positions.has(frame.id),state.deckHoverId===frame.id,palette);
}
if(stripHost){const f=stripHost;const chrome=el('g',{class:'element-node frame-selection-chrome',transform:'translate('+camera.x+' '+camera.y+') scale('+camera.zoom+')'},svg);propertyStrip(chrome,f,strip,palette,state);if(!state.propertyPanel&&!state.presentationHostId)dimensions(chrome,f,palette,camera,state,!strip.eligible);}
if(state.deckStroke?.length>1)el('polyline',{points:pts(state.deckStroke),fill:'none',stroke:palette.accent,'stroke-width':2,'pointer-events':'none',class:'frame-deck-stroke'},world);
if(state.marquee)el('rect',{x:state.marquee.x,y:state.marquee.y,width:state.marquee.w,height:state.marquee.h,fill:palette.select,'fill-opacity':.06,stroke:palette.select,'stroke-width':1/camera.zoom,'pointer-events':'none',class:'frame-marquee'},world);
return metrics;
}
root.FrameRenderer=Object.freeze({actions:ACTIONS,closedWidth:CLOSED_WIDTH,blisterWidths:Object.freeze({closed:BLISTER_CLOSED_WIDTH,open:BLISTER_OPEN_WIDTH}),render,textMetrics,screenGeom,dimensionGeometry,propertyGeometry,drawerGeometry,deckHighlight,slidePreview});
})(typeof window!=='undefined'?window:globalThis);

