/* Frame source datum 2026-10-31; no native raster acceptance implied. */
(function(root,factory){if(typeof module==='object'&&module.exports)module.exports=factory();else root.FrameBaseline=factory();})(typeof globalThis==='object'?globalThis:this,function(){
'use strict';
const tokens={
frame:{radius:8,strokeWidth:0,titleSize:12,tagSize:10.5,labelInset:2,labelLift:6,shadow:false,minDraw:8,creationDragThreshold:4,selectionDragThreshold:6,presets:{Letter:{w:612,h:792},Tabloid:{w:1224,h:792},Wide169:{w:960,h:540}}},
selection:{lineWeight:1.5,opacity:1,fillOpacity:.16,hoverLineWeight:1.25,hoverOpacity:.55,highlightIn:.12,highlightOut:.22,lockedOpacity:.45,filletHalfSize:1.6,filletMinInset:10},
strip:{buttonSize:30,buttonGap:7,objectGap:14,stringerGap:24,bottomStringerGap:68,fadeSeconds:.12},
hit:{cornerReach:7,edgeReach:6,rotateReach:9,rotateOffset:18,filletReach:9,wireReach:8,minimumReach:2,moveSize:18,compactFrame:72,cursorRange:42,cursorGain:.65,resizeRange:[12,48],rotateRange:[3,16],wireRange:[36,88]},
navigation:{minZoom:1/128,maxZoom:128,defaultZoom:.8,wheelCoefficient:.0015},
snap:{rotationStep:45,rotationTolerance:4,smartGuideReach:9,grid:20},
text:{minimumRenderPx:4,fontFamily:'FrameUbuntu',fontWeight:300,fontAsset:'../chat-train/assets/Ubuntu-Light.ttf',fontSha256:'80307b8da7649aa4ee4d484b232140e3ce1ec0ca093073d3c53c8f5a5ced7a70'},
themes:{light:{bg:'#f6f7f8',grid:'#dfe3e7',card:'#ffffff',ink:'#1b1e22',sub:'#878e96',select:'#1f6fb2',accent:'#0f766e',border:'#dfe3e8'},dark:{bg:'#0e1013',grid:'#23272d',card:'#1c2026',ink:'#dde2e8',sub:'#878e96',select:'#6fb7ff',accent:'#2dd4bf',border:'#333941'}}
};
function deepFreeze(o){Object.values(o).forEach(v=>{if(v&&typeof v==='object')deepFreeze(v);});return Object.freeze(o);}deepFreeze(tokens);
const radians=d=>d*Math.PI/180,degrees=r=>r*180/Math.PI,add=(a,b)=>[a[0]+b[0],a[1]+b[1]],sub=(a,b)=>[a[0]-b[0],a[1]-b[1]],mul=(a,k)=>[a[0]*k,a[1]*k],length=a=>Math.hypot(...a),unit=a=>length(a)>1e-6?mul(a,1/length(a)):[0,0];
function center(r){return[r.x+r.w/2,r.y+r.h/2];}
function rotatePoint(p,c,deg){const a=radians(deg),s=Math.sin(a),co=Math.cos(a),d=sub(p,c);return[c[0]+d[0]*co-d[1]*s,c[1]+d[0]*s+d[1]*co];}
function corners(r,deg=0){const c=center(r);return[[r.x,r.y],[r.x+r.w,r.y],[r.x+r.w,r.y+r.h],[r.x,r.y+r.h]].map(p=>rotatePoint(p,c,deg));}
function upperEdge(r,deg=0){const c=corners(r,deg);let index=0,best=unit(sub(c[1],c[0]))[0];for(let i=1;i<4;i++){const x=unit(sub(c[(i+1)%4],c[i]))[0];if(x>best+1e-4){index=i;best=x;}}const a=c[index],b=c[(index+1)%4];let tangent=unit(sub(b,a));if(Math.abs(tangent[1])<1e-3)tangent=[1,0];return{a,b,index,center:mul(add(a,b),.5),tangent,outward:[tangent[1],-tangent[0]],angle:degrees(Math.atan2(tangent[1],tangent[0]))};}
function labelPlacement(r,deg,textWidth,textHeight,atRight=false){const e=upperEdge(r,deg),run=mul(e.tangent,tokens.frame.labelInset+textWidth/2),rise=mul(e.outward,tokens.frame.labelLift+textHeight/2);return{center:add(atRight?sub(e.b,run):add(e.a,run),rise),angle:e.angle};}
function wrapRotation(d){while(d>180)d-=360;while(d< -180)d+=360;return d;}
function snapRotation(d,alt=false){if(!alt){const snapped=(d<0?-Math.round(-d/45):Math.round(d/45))*45;if(Math.abs(wrapRotation(d-snapped))<=4)d=snapped;}return wrapRotation(d);}
function smoothstep(lo,hi,v){const t=Math.max(0,Math.min(1,(v-lo)/(hi-lo)));return t*t*(3-2*t);}
function prominence(shortest){if(!Number.isFinite(shortest))return{wire:0,resize:0,rotate:0};return{wire:smoothstep(36,88,shortest),resize:smoothstep(12,48,shortest),rotate:smoothstep(3,16,shortest)};}
function cursorReach(base,distance){return base*(1+.65*(1-smoothstep(0,42,distance)));}
function selectionGeometry(r,deg=0,zoom=1){const actual=corners(r,deg).map(p=>mul(p,zoom)),shortest=Math.min(r.w,r.h)*zoom,p=prominence(shortest),pad=Math.max(0,(72-shortest)/2)*p.resize,x=unit(sub(actual[1],actual[0])),y=unit(sub(actual[3],actual[0])),control=[sub(actual[0],mul(add(x,y),pad)),add(actual[1],mul(sub(x,y),pad)),add(actual[2],mul(add(x,y),pad)),add(actual[3],mul(sub(y,x),pad))],c=mul(add(control[0],control[2]),.5),radius=Math.max(18*p.rotate,12);return{corners:actual,controlCorners:control,projectedShortest:shortest,prominence:p,padding:pad,rotationRadius:radius,rotatePoints:control.map(v=>add(v,mul(unit(sub(v,c)),radius)))};}
function cornerOutline(r,corner={kind:'rounded',radius:8},zoom=1,squareCorners){
const kind=corner.kind||'rounded',percent=corner.percent===true||kind.includes('percent');let amount=corner.radius??corner.cut??corner.amount??(typeof corner.percent==='number'?corner.percent:0);if(percent)amount=Math.min(r.w,r.h)*Math.min(amount,100)/200;const radius=kind==='square'?0:Math.max(0,Math.min(amount,r.w/2,r.h/2));if(!radius)return corners(r,0);
const x=r.x,y=r.y,right=x+r.w,bottom=y+r.h,ends=[[right-radius,y],[right,y+radius],[right,bottom-radius],[right-radius,bottom],[x+radius,bottom],[x,bottom-radius],[x,y+radius],[x+radius,y]],out=[];
const vertices=corners(r,0),cs=[[right-radius,y+radius],[right-radius,bottom-radius],[x+radius,bottom-radius],[x+radius,y+radius]],tolerance=.25/zoom,angle=2*Math.acos(1-Math.min(tolerance/radius,1)),steps=kind.startsWith('chamfer')?1:Math.max(1,Math.ceil((Math.PI/2)/Math.max(.0001,angle)));
for(let q=0;q<4;q++){const vertex=(q+1)%4;if(squareCorners?.has(vertex)){out.push(vertices[vertex]);continue;}out.push(ends[q*2]);for(let i=1;i<steps;i++){const a=(q-1+i/steps)*Math.PI/2;out.push([cs[q][0]+radius*Math.cos(a),cs[q][1]+radius*Math.sin(a)]);}out.push(ends[q*2+1]);}
const unique=out.filter((p,i)=>!i||p[0]!==out[i-1][0]||p[1]!==out[i-1][1]);if(unique.length>1&&unique[0][0]===unique.at(-1)[0]&&unique[0][1]===unique.at(-1)[1])unique.pop();return unique;
}
const handles=['NW','N','NE','E','SE','S','SW','W'];
function handlePoint(r,h){const i=typeof h==='number'?h:handles.indexOf(String(h).toUpperCase()),c=center(r);return[[r.x,r.y],[c[0],r.y],[r.x+r.w,r.y],[r.x+r.w,c[1]],[r.x+r.w,r.y+r.h],[c[0],r.y+r.h],[r.x,r.y+r.h],[r.x,c[1]]][i<0?7:i];}
function resizeRect(before,deg,handle,delta,mods={}){
const h=typeof handle==='number'?handle:handles.indexOf(String(handle).toUpperCase());if(h<0||h>7)throw Error('Unknown handle');const c=center(before),start=rotatePoint(handlePoint(before,h),c,deg),world=add(start,Array.isArray(delta)?delta:[delta.x,delta.y]),p=rotatePoint(world,c,-deg),min=Math.max(Number.EPSILON,Math.min(8,before.w,before.h)),aspect=Math.max(.001,before.w/Math.max(.001,before.h)),lock=h%2===0?!mods.shift:!!mods.shift,fromCenter=!!(mods.ctrl||mods.control);let r;
if(fromCenter){let hw=Math.max(Math.abs(p[0]-c[0]),min/2),hh=Math.max(Math.abs(p[1]-c[1]),min/2);if(lock){if(hw/aspect>hh)hh=hw/aspect;else hw=hh*aspect;}return{x:c[0]-hw,y:c[1]-hh,w:hw*2,h:hh*2};}
if(h%2===0){const a=handlePoint(before,(h+4)%8);let w=Math.max(min,(h===0||h===6)?a[0]-p[0]:p[0]-a[0]),ht=Math.max(min,(h===0||h===2)?a[1]-p[1]:p[1]-a[1]);if(lock){if(w/aspect>ht)ht=w/aspect;else w=ht*aspect;}r={x:(h===0||h===6)?a[0]-w:a[0],y:(h===0||h===2)?a[1]-ht:a[1],w,h:ht};}
else if(h===1||h===5){const ht=Math.max(min,h===1?before.y+before.h-p[1]:p[1]-before.y),w=lock?Math.max(min,ht*aspect):before.w;r={x:before.x+(before.w-w)/2,y:h===1?before.y+before.h-ht:before.y,w,h:ht};}
else{const w=Math.max(min,h===7?before.x+before.w-p[0]:p[0]-before.x),ht=lock?Math.max(min,w/aspect):before.h;r={x:h===7?before.x+before.w-w:before.x,y:before.y+(before.h-ht)/2,w,h:ht};}
const oldAnchor=rotatePoint(handlePoint(before,(h+4)%8),c,deg),newAnchor=rotatePoint(handlePoint(r,(h+4)%8),center(r),deg),offset=sub(oldAnchor,newAnchor);r.x+=offset[0];r.y+=offset[1];return r;
}
function placeRect(start,end,preset=tokens.frame.presets.Letter,shift=false){const dx=end[0]-start[0],dy=end[1]-start[1];if(shift){const side=Math.max(Math.abs(dx),Math.abs(dy));return{x:Math.min(start[0],end[0]),y:Math.min(start[1],end[1]),w:side,h:side};}const w=Math.max(8,Math.abs(dx)),h=Math.max(8,w/(preset.w/preset.h));return{x:dx<0?start[0]-w:start[0],y:dy<0?start[1]-h:start[1],w,h};}
function ease(t){t=Math.max(0,Math.min(1,t));return t<.5?4*t*t*t:1-Math.pow(-2*t+2,3)/2;}
function hexRgba(hex){return[parseInt(hex.slice(1,3),16),parseInt(hex.slice(3,5),16),parseInt(hex.slice(5,7),16),255];}
function fillFor(frame,theme='light'){return frame.fill_authored?frame.fill.slice():hexRgba(tokens.themes[theme].card);}
function frameAt(frames,x,y){for(let i=frames.length-1;i>=0;i--){const f=frames[i],r=f.rect;const c=corners(r,f.rotation_deg||0),xs=c.map(p=>p[0]),ys=c.map(p=>p[1]);if(x>=Math.min(...xs)-1e-6&&x<=Math.max(...xs)+1e-6&&y>=Math.min(...ys)-1e-6&&y<=Math.max(...ys)+1e-6&&x>=r.x&&y>=r.y&&x<=r.x+r.w&&y<=r.y+r.h)return f.id;}return null;}
function membersOf(state,id){return(state.contents||[]).filter(n=>{const c=center(n.rect);return frameAt(state.frames,c[0],c[1])===id;}).map(n=>n.id);}
function nearestSegment(p,a,b){const d=sub(b,a),den=d[0]*d[0]+d[1]*d[1],v=sub(p,a),t=den>1e-8?Math.max(0,Math.min(1,(v[0]*d[0]+v[1]*d[1])/den)):0;return add(a,mul(d,t));}
function chromePick(point,r,deg=0,zoom=1,allowRotate=true){
const p=mul(Array.isArray(point)?point:[point.x,point.y],zoom),g=selectionGeometry(r,deg,zoom),actual=g.corners,cc=g.controlCorners,c=mul(add(actual[0],actual[2]),.5),d=sub(p,c),x=sub(actual[1],actual[0]),y=sub(actual[3],actual[0]),ux=unit(x),uy=unit(y),dot=(a,b)=>a[0]*b[0]+a[1]*b[1];
if(Math.abs(dot(d,ux))<Math.min(9,length(x)/3)&&Math.abs(dot(d,uy))<Math.min(9,length(y)/3))return null;
let best=null;const consider=(action,handle,distance,base,prominence,offset,weight)=>{const resting=base*prominence;if(resting<2)return;const normalized=distance/cursorReach(resting,distance);if(normalized>1)return;const score=offset+weight*normalized;if(!best||score<best.score)best={action,handle,score};};
for(let i=0;i<4;i++)consider('resize',handles[i*2],length(sub(p,cc[i])),7,g.prominence.resize,.06,.6);
for(let i=0;i<4;i++)consider('resize',handles[i*2+1],length(sub(p,nearestSegment(p,cc[i],cc[(i+1)%4]))),6,g.prominence.resize,.65,.25);
if(allowRotate){const controlCenter=mul(add(cc[0],cc[2]),.5);for(let i=0;i<4;i++){const outward=sub(cc[i],controlCenter),bearing=Math.atan2(outward[1],outward[0]),v=sub(p,cc[i]),a=Math.atan2(v[1],v[0]),delta=Math.atan2(Math.sin(a-bearing),Math.cos(a-bearing)),ang=bearing+Math.max(-Math.PI/4,Math.min(Math.PI/4,delta)),near=add(cc[i],[Math.cos(ang)*g.rotationRadius,Math.sin(ang)*g.rotationRadius]);consider('rotate',handles[i*2],length(sub(p,near)),9,g.prominence.rotate,.15,.6);}}
return best;
}
function metricReport(state,camera={x:0,y:0,zoom:1},theme='light'){
return{schema:'slate-frame-metrics/v1',units:'world-units-and-screen-pixels',theme,camera:{x:camera.x||0,y:camera.y||0,zoom:camera.zoom},tokens,frames:state.frames.map(f=>{const deg=f.rotation_deg||0,e=upperEdge(f.rect,deg),g=selectionGeometry(f.rect,deg,camera.zoom),corner=f.corner||{kind:'square',radius:0},radius=corner.kind==='square'?0:Math.min(Math.max(0,(corner.percent===true||String(corner.kind).includes('percent'))?(corner.radius||corner.amount||0)*Math.min(f.rect.w,f.rect.h)/200:(corner.radius||corner.cut||0)),f.rect.w/2,f.rect.h/2);return{id:f.id,rect:{...f.rect},rotation_deg:deg,order:f.order,title:f.title,corners:corners(f.rect,deg),upperEdge:[e.a,e.b],titleAngle:e.angle,titleSizeScreen:12*camera.zoom,titleVisible:12*camera.zoom>=4,fill:fillFor(f,theme),cornerKind:corner.kind,cornerRadius:radius,silhouette:cornerOutline(f.rect,corner,camera.zoom).map(p=>rotatePoint(p,center(f.rect),deg)),labelProbe:labelPlacement(f.rect,deg,90,20),strokeWidth:f.stroke?f.stroke.width:0,shadow:false,selected:(state.selectedIds||[]).includes(f.id),members:membersOf(state,f.id),selection:g};}),deckOrder:state.frames.filter(f=>!f.hidden).slice().sort((a,b)=>a.order-b.order||String(a.id).localeCompare(String(b.id),undefined,{numeric:true})).map(f=>f.id)};
}
return{tokens,center,rotatePoint,corners,upperEdge,labelPlacement,snapRotation,wrapRotation,smoothstep,prominence,cursorReach,selectionGeometry,cornerOutline,handlePoint,resizeRect,placeRect,ease,fillFor,frameAt,membersOf,chromePick,metricReport,metrics:metricReport};
});
