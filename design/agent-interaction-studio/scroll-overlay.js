/* Shared zero-gutter scrollbar: visible only while the user scrolls. */
window.StudioScroll = {
  attach(view,host=view) {
    if(view.dataset.overlayScroll)return;
    view.dataset.overlayScroll='true';view.classList.add('overlay-scroll');
    const thumb=document.createElement('i');thumb.className='studio-scroll-thumb';thumb.setAttribute('aria-hidden','true');host.append(thumb);
    const update=()=>{const h=view.clientHeight,total=view.scrollHeight;thumb.hidden=total<=h+1;const size=Math.max(24,h*h/Math.max(h,total));thumb.style.height=size+'px';thumb.style.top=(view===host?view.scrollTop:view.offsetTop)+(h-size)*view.scrollTop/Math.max(1,total-h)+'px';};
    const show=()=>{update();thumb.classList.add('is-scrolling');clearTimeout(thumb._fade);thumb._fade=setTimeout(()=>thumb.classList.remove('is-scrolling'),650);};
    view.addEventListener('wheel',show,{passive:true});view.addEventListener('keydown',e=>{if(['ArrowUp','ArrowDown','PageUp','PageDown','Home','End',' '].includes(e.key))show();});view.addEventListener('scroll',update);new ResizeObserver(update).observe(view);update();return update;
  }
};
new MutationObserver(()=>document.querySelectorAll('.native-menu:not([data-overlay-scroll]),.agent-picker:not([data-overlay-scroll])').forEach(menu=>{if(menu.scrollHeight>menu.clientHeight+1)StudioScroll.attach(menu);})).observe(document.documentElement,{childList:true,subtree:true});
