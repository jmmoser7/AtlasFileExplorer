/* Editable DOM/SVG presentation; no native service or generated screenshot overlays. */

window.createImageStudioView = function ({make, catalog, save, transient, beginDrag, showHistoryMenu, drawWires, menus, selectNode, submitPrompt, beginOutputDrag, inputCount}) {

  const photo='image-generator/assets/pavilion.png', ns='http://www.w3.org/2000/svg';

  let pickerNode=null;

  const rect=(el,r)=>Object.assign(el.style,{left:r[0]+'px',top:r[1]+'px',width:r[2]+'px',height:r[3]+'px'});

  function svg(tag,attrs={}) {const el=document.createElementNS(ns,tag);for(const [k,v] of Object.entries(attrs))el.setAttribute(k,v);return el;}

  function icon(name) {const def=ImageStudioIcons[name],el=svg('svg',{viewBox:(def.view_box||[0,0,24,24]).join(' '),'aria-hidden':'true'});el.append(svg('path',{d:def.path,fill:def.filled?'currentColor':'none',stroke:def.filled?'none':'currentColor','stroke-width':def.stroke_width||.85,'stroke-linecap':'round','stroke-linejoin':'round'}));return el;}

  function image() {const el=make('img');el.src=photo;el.draggable=false;el.alt='Timber pavilion surrounded by planting';return el;}

  function button(root,node,label,r,action,cls='image-control') {const el=make('button',cls);el.type='button';el.setAttribute('aria-label',`${node.label}: ${label}`);el.dataset.hint=label;rect(el,r);el.addEventListener('pointerdown',e=>e.stopPropagation());el.addEventListener('click',e=>{e.stopPropagation();action(el);});root.append(el);return el;}

  function thumb(node,index,cls) {const chosen=node.selectedResults||[node.result||0],el=make('button',cls+(chosen.includes(index)?' selected':''));el.setAttribute('aria-label',`${node.label}: Result ${index+1}`);el.setAttribute('aria-pressed',String(chosen.includes(index)));el.dataset.resultIndex=index;el.dataset.hint=`Result ${index+1} · Shift-click to select a range`;el.append(image());el.addEventListener('pointerdown',e=>e.stopPropagation());el.onclick=e=>{selectNode(node,false);if(e.shiftKey){const start=node.selectionAnchor??node.result??0;node.selectedResults=Array.from({length:Math.abs(index-start)+1},(_,i)=>Math.min(start,index)+i);}else{node.selectedResults=[index];node.selectionAnchor=index;}node.result=index;save();paint(node);};return el;}

  function history(root,node,r) {

    const group=make('div','image-history');rect(group,r);root.append(group);

    const count=node.history||0,gap=2,diameter=Math.min(18,Math.max(14,(r[2]-gap*(count-1))/count));

    group.style.gap=gap+'px';

    const capacity=Math.max(1,Math.floor((r[2]-32+gap)/(diameter+gap))),fits=count*diameter+gap*(count-1)<=r[2];

    const start=Math.min(node.historyStart||0,Math.max(0,count-capacity));

    function nav(delta){const b=make('button','history-nav',delta<0?'‹':'›');b.setAttribute('aria-label',`${node.label}: ${delta<0?'Earlier':'Later'} results`);b.disabled=delta<0?start===0:start+capacity>=count;b.onclick=()=>{node.historyStart=Math.max(0,Math.min(count-capacity,start+delta));paint(node);};b.onpointerdown=e=>e.stopPropagation();group.append(b);}

    if(!fits)nav(-1);for(let i=fits?0:start;i<(fits?count:Math.min(count,start+capacity));i++){const t=thumb(node,i,'image-history-thumb');t.style.width=t.style.height=diameter+'px';group.append(t);}if(!fits)nav(1);

  }

  function paint(node) {

    const root=node.element,focused=root.querySelector('textarea')===document.activeElement,selection=focused?[document.activeElement.selectionStart,document.activeElement.selectionEnd]:null;

    const previousDialogue=root.querySelector('.image-dialogue');if(previousDialogue&&root.dataset.revision===String(node.revision||0))node.dialogueScroll=previousDialogue.scrollTop;root.dataset.revision=String(node.revision||0);
    root.replaceChildren();Object.assign(root.style,{left:node.x+'px',top:node.y+'px',width:node.width+'px',height:node.height+'px'});

    const provider=catalog.providers.find(p=>p.id===node.config.provider)||catalog.providers[0],model=provider.models.find(m=>m.id===node.config.model)||provider.models[0];

    const modelLabel=provider.id==='openai-image'?model.label:`${provider.label} · ${model.label}`;

    const measure=document.createElement('canvas').getContext('2d');measure.font='300 11px ChatTrainUbuntu';

    const l=ImageStudioLayout.layout(node.width,node.height,node.connected,node.expanded,node.history,Math.ceil(measure.measureText(modelLabel).width)+11);node.layout={...l,zoom:1,ports:ImageStudioLayout.ports(node.width)};root.dataset.attached=String(l.attached);

    root.dataset.generatorOpen=String(node.expanded);root.dataset.panelOpen=String(node.expanded||(node.selected&&node.activeTool!=null));root.dataset.selected=String(node.selected);root.classList.toggle('is-locked',!!node.locked);

    const surfaceMask=`url(#mask-${node.id})`;
    const art=svg('svg',{class:'image-art',width:node.width,height:node.height,viewBox:`0 0 ${node.width} ${node.height}`});

    const defs=svg('defs'),mask=svg('mask',{id:`mask-${node.id}`,maskUnits:'userSpaceOnUse',x:-1,y:-1,width:node.width+2,height:node.height+2});mask.append(svg('rect',{width:node.width,height:node.height,rx:8,fill:'white'}));

    if(node.selected&&node.bodySelected!==false){for(const [i,p] of node.layout.ports.entries())if(!node.grafted?.[['image','text','style'][i]])mask.append(svg('path',{d:ImageStudioLayout.notch(node.width,p[1],i===3),fill:'black'}));}

    if(l.attached&&!node.expanded&&node.selected&&node.bodySelected!==false){const x=(node.width-l.bandWidth)/2,y=node.height;mask.append(svg('path',{d:`M${x} ${y} C${x+42} ${y} ${x+42} ${y-20} ${x+84} ${y-20} H${x+l.bandWidth-84} C${x+l.bandWidth-42} ${y-20} ${x+l.bandWidth-42} ${y} ${x+l.bandWidth} ${y} Z`,fill:'black'}));}

    defs.append(mask);art.append(defs,node.blank?svg('rect',{width:node.width,height:node.height,rx:8,fill:'var(--card)',stroke:'var(--studio-line)',mask:surfaceMask}):svg('image',{href:photo,width:node.width,height:node.height,preserveAspectRatio:'none',style:`filter:${node.result%3===1?'sepia(.18) saturate(.8)':node.result%3===2?'saturate(1.25) contrast(1.03)':'none'}`,mask:surfaceMask}));root.append(art);

      ['Primary image','Text','Style image','Output'].forEach((name,i)=>{const p=node.layout.ports[i],kind=['image','text','style'][i],graft=ImageGraft.port(inputCount(node,kind),!!node.grafted?.[kind]),port=button(root,node,name,[p[0]-6,p[1]-graft.height/2,12,graft.height],()=>transient(`${name} connection port`),'image-port port-'+i);if(kind){port.dataset.inputKind=kind;port.dataset.hint=`${name}${node.grafted?.[kind]?' · Grafted':''} · Right-click for input options`;port.classList.toggle('is-grafted',!!node.grafted?.[kind]);}if(i===3){port.dataset.hint='Drag to generate a downstream image. Preserved history follows automatically.';port.addEventListener('pointerdown',e=>beginOutputDrag(e,node));}const label=make('span','image-port-label',name);label.style.top=(p[1]-24)+'px';label.style[i===3?'left':'right']=(node.width+12)+'px';root.append(label);});

    if(node.selected&&node.bodySelected!==false) {



      const names=['Fill color','Stroke color and width','Corners and crop','Photo filters','Brush and lasso selection','Image results and paint layers','Generate or edit this image','Generate image'];

      const glyphs=['Fill','Ellipse','Corners','Sliders','Brush','Media','Agent','play'];

      l.buttons.forEach((r,i)=>{const b=button(root,node,names[i],r,()=>{if(i===7){transient('Generation is simulated in this Studio.');return;}if(i!==6){transient(`${names[i]} remains unchanged in Slate and is outside this specification.`);return;}node.generator=true;node.expanded=!node.expanded;node.activeTool=node.expanded?6:null;node.rememberedPanel=node.expanded?'agent':null;paint(node);save();},'image-tool '+(l.attached?'attached-tool':'detached-tool')+(i===6&&node.expanded?' active':''));b.append(glyphs[i]==='play'?make('span','image-play'):icon(glyphs[i]));if(i===6)b.setAttribute('aria-expanded',String(node.expanded));});

    }

    if(node.generator!==false&&node.expanded) {

      const panel=make('div','image-editor');panel.classList.toggle('is-detached',!l.attached);panel.style.left=l.panelX+'px';panel.style.top=l.panelY+'px';panel.style.width=l.panelWidth+'px';root.append(panel);

      const c=l.controls;

      function field(name,label,r,action) {const b=button(panel,node,label,r,action,'image-control image-field image-menu-trigger'+(name==='model'?' model-name':''));b.append(make('span','image-field-value',label),make('span','image-caret'));return b;}

      const modelButton=field('model',provider.id==='openai-image'?model.label:`${provider.label} · ${model.label}`,c.model,anchor=>{

        if(pickerNode===node.id&&document.querySelector('.agent-picker')){ChatAgentPicker.close();pickerNode=null;return;}

        menus.close();pickerNode=node.id;

        ChatAgentPicker.open({anchor,card:node.config,catalog,onChange:config=>{node.config=config;paint(node);save();},onClose:()=>pickerNode=null,onStatus:transient});

      });StudioGeneratorUI.modelLabel(modelButton,modelLabel,node.config,catalog);modelButton.setAttribute('aria-label',`${node.label}: Agent settings`);modelButton.dataset.hint='Provider, model, effort and speed. OpenAI API has a separate Add or Change key chevron.';

      const more=button(panel,node,'Image options',c.more,a=>menus.options(node,a),'image-control image-more image-menu-trigger');more.innerHTML='<i></i><i></i><i></i>';more.dataset.hint='Aspect Ratio, Mode and Settings';

      const play=button(panel,node,'Generate image',c.play,()=>submitPrompt(node),'image-control generate-image');play.append(make('span','image-play'));

      const count=StudioGeneratorUI.count(node.count,node.label,value=>{node.count=value;save();});rect(count,c.count);panel.append(count);

      history(panel,node,c.history);

      const shared=StudioGeneratorUI.conversation(panel,{interactive:!!node.interactive,turns:node.interactive?ImageConversation.turns(node):[],prompt:node.prompt,placeholder:node.interactive?'Message…':'Describe the picture. Submit makes it again.',label:node.label,promptLabel:node.label+': Image prompt',conversationLabel:node.label+': Conversation',scrollTop:node.dialogueScroll,onScroll:value=>node.dialogueScroll=value,onInput:value=>{node.prompt=value;save();},onSubmit:()=>submitPrompt(node)});
      if(focused){shared.composer.focus({preventScroll:true});shared.composer.setSelectionRange(...selection);}

    }

    const grid=ImageStudioLayout.resultGrid(node.width,node.history||0),results=make('div','image-results');results.style.top=(node.height+(node.selected&&node.bodySelected!==false?(l.attached?28:52):8))+'px';results.style.width=grid.width+'px';results.style.height=grid.height+'px';results.style.left=(node.width-grid.width)/2+'px';results.setAttribute('aria-label',`${node.label}: Hover results`);
    grid.positions.forEach((position,i)=>{const tile=thumb(node,i,'image-result-thumb');tile.style.left=position.x+'px';tile.style.top=position.y+'px';results.append(tile);});root.append(results);
    const bridge=make('div','image-hover-bridge');bridge.style.top=node.height+'px';bridge.style.height=((node.selected&&node.bodySelected!==false?(l.attached?28:52):8)+grid.height+12)+'px';root.append(bridge);
    for(const side of ['in','out'])if(node.historyEnabled&&node[side==='in'?'contextIn':'contextOut']){const b=button(root,node,`${side==='in'?'Incoming':'Outgoing'} chat history`,[side==='in'?2.5:node.width-9.5,6.5,7,7],anchor=>showHistoryMenu(node,side,anchor),'image-context-port');b.dataset.direction=side;if(side==='out')b.addEventListener('pointerdown',e=>beginOutputDrag(e,node));}

    const grip=make('div','native-size-grip');grip.setAttribute('role','separator');grip.setAttribute('aria-label',`${node.label}: Resize width`);grip.tabIndex=0;grip.onpointerdown=e=>beginDrag(e,node,'resize');grip.onkeydown=e=>{if(['ArrowLeft','ArrowRight'].includes(e.key)){e.preventDefault();node.width=Math.max(120,Math.min(1000,node.width+(e.key==='ArrowRight'?20:-20)));paint(node);save();}};root.append(grip);drawWires();

  }

  return paint;

};
