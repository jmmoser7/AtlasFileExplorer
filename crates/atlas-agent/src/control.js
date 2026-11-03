(()=>{
  if(window.slateControl?.version===1)return;
  let sequence=0,queue=[];
  const api={version:1,state:{ready:false,message:'Waiting for model wire'},
    set(control_id,value){
      if(typeof control_id!=='string'||queue.length>=8)throw Error('Control queue is full');
      const intent={version:1,control_id,sequence:++sequence,value};
      if(JSON.stringify(intent).length>4096)throw Error('Control value is too large');
      queue.push(intent);return intent.sequence;
    },
    __drain(){
      const tag=document.querySelector('script[type="application/slate-control+json"]');
      if(!tag)return '';
      try{const manifest=JSON.parse(tag.textContent);const result=JSON.stringify({manifest,intents:queue.splice(0,8)});return result.length<=32768?result:'';}catch{return '';}
    },
    __accept(state){api.state=state;window.dispatchEvent(new CustomEvent('slate-control',{detail:state}));}
  };
  Object.defineProperty(window,'slateControl',{value:api,configurable:false,writable:false});
  window.dispatchEvent(new CustomEvent('slate-control',{detail:api.state}));
})();
