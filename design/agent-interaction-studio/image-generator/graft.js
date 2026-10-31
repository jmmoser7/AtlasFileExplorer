/* Input tree matching: graft splits items; ungrafted inputs stay a shared bundle. */
(function(root){
  const kinds=['image','text','style'];
  function jobs(inputs,grafted={}){
    const counts=kinds.filter(k=>grafted[k]).map(k=>(inputs[k]||[]).length);
    if(counts.some(n=>n===0))return [];
    const count=Math.max(1,...counts);
    return Array.from({length:count},(_,branch)=>Object.fromEntries(kinds.map(k=>[k,grafted[k]?[inputs[k][Math.min(branch,inputs[k].length-1)]]:[...(inputs[k]||[])]])));
  }
  function port(count,grafted){const slots=grafted?Math.min(4,Math.max(1,count)):1;return {height:12+(slots-1)*4,offset:index=>grafted?(Math.min(index,3)-(slots-1)/2)*4:0};}
  const api={jobs,port};root.ImageGraft=api;if(typeof module!=='undefined')module.exports=api;
})(typeof window==='undefined'?globalThis:window);
