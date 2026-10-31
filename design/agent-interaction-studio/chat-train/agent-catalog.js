/* Pure model-capability contract; this studio catalogue is a fixture, not live availability. */
(()=>{
  const efforts=['low','medium','high','xhigh'], speeds=['standard','fast'];
  const providers=[
    {id:'cursor',label:'Cursor',icon:'cursor',hint:'Use the Cursor subscription connection.',models:[
      {id:'Auto',label:'Auto',efforts,speeds:['standard']},
      {id:'Astra',label:'Astra',efforts,speeds},
      {id:'Terra',label:'Terra',efforts,speeds},
      {id:'Luna',label:'Luna',efforts:['low','medium'],speeds},
      {id:'Long-name test — Experimental reasoning model with extended context, multimodal tools, research preview, and a deliberately oversized provider-supplied version identifier 2026-10-30-build-1234567890',label:'Long-name test — Experimental reasoning model with extended context, multimodal tools, research preview, and a deliberately oversized provider-supplied version identifier 2026-10-30-build-1234567890',efforts,speeds}
    ]},
    {id:'codex',label:'Codex',icon:'codex',hint:'Use the Codex subscription connection.',models:[
      {id:'Default',label:'Default',efforts,speeds},
      {id:'Sol',label:'Sol',efforts,speeds:['standard','fast','ultra']},
      {id:'Astra',label:'Astra',efforts,speeds:['standard']},
      {id:'Terra',label:'Terra',efforts,speeds:['standard']},
      {id:'Luna',label:'Luna',efforts:['low','medium'],speeds}
    ]},
    {id:'openai',label:'OpenAI API',icon:'openai',needsKey:true,hint:'Select API access. Use the separate chevron to add or change its key.',models:[
      {id:'Sample text model',label:'Sample text model',efforts:['low','medium','high'],speeds:['standard']}
    ]},
    {id:'local',label:'Local model',icon:'local',hint:'Use a model running on this computer.',models:[
      {id:'Sample local model',label:'Sample local model',efforts:[],speeds:[]}
    ]}
  ];
  const labels={low:'Low',medium:'Medium',high:'High',xhigh:'Extra high',standard:'Standard',fast:'Fast',automatic:'Automatic'};

  const speedInfo={standard:{label:'Standard',bolts:0},fast:{label:'Fast',bolts:1},ultra:{label:'Ultra',bolts:2},extra_fast:{label:'Extra fast',bolts:2}};
  labels.ultra='Ultra';labels.extra_fast='Extra fast';
  function register(provider){
    if(!provider.id||!Array.isArray(provider.models)||!provider.models.length)throw new Error('Provider requires an ID and model catalogue.');
    if(providers.some(p=>p.id===provider.id))throw new Error('Duplicate provider ID.');
    for(const model of provider.models){
      if(!model.id||!Array.isArray(model.efforts)||!Array.isArray(model.speeds))throw new Error('Each model must explicitly declare effort and speed options, including empty arrays.');
      for(const value of model.efforts)if(!labels[value])throw new Error('Effort option must have a registered label.');
      for(const value of model.speeds)if(!speedInfo[value])throw new Error('Speed option must have registered label and bolt metadata.');
    }
    providers.push(provider);
  }
  function normalize(config){
    const p=providers.find(p=>p.id===config.provider)||providers[0];
    const m=p.models.find(m=>m.id===config.model)||p.models[0];
    const effort=m.efforts.includes(config.effort)?config.effort:m.efforts.includes('medium')?'medium':m.efforts[0]||'automatic';
    const speed=m.speeds.includes(config.speed)?config.speed:m.speeds[0]||'automatic';
    return {provider:p.id,model:m.id,effort,speed};
  }
  function capabilities(config){const c=normalize(config),p=providers.find(p=>p.id===c.provider),m=p.models.find(m=>m.id===c.model);return {efforts:[...m.efforts],speeds:[...m.speeds]};}
  window.ChatAgentCatalog={providers,labels,normalize,capabilities,register,speedInfo,registerEffort(id,label){if(!id||!label)throw new Error('Effort requires an ID and label.');labels[id]=label;},registerSpeed(id,label,bolts){if(!id||!label||![0,1,2].includes(bolts))throw new Error('Speed metadata requires an ID, label and 0–2 bolts.');speedInfo[id]={label,bolts};labels[id]=label;}};
})();
