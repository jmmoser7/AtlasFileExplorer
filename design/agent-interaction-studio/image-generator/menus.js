/* Image menus reuse the Chat Train row, icon, divider and submenu vocabulary. */
window.createImageMenus=function({make,save,transient,repaint,historyActions}) {
  let pinned=null;
  const paths={mode:'<rect x="2" y="2" width="12" height="12" rx="2"/><path d="m6 5 5 3-5 3z"/>',settings:'<path d="M2 4h12M2 8h12M2 12h12"/><circle cx="5" cy="4" r="2"/><circle cx="11" cy="8" r="2"/><circle cx="6" cy="12" r="2"/>',check:'<path d="m3 8 3 3 7-7"/>',next:'<path d="m6 3 5 5-5 5"/>',image:'<rect x="2" y="2" width="12" height="12" rx="1"/><path d="m3 12 4-5 3 3 2-2 2 4"/>',copy:'<rect x="2" y="4" width="10" height="10" rx="1"/><path d="M5 2h9v10"/>',lock:'<rect x="3" y="7" width="10" height="7" rx="1"/><path d="M5 7V5a3 3 0 0 1 6 0v2"/>',trash:'<path d="M2 4h12M5 4V2h6v2M4 4l1 10h6l1-10"/>',folder:'<path d="M2 4h5l2 2h5v8H2z"/>',layers:'<path d="m8 1 7 4-7 4-7-4zM1 8l7 4 7-4M1 11l7 4 7-4"/>',tag:'<path d="M1 2h6l8 8-5 5-9-9z"/><circle cx="4" cy="5" r="1"/>',front:'<rect x="2" y="2" width="9" height="9"/><path d="M12 5h2v9H5v-2"/>',back:'<rect x="5" y="5" width="9" height="9"/><path d="M4 11H2V2h9v2"/>',external:'<path d="M9 2h5v5M14 2 7 9M6 3H2v11h11v-4"/>',crop:'<path d="M4 1v11h11M1 4h11v11"/>'};
  const glyph=name=>`<svg viewBox="0 0 16 16" aria-hidden="true">${paths[name]||paths.image}</svg>`;
  function close(){ChatAgentPicker.close();document.getElementById('menus').replaceChildren();if(pinned){pinned.element.classList.remove('menu-pinned');pinned=null;}document.querySelectorAll('.image-menu-trigger[aria-expanded]').forEach(e=>e.setAttribute('aria-expanded','false'));}
  function place(menu,x,y){menu.style.left=Math.max(6,Math.min(innerWidth-238,x))+'px';menu.style.top=Math.max(6,Math.min(innerHeight-menu.offsetHeight-6,y))+'px';}
  function shell(node,anchor,label){close();pinned=node;node.element.classList.add('menu-pinned');const menu=make('div','native-menu image-action-menu');menu.setAttribute('role','menu');menu.setAttribute('aria-label',label);document.getElementById('menus').append(menu);const box=anchor.getBoundingClientRect();menu._point=[box.left,box.bottom+3];place(menu,...menu._point);return menu;}
  function row(menu,{label,icon='image',value='',selected=false,expand=false,action,hint}){const b=make('button','menu-row'+(selected?' selected':''));b.setAttribute('role',selected?'menuitemradio':'menuitem');if(selected)b.setAttribute('aria-checked','true');b.innerHTML=`<span class="menu-glyph">${glyph(selected?'check':icon)}</span>`;b.append(make('span','',label));if(value)b.append(make('span','image-menu-value',value));if(expand){const caret=make('span','menu-glyph image-sub-caret');caret.innerHTML=glyph('next');b.append(caret);}if(hint)b.dataset.hint=hint;b.onclick=action;menu.append(b);return b;}
  function aspect(node,anchor){if(pinned===node&&document.querySelector('[aria-label="Aspect Ratio"][role="menu"]')){close();return;}const menu=shell(node,anchor,'Aspect Ratio');row(menu,{label:'Aspect Ratio',value:node.aspect===5?`${node.customRatio?.width||1}:${node.customRatio?.height||1}`:['Source','1:1','3:2','2:3','16:9'][node.aspect],action:close});menu.append(make('div','menu-rule'));['Source','1:1','3:2','2:3','16:9','Custom…'].forEach((label,i)=>row(menu,{label,selected:node.aspect===i,action:()=>{close();if(i===5)custom(node);else{node.aspect=i;save();repaint(node);}}}));place(menu,...menu._point);}
  function custom(node){const cover=make('div','modal-cover'),form=make('form','reference-dialog image-custom-ratio');form.setAttribute('role','dialog');form.setAttribute('aria-label','Custom aspect ratio');form.append(make('h2','','Custom aspect ratio'));const inputs={};for(const [key,label] of [['width','Width'],['height','Height']]){const wrap=make('label','',label),input=make('input');input.type='number';input.min='1';input.max='10000';input.required=true;input.value=node.customRatio?.[key]|| (key==='width'?16:9);input.setAttribute('aria-label',label);inputs[key]=input;wrap.append(input);form.append(wrap);}const actions=make('div','dialog-actions'),cancel=make('button','','Cancel'),apply=make('button','','Apply');cancel.type='button';cancel.onclick=()=>cover.remove();apply.type='submit';actions.append(cancel,apply);form.append(actions);form.onsubmit=e=>{e.preventDefault();node.aspect=5;node.customRatio={width:+inputs.width.value,height:+inputs.height.value};save();repaint(node);cover.remove();};cover.append(form);document.getElementById('dialogs').replaceChildren(cover);inputs.width.focus();}
  function options(node,anchor){
    if(pinned===node&&document.querySelector('[aria-label="Image options"][role="menu"]')){close();return;}
    const menu=shell(node,anchor,'Image options');let page='';
    function render(){menu.replaceChildren();for(const section of ['aspect','mode','settings']){
      const label=section==='aspect'?'Aspect Ratio':section==='mode'?'Mode':'Settings';
      const value=section==='aspect'?(node.aspect===5?`${node.customRatio?.width||1}:${node.customRatio?.height||1}`:['Source','1:1','3:2','2:3','16:9'][node.aspect]):section==='mode'?(node.live?'Live Mode':'Single Image Mode'):'';
      const header=row(menu,{label,value,icon:section==='aspect'?'crop':section,expand:true,action:()=>{page=page===section?'':section;render();}});header.setAttribute('aria-expanded',String(page===section));
      if(page!==section)continue;menu.append(make('div','menu-rule'));
      const items=section==='aspect'?['Source','1:1','3:2','2:3','16:9','Custom…']:section==='mode'?['Single Image Mode','Live Mode']:['Seed unlocked','Seed locked'];
      items.forEach((text,i)=>row(menu,{label:text,selected:section==='aspect'?node.aspect===i:section==='mode'?!!node.live===!!i:!!node.seed===!!i,action:()=>{close();if(section==='aspect'&&i===5){custom(node);return;}if(section==='aspect')node.aspect=i;else if(section==='mode')node.live=!!i;else node.seed=!!i;save();repaint(node);}}));break;
    }if(!page){menu.append(make('div','menu-rule'));
      row(menu,{label:'Preserve model history',icon:'layers',selected:!!node.historyEnabled,hint:'Keep conversation in this image; dragging its output carries history downstream.',action:()=>{historyActions.preserve(node);render();}});
    }place(menu,...menu._point);}render();
  }
  function context(node,index,e){
    const target=index==null?'body':'result',anchor={getBoundingClientRect:()=>({left:e.clientX,bottom:e.clientY})},menu=shell(node,anchor,`${node.label}${index==null?'':`: Result ${index+1}`} image menu`);
    const iconMap={'bring-to-front':'front','send-to-back':'back','external-link':'external','trash-2':'trash'};
    for(const entry of ImageMenuStudio.entries(target)){const key=entry.id;if(key.startsWith('sep:')){if(menu.children.length)menu.append(make('div','menu-rule'));continue;}const def=ImageContextRecipe.actions[key];if(!def)continue;
      const label=key==='board.lock'&&node.locked?'Unlock':entry.label;
      const b=row(menu,{label,icon:iconMap[def.icon]||def.icon,expand:key==='menu.tags',hint:index==null?label:`${label} · result ${index+1}`,action:()=>{
        if(key==='menu.tags'){menu.querySelector('.image-tags-submenu')?.remove();const sub=make('div','image-tags-submenu');menu.append(sub);['Reference','Keep','Review'].forEach(text=>row(sub,{label:text,icon:'tag',action:()=>{node.tag=text;save();close();transient(`Tag: ${text}`);}}));place(menu,...menu._point);return;}
        close();if(key==='image.result.use'&&index!=null){node.result=index;save();repaint(node);return;}
        if(key==='board.lock'){node.locked=!node.locked;save();repaint(node);}else if(key==='board.to_front')node.element.style.zIndex='10';else if(key==='board.to_back')node.element.style.zIndex='0';else transient(`${label} is unchanged in Slate and simulated here.`);
      }});if(def.danger)b.classList.add('danger');
    }
    menu.append(make('div','menu-rule'));const edit=row(menu,{label:'Edit this menu…',icon:'settings',hint:'Studio-only: configure image-body and thumbnail menus independently.',action:()=>{close();ImageMenuStudio.open(target);}});edit.classList.add('image-menu-studio-link');place(menu,...menu._point);
  }
  window.ImageMenuGlyph=glyph;
  return {close,aspect,options,context};
};
