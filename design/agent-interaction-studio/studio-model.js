/* Shared phase and element identity. No element-owned state is migrated or rewritten. */
(() => {
  'use strict';
  const phases=[
    {id:'00',name:'Image Generation',folder:'00-images',caption:'Explore appearance',detail:'Reference images for each element.',icon:'image'},
    {id:'01',name:'JS Studio',folder:'01-js',caption:'Design reference',detail:'Appearance and interaction.',icon:'window'},
    {id:'02',name:'Rust Component Studio',folder:'02-components',caption:'One real component',detail:'Pixels, menus and local behavior.',icon:'component'},
    {id:'03',name:'Rust Canvas Studio',folder:'03-canvas',caption:'Components together',detail:'Zoom, wires, focus and resizing.',icon:'canvas'},
    {id:'04',name:'Slate Integration',folder:'04-integration',caption:'Time summary',detail:'Actual application · recorded work.',icon:'integration'},
    {id:'05',name:'Release Build',folder:'05-release',caption:'Time summary',detail:'Optimized and packaged · recorded work.',icon:'package'}
  ];
  const builtins=[
    {id:'chat-train',name:'Chat Train',kind:'chat-train',contract:'contracts/chat-train.md'},
    {id:'image-generator',name:'Images',kind:'image-generator',contract:'contracts/image-generator.md',status:'draft'},
    ...[['dashboard-builder','Dashboard Builder'],['text-agent','Text Agent'],['rhino-agent','Rhino 3D'],['frame','Frames']].map(([id,name])=>({id,name,kind:'element',contract:`contracts/${id}.md`,status:'draft'})),
    {id:'video',name:'Videos',kind:'blank',contract:'contracts/video.md',status:'draft'}
  ];
  const studio={id:'studio',name:'Studio',kind:'studio'};
  function elements(saved){
    const seen=new Set([...builtins.map(t=>t.id),'studio']);
    const custom=(Array.isArray(saved?.tabs)?saved.tabs:[]).filter(t=>{
      if(!t||t.kind!=='blank'||typeof t.name!=='string'||typeof t.id!=='string'||seen.has(t.id))return false;
      seen.add(t.id);return true;
    });
    return [...builtins.map(t=>({...t})),...custom.map(t=>({...t})),{...studio}];
  }
  function route(search,tabs){
    const q=new URLSearchParams(search),id=q.get('tab');
    const phase=q.has('phase')?q.get('phase'):(id?'01':null);
    return {phase:phases.some(p=>p.id===phase)?phase:null,tab:tabs.some(t=>t.id===id)?id:'chat-train'};
  }
  globalThis.StudioPortalModel={phases,elements,route};
})();
