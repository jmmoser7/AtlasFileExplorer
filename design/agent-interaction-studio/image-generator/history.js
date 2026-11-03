/* Conversation snapshots are independent of image-result stacks. */
(function(root){
  const clone=x=>JSON.parse(JSON.stringify(x));
  const turns=node=>[...(node.inheritedHistory||[]),...(node.conversation||[])];
  function connect(source,target){
    const snapshot=clone(turns(source));
    if((target.conversation||[]).length)target.pendingHistory=snapshot;
    else target.inheritedHistory=snapshot;
    target.historySource=source.id;target.sourceRevision=source.revision||0;
    target.historyCut=false;target.historyStale=false;target.historyConnected=true;source.historyEnabled=true;target.historyEnabled=true;source.interactive=true;target.interactive=true;
  }
  function disconnect(target){target.historyConnected=false;delete target.pendingHistory;}
  function submit(node,prompt){
    if(node.pendingHistory){node.inheritedHistory=node.pendingHistory;delete node.pendingHistory;}
    const prior=turns(node).length;node.conversation||=[];
    node.conversation.push({role:'user',text:prompt},{role:'assistant',text:`Preview response: I’ll use this direction for the next image${prior?`, keeping the ${prior} earlier messages in context`:''}. ${prompt}`});
    node.revision=(node.revision||0)+1;return turns(node);
  }
  const canCut=(wire,wires)=>wire?.kind==='history'&&!wires.some(w=>w.from===wire.to&&(w.kind==='history'||w.kind==='image'));
  function cut(wire,nodes,wires){
    if(!canCut(wire,wires))return false;
    const downstream=new Set([wire.to]);let changed=true;
    while(changed){changed=false;for(const w of wires)if(downstream.has(w.from)&&!downstream.has(w.to)){downstream.add(w.to);changed=true;}}
    for(const n of nodes)if(downstream.has(n.id)){n.historyArchive=[...(n.historyArchive||[]),...clone(turns(n))];n.inheritedHistory=[];n.conversation=[];disconnect(n);n.historyEnabled=false;n.interactive=false;n.contextIn=false;n.contextOut=false;n.historyCut=true;}
    return true;
  }
  const api={turns,connect,disconnect,submit,canCut,cut};root.ImageConversation=api;
  if(typeof module!=='undefined')module.exports=api;
})(typeof window==='undefined'?globalThis:window);
