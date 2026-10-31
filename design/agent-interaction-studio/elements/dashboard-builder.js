/* Arbitrary completed HTML artifact content. This is not Slate editor chrome.
   Native host/editor owners: board_agent/artifact/editor.rs and generator.rs. */
(() => {
  'use strict';
  const source=`<main class="dashboard">
  <h1>Project overview</h1>
  <section aria-label="Summary metrics"></section>
  <svg aria-label="Completed work by month"></svg>
</main>`;
  const data=node=>{node.data||={};node.data.variant||=0;node.data.appliedVariant||=0;return node.data;};
  function svg(tag,attrs={}){const el=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const [k,v]of Object.entries(attrs))el.setAttribute(k,v);return el;}
  function renderPreview({node,make}){
    const d=data(node),root=make('main','dashboard-preview');root.setAttribute('aria-label','Sample generated HTML artifact');
    root.style.setProperty('--dashboard-accent',d.palette==='blue'?'#557caa':'#648c81');
    root.append(make('h1','','Project overview'));
    const metrics=make('section','dashboard-preview-metrics');metrics.setAttribute('aria-label','Summary metrics');
    [['Completed',128+d.variant*9],['In progress',24-d.variant%5],['On schedule',(91+d.variant%5)+'%']].forEach(([name,value])=>{const item=make('div','dashboard-preview-metric');item.append(make('span','',name),make('strong','',String(value)));metrics.append(item);});root.append(metrics);
    root.append(make('h2','','Completed work'));
    const chart=svg('svg',{viewBox:'0 0 620 170',role:'img','aria-label':'Sample monthly completed work'});
    for(const y of [20,60,100,140])chart.append(svg('path',{d:`M18 ${y}H606`,class:'dashboard-preview-grid'}));
    [17,24,21,33,38,49].forEach((value,i)=>{const h=(value+d.variant*(i%3))*2,x=49+i*94;chart.append(svg('rect',{x,y:140-h,width:38,height:h,rx:3,class:'dashboard-preview-bar'}));const t=svg('text',{x:x+19,y:162,'text-anchor':'middle'});t.textContent=['May','Jun','Jul','Aug','Sep','Oct'][i];chart.append(t);});root.append(chart);return root;
  }
  window.StudioElements||=[];
  window.StudioElements.push({
    id:'dashboard-builder',title:'Dashboard Builder',width:760,height:420,
    initialPrompt:'',initialTurns:[],promptHint:'Describe the dashboard or the next change',
    propertyTools:[
      {id:'fill',label:'Fill',icon:'Fill'},
      {id:'stroke',label:'Stroke',icon:'Ellipse'},
      {id:'agent',label:'Agent: write or picture from this',icon:'Agent'},
      {id:'capture',label:'Capture: still image or video to File or Canvas',icon:'Media'}
    ],
    stages:[
      {label:'Reading dashboard',detail:'Reading the current artifact and explicit inputs.',code:'const current = readArtifact();'},
      {label:'Building artifact',detail:'Preparing an HTML revision without changing the applied artifact.',code:source},
      {label:'Checking preview',detail:'Checking the generated page and revision.',code:'validateArtifact(preview);'},
      {label:'Preview ready',detail:'Use Settings to Apply or Discard the revision.',code:'stageArtifact(preview);'}
    ],renderPreview,
    applyResult(node,prompt){const d=data(node);d.variant++;d.pending=true;if(/blue/i.test(prompt))d.palette='blue';},
    tools:[
      {id:'apply-preview',label:'Apply preview',hint:'Apply the staged artifact revision.',action(node){const d=data(node);d.appliedVariant=d.variant;d.pending=false;}},
      {id:'discard-preview',label:'Discard preview',hint:'Restore the applied artifact revision.',action(node){const d=data(node);d.variant=d.appliedVariant;d.pending=false;}}
    ]
  });
})();
