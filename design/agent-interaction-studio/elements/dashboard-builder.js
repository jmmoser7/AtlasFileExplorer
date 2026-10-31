/* Dashboard content specimen. Shared agent controls are supplied by element-studios.js. */
(() => {
  'use strict';
  const months=['May','Jun','Jul','Aug','Sep','Oct'];
  const source=`const dashboard = {
  title: 'Project overview',
  period: 'May–October 2026',
  metrics: ['Completed', 'In progress', 'On schedule'],
  sections: ['Completed by month', 'Work by team', 'Upcoming milestones']
};
renderDashboard(dashboard);`;
  function data(node){
    node.data||={};
    return Object.assign(node.data,{variant:node.data.variant||0,appliedVariant:node.data.appliedVariant||0});
  }
  function svg(tag,attrs={}){const n=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const [key,val] of Object.entries(attrs))n.setAttribute(key,val);return n;}
  function button(make,label,hint,action,cls=''){const b=make('button',cls,label);b.type='button';b.title=hint;b.setAttribute('aria-label',label);b.onclick=e=>{e.stopPropagation();action();};return b;}
  function renderPreview({node,make,update}){
    const d=data(node),v=d.variant,root=make('div','dashboard-preview');
    root.style.setProperty('--dashboard-accent',d.palette==='blue'?'#557caa':'#648c81');
    const header=make('header','dashboard-preview-header'),title=make('div','dashboard-preview-heading');title.append(make('span','dashboard-preview-mark',''),make('strong','','Project overview'));
    const period=make('span','dashboard-preview-period',d.period==='month'?'October 2026':'May – October 2026');
    header.append(title,period);root.append(header);
    const nav=make('nav','dashboard-preview-nav');nav.setAttribute('aria-label','Dashboard view');
    for(const [id,label] of [['overview','Overview'],['milestones','Milestones'],['team','Team']]){const b=button(make,label,`Show ${label.toLowerCase()} on this dashboard`,()=>{d.view=id;update();});b.setAttribute('aria-pressed',String((d.view||'overview')===id));nav.append(b);}
    const controls=make('div','dashboard-preview-controls');
    controls.append(button(make,d.period==='month'?'This month':'Six months','Switch the sample reporting period',()=>{d.period=d.period==='month'?'half':'month';update();},'dashboard-preview-period-button'));
    const rev=make('span','dashboard-preview-revision',d.pending?'Preview ready':`Revision ${v+1}`);rev.title=d.pending?'Generated changes are staged until applied':'Current dashboard revision';controls.append(rev);nav.append(controls);root.append(nav);
    const body=make('div','dashboard-preview-body');root.append(body);
    const pick=(el,name)=>{el.tabIndex=0;el.setAttribute('role','button');el.setAttribute('aria-label',`Select ${name} widget`);el.title=`Select ${name} for the next instruction`;if(d.selectedWidget===name)el.classList.add('is-selected');const select=e=>{e.stopPropagation();d.selectedWidget=name;update();};el.onclick=select;el.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();select(e);}};return el;};
    const values=d.period==='month'?[31+v,6,96]:[128+v*9,24-v%5,91+v%5];
    const metricNames=['Completed','In progress','On schedule'];
    const metrics=make('div','dashboard-preview-metrics');metricNames.forEach((name,i)=>{const metric=pick(make('section','dashboard-preview-metric'),name);metric.append(make('span','dashboard-preview-metric-label',name),make('strong','dashboard-preview-metric-number',`${values[i]}${i===2?'%':''}`),make('span','dashboard-preview-metric-note',i===0?'+18% from last period':i===1?'Across three teams':'Next review · Friday'));metrics.append(metric);});
    body.append(metrics);
    const middle=make('div','dashboard-preview-middle');
    const chart=pick(make('section','dashboard-preview-chart dashboard-preview-panel'),'Completed by month');chart.append(make('h3','','Completed by month'));
    const graph=svg('svg',{viewBox:'0 0 430 150',role:'img','aria-label':'Bar chart showing completed work increasing from May to October'});
    for(const y of [20,60,100]){graph.append(svg('path',{d:`M26 ${y}H420`,class:'dashboard-preview-grid'}));const t=svg('text',{x:0,y:y+3,class:'dashboard-preview-axis'});t.textContent=String((120-y)/2);graph.append(t);}
    const numbers=[17,24,21,33,38,49].map((n,i)=>Math.min(55,n+(v*(i%3))));
    numbers.forEach((n,i)=>{const h=n*1.8,x=39+i*64;graph.append(svg('rect',{x,y:120-h,width:29,height:h,rx:3,class:'dashboard-preview-bar'}));const t=svg('text',{x:x+14.5,y:140,'text-anchor':'middle',class:'dashboard-preview-axis'});t.textContent=months[i];graph.append(t);});chart.append(graph);
    const team=pick(make('section','dashboard-preview-team dashboard-preview-panel'),'Work by team');team.append(make('h3','','Work by team'));
    for(const [name,value] of [['Design',78],['Engineering',92],['Operations',64]]){const row=make('div','dashboard-preview-team-row'),line=make('div','dashboard-preview-team-caption');line.append(make('span','',name),make('span','',value+'%'));const track=make('div','dashboard-preview-track'),fill=make('i');fill.style.width=value+'%';track.append(fill);row.append(line,track);team.append(row);}
    middle.append(chart,team);body.append(middle);
    const milestones=pick(make('section','dashboard-preview-milestones dashboard-preview-panel'),'Upcoming milestones');const milestoneHeader=make('div','dashboard-preview-section-head');milestoneHeader.append(make('h3','','Upcoming milestones'),make('span','','Next 14 days'));milestones.append(milestoneHeader);
    const table=make('table'),thead=make('thead'),tr=make('tr');for(const text of ['Milestone','Owner','Due','Status'])tr.append(make('th','',text));thead.append(tr);table.append(thead);const tbody=make('tbody');
    for(const row of [['Concept review','Design','Oct 30','Ready'],['Prototype handoff','Engineering','Nov 04','In progress'],['Stakeholder demo','Operations','Nov 09','On track']]){const tr=make('tr');row.forEach((text,i)=>{const td=make('td','',i===3?undefined:text);if(i===3)td.append(make('span','dashboard-preview-status',text));tr.append(td);});tbody.append(tr);}table.append(tbody);milestones.append(table);body.append(milestones);
    if(d.view==='milestones')root.classList.add('dashboard-preview-milestones-view');if(d.view==='team')root.classList.add('dashboard-preview-team-view');
    const footer=make('footer','dashboard-preview-footer');footer.append(make('span','','Project data · Studio sample'),make('span','',d.selectedWidget?`${d.selectedWidget} selected`:'Select a widget to refine it'));
    if(d.pending){const actions=make('div','dashboard-preview-review-actions');actions.append(button(make,'Apply preview','Apply this staged revision to the sample dashboard',()=>{d.appliedVariant=d.variant;d.pending=false;update();}),button(make,'Discard preview','Restore the applied revision while retaining conversation history',()=>{d.variant=d.appliedVariant;d.pending=false;update();}));footer.append(actions);}root.append(footer);return root;
  }
  window.StudioElements||=[];
  window.StudioElements.push({
    id:'dashboard-builder',title:'Dashboard Builder',width:760,height:510,
    initialPrompt:'Make the upcoming milestones easier to scan, and keep the three overview metrics.',
    initialTurns:[{role:'user',text:'Build a compact project dashboard with milestones, open questions, and the connected data.'},{role:'assistant',text:'The overview includes three metrics, monthly progress, team workload and the next milestones. Select a widget or describe the next change.'}],
    stages:[
      {label:'Reading dashboard',detail:'Inspecting the current layout, selected widget and connected sample data.',code:"const current = readDashboard();\nconst target = current.selectedWidget || 'overview';"},
      {label:'Updating layout',detail:'Preparing a staged revision while keeping the applied dashboard available.',code:source},
      {label:'Checking layout',detail:'Checking labels, spacing and responsive widget bounds.',code:"validateLabels(dashboard);\ncheckWidgetBounds({ minWidth: 320, maxWidth: 1440 });"},
      {label:'Preview ready',detail:'The proposed revision is ready to inspect and apply.',code:"stagePreview(dashboard);\n// Apply preview commits this revision."}
    ],
    renderPreview,
    applyResult(node,prompt){const d=data(node);d.variant++;d.pending=true;if(/blue/i.test(prompt))d.palette='blue';if(/month/i.test(prompt))d.period='month';if(/milestone/i.test(prompt))d.view='milestones';if(/overview|metrics/i.test(prompt))d.view='overview';},
    tools:[
      {id:'apply-preview',label:'Apply preview',hint:'Apply the staged dashboard revision in this Studio.',action(node){const d=data(node);d.appliedVariant=d.variant;d.pending=false;}},
      {id:'discard-preview',label:'Discard preview',hint:'Restore the last applied dashboard revision.',action(node){const d=data(node);d.variant=d.appliedVariant;d.pending=false;}},
      {id:'clear-widget',label:'Clear widget selection',hint:'Use the whole dashboard as the next instruction target.',action(node){delete data(node).selectedWidget;}}
    ]
  });
})();
