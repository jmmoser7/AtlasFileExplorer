/* Actual output miniatures. Geometry comes from the reviewed Images result grid. */
(() => {
  const make=(tag,cls)=>{const e=document.createElement(tag);e.className=cls;return e;};
  function thumbnail({node,index,size,render,onSelect,circle=false}){
    const button=make('button',circle?'element-revision':'element-result-thumb');button.type='button';button.dataset.resultIndex=index;button.setAttribute('aria-label','Revision '+(index+1));button.setAttribute('aria-pressed',String((node.selectedRevision??node.versions.length-1)===index));button.title='View option '+(index+1);
    button.style.width=button.style.height=size+'px';
    const scene=make('span','element-miniature');scene.setAttribute('aria-hidden','true');scene.inert=true;scene.style.width=node.width+'px';scene.style.height=node.height+'px';scene.style.transform=`translate(-50%,-50%) scale(${Math.max(size/node.width,size/node.height)})`;scene.append(render(node.versions[index],index));button.append(scene);
    button.onpointerdown=e=>e.stopPropagation();button.onclick=e=>{e.stopPropagation();onSelect(index,circle);};return button;
  }
  function history({node,width,render,onSelect}){
    const group=make('div','element-revisions');group.setAttribute('aria-label','Revision history');const count=node.versions.length,capacity=Math.max(1,Math.floor((width-32+2)/20)),fits=count*20-2<=width,start=Math.min(node.historyStart||0,Math.max(0,count-capacity));
    const nav=delta=>{const b=make('button','history-nav');b.type='button';b.textContent=delta<0?'‹':'›';b.setAttribute('aria-label',delta<0?'Earlier results':'Later results');b.disabled=delta<0?start===0:start+capacity>=count;b.onpointerdown=e=>e.stopPropagation();b.onclick=e=>{e.stopPropagation();node.historyStart=Math.max(0,Math.min(count-capacity,start+delta));group.dispatchEvent(new CustomEvent('page-results',{bubbles:true}));};group.append(b);};
    if(!fits)nav(-1);for(let i=fits?0:start;i<(fits?count:Math.min(count,start+capacity));i++)group.append(thumbnail({node,index:i,size:18,render,onSelect,circle:true}));if(!fits)nav(1);return group;
  }
  function grid({node,top,render,onSelect}){
    const geometry=ImageStudioLayout.resultGrid(node.width,node.versions.length),group=make('div','element-results');group.setAttribute('aria-label','Output previews');Object.assign(group.style,{top:top+'px',left:(node.width-geometry.width)/2+'px',width:geometry.width+'px',height:geometry.height+'px'});
    geometry.positions.forEach((position,index)=>{const b=thumbnail({node,index,size:24,render,onSelect});b.style.left=position.x+'px';b.style.top=position.y+'px';group.append(b);});
    const bridge=make('div','element-hover-bridge');Object.assign(bridge.style,{top:node.height+'px',height:(top-node.height+geometry.height+12)+'px'});return {group,bridge};
  }
  window.StudioResultPreviews={history,grid};
})();
