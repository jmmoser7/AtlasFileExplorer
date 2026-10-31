/* Pure conversation/branch state for the three new Studio elements. */
(function(root){
  const copy=value=>JSON.parse(JSON.stringify(value));
  const contextTurns=node=>[...(node.inherited||[]),...(node.turns||[]).slice(node.contextStart||0)];
  function create(config,id='root'){
    return {id,x:90,y:65,width:config.width,height:config.height,data:{},revision:0,
      turns:copy(config.initialTurns||[]),inherited:[],contextStart:0,prompt:config.initialPrompt||'',
      preserve:true,mode:'single',open:true,selected:true,events:[],script:'',activity:false};
  }
  function branch(source,id){
    return {...copy(source),id,x:source.x+source.width+110,y:source.y,
      inherited:source.preserve?copy(contextTurns(source)):[],
      turns:[],contextStart:0,prompt:'',selection:null,parent:source.preserve?source.id:null,open:false,selected:false,blank:true,grafted:{},versions:[],revision:0,selectedRevision:0,
      events:[],script:'',running:false,stage:-1,activity:false};
  }
  function begin(node,prompt,selection){
    if(node.running||!prompt.trim())return false;
    node.turns.push({role:'user',text:prompt,selection:selection?copy(selection):null});
    node.prompt='';node.running=true;node.stage=-1;node.events=[];node.script='';
    return true;
  }
  function finish(node,text){node.turns.push({role:'assistant',text});node.running=false;node.revision++;}
  function stop(node){node.running=false;node.events.push({label:'Stopped',detail:'The preview run stopped. Completed output remains available.'});}
  function cut(node){node.archived=[...(node.archived||[]),...copy(contextTurns(node))];node.inherited=[];node.contextStart=(node.turns||[]).length;node.preserve=false;node.parent=null;}
  function options(node,count,apply){
    const base=copy(node.data),total=Math.max(1,Math.min(8,Number(count)||1));
    return Array.from({length:total},(_,index)=>{const candidate={...node,data:copy(base)};candidate.data.optionIndex=index;if(index)candidate.data.variant=(base.variant||0)+index;apply(candidate,index);return copy(candidate.data);});
  }
  const api={copy,contextTurns,create,branch,begin,finish,stop,cut,options};root.StudioSession=api;
  if(typeof module!=='undefined')module.exports=api;
})(typeof window==='undefined'?globalThis:window);
