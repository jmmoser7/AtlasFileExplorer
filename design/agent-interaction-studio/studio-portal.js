/* Phase views share the existing shell, tabs, document viewer and navigation owner. */
(() => {
  'use strict';
  const el=(tag,cls,text)=>{const e=document.createElement(tag);if(cls)e.className=cls;if(text!==undefined)e.textContent=text;return e;};
  const icons={image:'M3 4h18v16H3z M3 15l5-5 5 5 3-3 5 5 M16 8h.01',window:'M3 4h18v16H3z M3 9h18 M6 6.5h.01 M9 6.5h.01',component:'M3 7h18v10H3z M6 12h6 M16 10h3v4h-3z',canvas:'M6 12l12-7 M6 12l12 7 M6 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6 M18 2a3 3 0 1 0 0 6 3 3 0 0 0 0-6 M18 16a3 3 0 1 0 0 6 3 3 0 0 0 0-6',integration:'M3 4h18v16H3z M3 9h18 M9 9v11 M5 12h2 M5 15h2',package:'M3 6l9-4 9 4v12l-9 4-9-4z M3 6l9 5 9-5 M12 11v11 M7 4l10 5'};
  function icon(name){const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('aria-hidden','true');const p=document.createElementNS(svg.namespaceURI,'path');p.setAttribute('d',icons[name]);svg.append(p);return svg;}
  function action(text,callback,cls='portal-action'){const b=el('button',cls,text);b.type='button';b.addEventListener('click',callback);return b;}
  let homeBuilt=false,current=null;
  const root=document.getElementById('portal-phase'),home=document.getElementById('portal-home');
  const camera={x:0,y:0,zoom:1},cameras=new Map();
  const canvas=el('section','portal-native-canvas'),world=el('div','portal-native-world');canvas.id='portal-workspace';canvas.setAttribute('aria-label','Empty element workspace');canvas.append(world);canvas.hidden=true;
  const transform=()=>{world.style.transform=`translate(${camera.x}px,${camera.y}px) scale(${camera.zoom})`;if(current)cameras.set(current,{...camera});};
  StudioCanvasNavigation.bind({canvas,camera,transform,closeMenus:()=>{},deselect:()=>{}});
  function nativeLauncher(element='frame'){
    const endpoint=element==='video'?'/api/native-studio/video':'/api/native-studio';
    const group=el('div','portal-native-launch'),status=el('span','portal-subtitle');
    status.setAttribute('role','status');
    const button=action(element==='video'?'Launch Video Studio':'Launch Rust Studio',async()=>{
      button.disabled=true;status.textContent='Opening Rust Studio…';
      try{
        const response=await fetch(endpoint+'/launch',{method:'POST',headers:{'Content-Type':'application/json'},body:'{}'});
        let result;try{result=await response.json();}catch{throw new Error('Restart the local Studio server with python serve.py to enable launching.');}
        if(!response.ok)throw new Error(result.message||'Rust Studio could not be opened.');
        status.textContent=result.message;status.title=result.modifiedUtc?'Built artifact modified '+result.modifiedUtc:'';
      }catch(error){status.textContent=error.message||'The local Studio launcher is unavailable.';}
      finally{button.disabled=false;}
    });
    button.title='Open the latest built standalone Rust canvas specimen in its own window';
    group.append(button,status);
    fetch(endpoint).then(response=>response.json()).then(result=>{
      if(result.available){button.title+=' · built artifact modified '+result.modifiedUtc;}
      else{status.textContent=result.message;}
    }).catch(()=>{status.textContent='Start the local Studio server with python serve.py to enable launching.';});
    return group;
  }
  function homeView(navigate){
    if(homeBuilt)return;homeBuilt=true;
    home.append(el('h1','','Slate’s build process'));
    const ladder=el('div','build-ladder'),iterate=el('div','ladder-iterate','Iterate here');ladder.append(iterate);
    for(const p of StudioPortalModel.phases){
      const card=action('',()=>navigate(p.id),'phase-card');card.dataset.phase=p.id;
      const mark=el('div','phase-mark');mark.append(el('span','',p.id),icon(p.icon));
      card.append(mark,el('h2','',p.name),el('p','phase-caption',p.caption),el('p','phase-detail',p.detail));ladder.append(card);
    }
    ladder.append(el('div','ladder-production','Same production Rust UI code'));home.append(ladder);
    const foot=el('div','portal-home-foot');foot.append(el('p','','Open any stage. Move between stages whenever you need.'));
    const resume=action('Resume last workspace',()=>{let last;try{last=JSON.parse(localStorage.getItem('slate-studio-last-route-v1'));}catch{}if(last)navigate(last.phase,last.tab);});resume.id='portal-resume';resume.hidden=true;foot.append(resume);
    foot.append(nativeLauncher());home.append(foot);
  }
  function imageGallery(container,tab){
    const items=window.StudioPortalData?.images?.[tab.id]||[];
    if(!items.length){container.append(el('p','portal-empty','No reference images for this element yet.'));return;}
    const grid=el('div','portal-gallery');
    for(const item of items){
      const figure=el('figure','portal-image'),b=action('',()=>lightbox(item,tab),'portal-image-open'),img=el('img');img.src=item.src;img.alt=item.title||'Element reference';img.loading='lazy';b.append(img);figure.append(b,el('figcaption','',item.title||''),StudioImageFeedback.create({element:tab,image:item}));grid.append(figure);
    }
    container.append(grid);
  }
  function lightbox(item,tab){
    const dialog=el('dialog','portal-lightbox'),close=action('Close',()=>dialog.close()),img=el('img'),download=el('a','portal-action','Download image');img.src=item.src;img.alt=item.title||'Reference image';download.href=item.download||item.src;download.download=(item.src||'reference.png').split('/').at(-1);
    const toolbar=el('div','portal-lightbox-toolbar');toolbar.append(el('strong','',item.title||'Reference image'),download,close);dialog.append(toolbar,img);if(item.caption)dialog.append(el('p','',item.caption));dialog.append(StudioImageFeedback.create({element:tab,image:item}));document.body.append(dialog);dialog.addEventListener('close',()=>dialog.remove());dialog.addEventListener('click',e=>{if(e.target===dialog)dialog.close();});dialog.showModal();
  }
  const duration=s=>typeof s==='number'?s<60?`${s.toFixed(0)} s`:`${Math.floor(s/60)} min ${Math.round(s%60)} s`:'Not recorded';
  function table(columns,rows){const t=el('table','portal-time-table'),head=el('thead'),tr=el('tr');columns.forEach(c=>tr.append(el('th','',c)));head.append(tr);t.append(head);const body=el('tbody');rows.forEach(values=>{const r=el('tr');values.forEach(v=>{const cell=el('td');if(v instanceof Node)cell.append(v);else cell.textContent=String(v??'—');r.append(cell);});body.append(r);});t.append(body);return t;}
  function summary(container,phase,tab){
    const data=window.StudioPortalData?.summaries?.[phase.id]?.[tab.id];
    container.append(el('p','portal-subtitle','Read-only record of completed work. This stage is a time summary, not a builder.'));
    if(!data){container.append(el('p','portal-empty','No recorded time summary for this element yet.'));return;}
    if(data.note)container.append(el('p','portal-summary-note',data.note));
    if(data.entries?.length)container.append(table(['Work','Elapsed time','Evidence / scope'],data.entries.map(r=>[r.label,duration(r.seconds),r.detail])));
    if(data.records?.length){
      const details=el('details','portal-records');details.append(el('summary','',`Recorded intervals · ${data.records.length}`));
      const zone=Intl.DateTimeFormat().resolvedOptions().timeZone;
      details.append(el('p','portal-subtitle',`Times shown in ${zone}. Raw UTC timestamps remain in the download. Intervals can overlap; they are not added together.`));
      const local=value=>{const date=new Date(value);return Number.isFinite(date.getTime())?date.toLocaleString(undefined,{dateStyle:'medium',timeStyle:'medium'}):'Not recorded';};
      const evidence=record=>{
        const cell=el('div','portal-evidence');
        if(record.evidence){
          let url;try{url=new URL(record.evidence);}catch{}
          if(url&&['http:','https:'].includes(url.protocol)){const link=el('a','','View evidence');link.href=url.href;link.target='_blank';link.rel='noopener noreferrer';cell.append(link);}
          else cell.append(el('span','',typeof record.evidence==='string'?record.evidence:JSON.stringify(record.evidence)));
        }
        if(record.result)cell.append(el('p','',record.result));
        if(record.detail)cell.append(el('p','',record.detail));
        if(typeof record.compilerSeconds==='number')cell.append(el('p','',`Compiler: ${duration(record.compilerSeconds)}`));
        if(record.source)cell.append(el('small','',record.source));
        return cell;
      };
      details.append(table(['Record','Category / status',`Start → end (${zone})`,'Elapsed','Evidence'],data.records.map(r=>[r.label||r.id,`${r.category||''} · ${r.status||''}`,`${local(r.start)} → ${local(r.end)}`,duration(r.seconds),evidence(r)])));container.append(details);
    }
    if(data.unknownDurations)container.append(el('p','portal-subtitle',`Unmeasured time: ${Object.keys(data.unknownDurations).join(', ')}. Unknown durations remain unknown.`));
    container.append(action('Download time record',()=>{const url=URL.createObjectURL(new Blob([JSON.stringify(data,null,2)],{type:'application/json'}));const a=el('a');a.href=url;a.download=`${tab.id}-${phase.folder}-time.json`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}));
  }
  function render({state,phase,tab,navigate}){
    homeView(navigate);let last;try{last=JSON.parse(localStorage.getItem('slate-studio-last-route-v1'));}catch{}document.getElementById('portal-resume').hidden=!StudioPortalModel.phases.some(p=>p.id===last?.phase);canvas.hidden=true;document.querySelectorAll('.portal-lightbox').forEach(d=>d.close());
    if(!phase)return;
    root.replaceChildren();const heading=el('header','portal-phase-heading');heading.append(el('h1','',tab.name),el('span','',`${phase.id} · ${phase.name}`));root.append(heading);
    if(state.phase==='02'||state.phase==='03'||(state.phase==='01'&&tab.kind==='blank')){
      const specimen=state.phase==='03'&&window.StudioPortalData?.nativeCanvas?.[tab.id];
      if(specimen){
        current=null;
        const content=el('section','portal-content'),links=el('div','portal-audit-links');
        for(const [label,href] of [['Run and verification',specimen.report],['Native captures','?phase=00&tab='+tab.id]]){
          const link=el('a','portal-action',label);link.href=href;links.append(link);
        }
        links.prepend(nativeLauncher(tab.id));
        heading.append(el('p','portal-subtitle','Interactive specimen opens in its own Rust window.'));
        content.append(links);
        if(specimen.capture){const capture=el('img','portal-native-capture');capture.src=specimen.capture;capture.alt=specimen.alt||'Native '+tab.name+' canvas';content.append(capture);}
        else content.append(el('p','portal-empty','Native capture not recorded yet. Launch the specimen to exercise the real component.'));
        root.append(content);return;
      }
      const key=`${state.phase}:${tab.id}`;current=key;Object.assign(camera,cameras.get(key)||{x:0,y:0,zoom:1});transform();
      const native=state.phase!=='01';heading.append(el('p','portal-subtitle',native?'Empty native workspace · no native component has been populated for this element.':'Empty JavaScript workspace for this element.'));
      canvas.hidden=false;root.append(canvas);const help=el('div','portal-canvas-help','Middle / right drag or Space + drag: pan · Wheel: zoom');root.append(help);return;
    }
    current=null;const content=el('section','portal-content');root.append(content);
    if(state.phase==='00'){content.append(el('p','portal-subtitle','Reference images · choose an image to view or download.'));imageGallery(content,tab);}
    else if(state.phase==='04'||state.phase==='05')summary(content,phase,tab);
    else if(tab.id==='studio'){
      content.append(el('p','portal-subtitle','Studio workflow references and the global time audit.'),nativeLauncher());imageGallery(content,tab);
      const links=el('div','portal-audit-links');links.append(action('Integration time summary',()=>navigate('04','studio')),action('Release time summary',()=>navigate('05','studio')));content.append(links);
    }
  }
  window.StudioPortalView={render};
})();
