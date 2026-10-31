/* Stable text-to-answer anchors, independent of the preview renderer. */
(function(root){
  function valid(text,answer){return Number.isInteger(answer.start)&&Number.isInteger(answer.end)&&answer.end>answer.start&&text.slice(answer.start,answer.end)===answer.text;}
  function ranges(text,answers=[]){let end=0;return answers.filter(a=>valid(text,a)).sort((a,b)=>a.start-b.start).filter(a=>{if(a.start<end)return false;end=a.end;return true;});}
  function rebase(answers,oldText,newText){
    let prefix=0,suffix=0;while(prefix<oldText.length&&prefix<newText.length&&oldText[prefix]===newText[prefix])prefix++;
    while(suffix<oldText.length-prefix&&suffix<newText.length-prefix&&oldText[oldText.length-1-suffix]===newText[newText.length-1-suffix])suffix++;
    const oldEnd=oldText.length-suffix,delta=newText.length-oldText.length;
    return answers.flatMap(a=>a.end<=prefix?[{...a}]:a.start>=oldEnd?[{...a,start:a.start+delta,end:a.end+delta}]:[]).filter(a=>valid(newText,a));
  }
  function answer(node,question,response){
    const selection=node.selection;if(!selection||!valid(node.data.text,selection))return null;
    const item={...selection,id:`answer-${node.id}-${node.revision+1}-${node.data.optionIndex||0}`,question,response};
    node.data.answers=[...(node.data.answers||[]).filter(a=>a.end<=item.start||a.start>=item.end),item];return item;
  }
  const api={valid,ranges,rebase,answer};root.StudioTextLinks=api;if(typeof module!=='undefined')module.exports=api;
})(typeof window==='undefined'?globalThis:window);
