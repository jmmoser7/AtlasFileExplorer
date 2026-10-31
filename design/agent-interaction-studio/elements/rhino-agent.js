/* Rhino-facing element content. Shared Studio runtime owns agent chrome and execution. */
(() => {
  'use strict';
  const ns='http://www.w3.org/2000/svg';
  const defaults={yaw:-.62,pitch:.48,zoom:1,panX:0,panY:0};
  const script=`import Rhino
import rhinoscriptsyntax as rs
import scriptcontext as sc

# New geometry only; unrelated document objects are preserved.
created = []
undo = sc.doc.BeginUndoRecord("Build courtyard pavilion")
redraw = rs.EnableRedraw(False)
def box(x, y, z, width, depth, height):
    corners = [(x,y,z), (x+width,y,z), (x+width,y+depth,z), (x,y+depth,z),
               (x,y,z+height), (x+width,y,z+height),
               (x+width,y+depth,z+height), (x,y+depth,z+height)]
    object_id = rs.AddBox(corners)
    if object_id is None:
        raise RuntimeError("Rhino could not create a solid")
    created.append(object_id)
    return object_id

try:
    slab = box(-6, -4, 0, 12, 8, .25)
    for x in [-5.6, 5.3]:
        for y in [-3.6, 3.3]:
            box(x, y, .25, .3, .3, 3.8)
    box(-6, -3.8, 3.8, 12, .22, .25)
    box(-6, 3.6, 3.8, 12, .22, .25)
    for i in range(17):
        box(-6 + i * 12.0 / 16, -4, 4.05, .14, 8, .3)
    rs.ObjectName(slab, "Pavilion floor")
except Exception:
    if created:
        rs.DeleteObjects(created)
    raise
finally:
    sc.doc.EndUndoRecord(undo)
    rs.EnableRedraw(redraw)
    sc.doc.Views.Redraw()
print("Created {} pavilion objects".format(len(created)))`;

  function svg(tag,attrs={}){const e=document.createElementNS(ns,tag);for(const [k,v] of Object.entries(attrs))e.setAttribute(k,v);return e;}
  function renderPreview({node,make,update}){
    node.data ||= {};const data=node.data;
    data.camera ||= {...defaults};data.display ||= 'Shaded';data.view ||= 'Perspective';if(data.display==='Wireframe')data.display='Hidden line';
    const root=make('div','rhino-preview');root.setAttribute('aria-label','Rhino model preview');
    const viewport=make('div','rhino-preview-viewport');viewport.tabIndex=0;viewport.setAttribute('aria-label','3D viewport. Drag to orbit; Shift-drag to pan; focus then scroll to zoom.');
    viewport.dataset.hint='Drag to orbit · Shift-drag to pan · Scroll after selection to zoom';
    const scene=svg('svg',{viewBox:'0 0 760 350',role:'img','aria-label':'Timber pavilion with slab, columns and open slatted roof'});viewport.append(scene);
    const compass=make('div','rhino-preview-compass');compass.innerHTML='<span class="axis-z">Z</span><span class="axis-y">Y</span><span class="axis-x">X</span>';viewport.append(compass);root.append(viewport);
    function project(p){const c=data.camera,a=data.view==='Top'?0:data.view==='Front'?0:c.yaw,tilt=data.view==='Top'?Math.PI/2:data.view==='Front'?0:c.pitch;const x=p[0]*Math.cos(a)-p[1]*Math.sin(a),depth=p[0]*Math.sin(a)+p[1]*Math.cos(a),y=depth*Math.sin(tilt)-p[2]*Math.cos(tilt);return[380+x*23*c.zoom+c.panX,242+y*23*c.zoom+c.panY,depth*Math.cos(tilt)+p[2]*Math.sin(tilt)];}
    function points(ps){return ps.map(p=>project(p).slice(0,2).join(',')).join(' ');}
    function line(a,b,cls){scene.append(svg('line',{x1:project(a)[0],y1:project(a)[1],x2:project(b)[0],y2:project(b)[1],class:cls}));}
    function solid(x,y,z,w,d,h,label,color){const p=[[x,y,z],[x+w,y,z],[x+w,y+d,z],[x,y+d,z],[x,y,z+h],[x+w,y,z+h],[x+w,y+d,z+h],[x,y+d,z+h]];return{p,label,color,depth:project([x+w/2,y+d/2,z+h/2])[2]};}
    function draw(){
      scene.replaceChildren();
      for(let n=-12;n<=12;n++) {line([n,-12,0],[n,12,0],'rhino-grid');line([-12,n,0],[12,n,0],'rhino-grid');}
      line([-12,0,0],[12,0,0],'rhino-axis-x');line([0,-12,0],[0,12,0],'rhino-axis-y');
      const variant=data.variant||0,solids=[solid(-6,-4,0,12,8,.25,'Floor slab','#bdc7ca')];
      for(const x of [-5.6,5.3])for(const y of [-3.6,3.3])solids.push(solid(x,y,.25,.3,.3,3.8,'Timber column','#b58b62'));
      const ribs=variant%2?17:13;for(let n=0;n<ribs;n++)solids.push(solid(-6+n*12/(ribs-1),-4,4.05,.14,8,.3,'Roof slat '+(n+1),'#c8a577'));
      solids.push(solid(-6,-3.8,3.8,12,.22,.25,'Edge beam','#aa825c'),solid(-6,3.6,3.8,12,.22,.25,'Edge beam','#aa825c'));
      if(variant>1)solids.push(solid(-5,-3,.25,4,.7,.45,'Low seating','#ae926e'));
      solids.sort((a,b)=>b.depth-a.depth);
      for(const item of solids){const group=svg('g',{'data-object-name':item.label,tabindex:'0',role:'button','aria-label':'Select '+item.label,class:'rhino-solid'+(data.selected===item.label?' selected':'')});
        for(const [idx,face] of [[0,[0,1,5,4]],[1,[1,2,6,5]],[2,[3,2,6,7]],[3,[0,3,7,4]],[4,[4,5,6,7]]]){
          const poly=svg('polygon',{points:points(face.map(i=>item.p[i])),fill:data.display==='Arctic'||data.display==='Hidden line'?'#ececea':data.display==='Z-buffer'?`hsl(0 0% ${Math.max(20,Math.min(90,65-item.depth*2))}%)`:data.display==='Material mask'?(item.label==='Floor slab'?'#577898':item.label.startsWith('Roof')?'#ac8869':'#698b74'):item.color,'fill-opacity':idx===4?1:.76,stroke:data.selected===item.label?'#198c92':'#657072','stroke-width':data.selected===item.label?1.7:.65});group.append(poly);
        }
        const select=()=>{data.selected=item.label;draw();};group.onclick=e=>{e.stopPropagation();if(!drag?.moved)select();};group.onkeydown=e=>{if(e.key==='Enter'){e.preventDefault();select();}};scene.append(group);
      }
      viewport.setAttribute('aria-description',data.selected?data.selected+' selected':data.view+' viewport');
    }
    let drag=null;
    viewport.onpointerdown=e=>{if(e.button!==0)return;e.stopPropagation();viewport.focus({preventScroll:true});drag={x:e.clientX,y:e.clientY,moved:false,pan:e.shiftKey,object:e.target.closest('[data-object-name]')?.getAttribute('data-object-name'),camera:{...data.camera}};viewport.setPointerCapture(e.pointerId);};
    viewport.onpointermove=e=>{if(!drag)return;e.stopPropagation();const bounds=viewport.getBoundingClientRect(),scale=760/bounds.width,dx=(e.clientX-drag.x)*scale,dy=(e.clientY-drag.y)*scale;if(Math.hypot(dx,dy)>3)drag.moved=true;if(!drag.moved)return;if(drag.pan){data.camera.panX=drag.camera.panX+dx;data.camera.panY=drag.camera.panY+dy;}else if(data.view==='Perspective'){data.camera.yaw=drag.camera.yaw-dx*.008;data.camera.pitch=Math.max(.08,Math.min(1.5,drag.camera.pitch+dy*.008));}draw();};
    viewport.onpointerup=e=>{if(viewport.hasPointerCapture(e.pointerId))viewport.releasePointerCapture(e.pointerId);if(drag&&!drag.moved&&drag.object)data.selected=drag.object;if(drag?.moved)e.preventDefault();drag=null;update();};viewport.onpointercancel=()=>drag=null;
    viewport.addEventListener('wheel',e=>{if(e.ctrlKey||document.activeElement!==viewport)return;e.stopPropagation();e.preventDefault();data.camera.zoom=Math.min(3,Math.max(.35,data.camera.zoom*Math.exp(-e.deltaY*.0015)));draw();},{passive:false});
    draw();return root;
  }
  window.StudioElements ||= [];
  window.StudioElements.push({
    id:'rhino-agent',title:'Rhino 3D',width:760,height:420,
    initialPrompt:'Refine the canopy with closer-spaced timber slats and keep the current dimensions.',
    initialTurns:[{role:'user',text:'Create a timber pavilion, 12 by 8 meters, with an open slatted roof.'},{role:'assistant',text:'The preview contains a floor slab, four columns, two edge beams and thirteen roof slats. The model stays centered at the origin.'}],
    stages:[
      {label:'Inspect model',detail:'Reading model units, selected objects and the current bounding box.',code:'# Units: meters\n# Model bounds: 12 × 8 × 4.35\n# Scope: pavilion objects only'},
      {label:'Prepare geometry',detail:'Preparing a dimensioned slab, columns, edge beams and roof slats.',code:'width, depth, roof_z = 12.0, 8.0, 4.05\nslat_count = 17\nspacing = width / (slat_count - 1)'},
      {label:'Write Rhino script',detail:'Building scoped Rhino Python commands with one undo record.',code:script},
      {label:'Check geometry',detail:'Checking positive dimensions, closed box solids and retained document objects.',code:'# Check: 17 roof slats\n# Check: closed solids\n# Check: no unrelated object deletion'},
      {label:'Update preview',detail:'Replacing the preview revision and retaining the prior model snapshot.',code:'print("Preview updated; Rhino connection is simulated in this Studio.")'}
    ],
    renderPreview,
    applyResult(node,prompt){node.data ||= {};node.data.variant=(node.data.variant||0)+1;node.data.selected=null;node.data.lastInstruction=prompt;},
    nativeTools:[
      {id:'model-display',label:'Viewport display',icon:'Sliders',hint:'Native model display modes.',choices:['Shaded','Arctic','Hidden line','Material mask','Z-buffer'],action(node,value){node.data ||= {};node.data.display=value||'Shaded';}},
      {id:'model-measure',label:'Measure: pick two points on the model',icon:'Ruler',hint:'Native measurement control; measurement execution is outside this preview.',action(node){node.data ||= {};node.data.activeTool='measure';}},
      {id:'model-capture',label:'Capture: still image or video to File or Canvas',icon:'Media',hint:'Native capture control; export is outside this preview.',action(node){node.data ||= {};node.data.activeTool='capture';}},
      {id:'model-maximize',label:'Maximize / restore model portal',icon:'Corners',hint:'Native model portal sizing.',action(node){node.data ||= {};node.data.maximized=!node.data.maximized;}},
      {id:'filter',label:'Photo filters',icon:'Sliders',action(node){node.data ||= {};node.data.activeTool='filter';}},
      {id:'model-connector',label:'Rhino.Inside: switch native / Slate 3D',icon:'Rhino',hint:'The production control switches the host surface. This preview does not launch Rhino.',action(node){node.data ||= {};node.data.nativeRequested=!node.data.nativeRequested;}}
    ],
    tools:[{id:'fit-model',label:'Fit model',hint:'Restore the model camera without changing geometry.',action(node){node.data ||= {};node.data.camera={...defaults};}}]
  });
})();
