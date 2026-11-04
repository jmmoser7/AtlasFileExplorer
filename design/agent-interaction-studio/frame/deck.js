/* board_deck.rs: first-contact polyline ordering, including skipped pointer samples. */
(() => {
'use strict';
function entry(a,b,r){
 let t0=0,t1=1;const dx=b[0]-a[0],dy=b[1]-a[1],p=[-dx,dx,-dy,dy],q=[a[0]-r.x,r.x+r.w-a[0],a[1]-r.y,r.y+r.h-a[1]];
 for(let i=0;i<4;i++){if(Math.abs(p[i])<1e-8){if(q[i]<0)return null;continue;}const t=q[i]/p[i];if(p[i]<0){if(t>t1)return null;t0=Math.max(t0,t);}else{if(t<t0)return null;t1=Math.min(t1,t);}}
 return t0<=t1?t0:null;
}
function along(frames,points){
 const hit=[];if(!points.length)return hit;
 const segments=points.length===1?[[points[0],points[0]]]:points.slice(1).map((p,i)=>[points[i],p]);
 for(const [a,b] of segments){let cursor=0;while(true){let best=null;frames.forEach((f,i)=>{if(f.hidden||hit.includes(f.id))return;const t=entry(a,b,f.rect);if(t===null||t+1e-4<cursor)return;if(!best||t<best.t-1e-4||Math.abs(t-best.t)<=1e-4&&i>best.i)best={t,i,id:f.id};});if(!best)break;hit.push(best.id);cursor=best.t+1e-3;if(cursor>1)break;}}
 return hit;
}
function ensure(state) {
  if (!Array.isArray(state.presentations) || !state.presentations.length) {
    const ids = [...state.frames]
      .sort((a, b) => a.order - b.order || a.id.localeCompare(b.id, undefined, {numeric: true}))
      .map(frame => frame.id);
    state.presentations = [{id: 'presentation-1', name: 'Presentation 1', frameIds: ids}];
  }
  if (!state.presentations.some(deck => deck.id === state.activePresentationId)) {
    state.activePresentationId = state.presentations[0].id;
  }
}

function active(state) {
  return state.presentations.find(deck => deck.id === state.activePresentationId);
}

function slides(state, deck = active(state)) {
  const visible = new Map(state.frames.filter(frame => !frame.hidden).map(frame => [frame.id, frame]));
  return deck.frameIds.map(id => visible.get(id)).filter(Boolean);
}

function append(state, ids) {
  const deck = active(state);
  const eligible = new Set(state.frames.filter(frame => !frame.hidden).map(frame => frame.id));
  for (const id of ids) {
    if (eligible.has(id) && !deck.frameIds.includes(id)) deck.frameIds.push(id);
  }
}

function remove(state, ids) {
  const excluded = new Set(ids);
  const presentation = active(state);
  presentation.frameIds = presentation.frameIds.filter(id => !excluded.has(id));
}

function toggle(state, id) {
  if (active(state).frameIds.includes(id)) remove(state, [id]);
  else append(state, [id]);
}

function insert(order, selected, target, after) {
  const index = order.indexOf(target);
  if (index < 0 || selected.has(target)) return order;
  const moved = order.filter(id => selected.has(id));
  const rest = order.filter(id => !selected.has(id));
  const destination = rest.indexOf(target) + Number(after);
  rest.splice(destination, 0, ...moved);
  return rest;
}

window.FrameDeck = {entry, along, ensure, active, slides, append, remove, toggle, insert};
})();
