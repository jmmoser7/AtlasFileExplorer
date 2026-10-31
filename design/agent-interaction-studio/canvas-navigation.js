/* Slate board navigation contract, shared by Studio canvases.

   Native owners: app/board.rs and atlas-core/src/display.rs (SLATE_BOARD). */

(() => {

  const profile={min:1/128,max:128,wheel:.0015,dragThreshold:4};

  const clamp=z=>Math.max(profile.min,Math.min(profile.max,z));

  const factor=delta=>Math.exp(-delta*profile.wheel);

  const editing=target=>target instanceof Element&&target.matches('textarea,input,[contenteditable="true"]')&&document.activeElement===target;

  function bind({canvas,camera,transform,closeMenus,context,deselect}) {

    let space=false,pan=null,suppressContext=false;

    const zoom=(next,x=canvas.clientWidth/2,y=canvas.clientHeight/2)=>{const z=clamp(next);camera.x=x-(x-camera.x)*z/camera.zoom;camera.y=y-(y-camera.y)*z/camera.zoom;camera.zoom=z;closeMenus();transform();};

    canvas.addEventListener('pointerdown',e=>{

      if(e.button===1||e.button===2||(space&&e.button===0&&!editing(e.target))){

        e.preventDefault();e.stopImmediatePropagation();closeMenus();

        pan={id:e.pointerId,x:e.clientX,y:e.clientY,ox:camera.x,oy:camera.y,moved:false,target:e.target,button:e.button};suppressContext=false;canvas.setPointerCapture(e.pointerId);

      }else if(e.button===0&&!e.target.closest('.native-image-node,.native-input-text,.wire-hit,.element-node')){const hadMenu=!!document.querySelector('.native-menu');closeMenus();document.activeElement?.blur();if(!hadMenu)deselect?.();}

    },true);

    canvas.addEventListener('pointermove',e=>{if(!pan)return;e.stopImmediatePropagation();const dx=e.clientX-pan.x,dy=e.clientY-pan.y;if(Math.hypot(dx,dy)>profile.dragThreshold)pan.moved=true;if(pan.moved){camera.x=pan.ox+dx;camera.y=pan.oy+dy;canvas.classList.add('is-panning');transform();}},true);

    const end=e=>{if(!pan)return;e.stopImmediatePropagation();const ended=pan;suppressContext=true;pan=null;if(e.type!=='pointercancel'&&ended.button===2&&!ended.moved)context?.({target:ended.target,clientX:e.clientX,clientY:e.clientY});canvas.classList.remove('is-panning');if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);};

    canvas.addEventListener('pointerup',end,true);canvas.addEventListener('pointercancel',end,true);

    const contextMenu=e=>{e.preventDefault?.();if(pan||suppressContext){suppressContext=false;return;}context?.(e);};
    canvas.addEventListener('contextmenu',contextMenu);

    canvas.addEventListener('wheel',e=>{if(e.target.closest?.('[data-capture-wheel]')&&!e.ctrlKey){e.preventDefault();const region=e.target.closest('[data-capture-wheel]');region.scrollTop+=e.deltaY/camera.zoom;return;}if((editing(e.target)||(e.target.closest?.('.image-dialogue')&&e.target.closest('.image-dialogue')===document.activeElement))&&!e.ctrlKey&&!e.shiftKey)return;e.preventDefault();if(pan)return;closeMenus();if(e.shiftKey){camera.x-=e.deltaY;transform();return;}const r=canvas.getBoundingClientRect();zoom(camera.zoom*factor(e.deltaY),e.clientX-r.left,e.clientY-r.top);},{passive:false});

    window.addEventListener('keydown',e=>{if(canvas.hidden)return;if(e.code==='Space'&&!e.target.closest('input,textarea,[contenteditable="true"]')){e.preventDefault();space=true;canvas.classList.add('space-held');}});

    const reset=()=>{space=false;pan=null;canvas.classList.remove('space-held','is-panning');};

    window.addEventListener('keyup',e=>{if(e.code==='Space')reset();});window.addEventListener('blur',reset);

    return {zoom,contextMenu};

  }

  window.StudioCanvasNavigation={profile,clamp,factor,editing,bind};

})();
