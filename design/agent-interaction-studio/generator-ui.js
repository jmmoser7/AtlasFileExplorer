/* The one conversation and count control used by Images and other generator hosts. */
(() => {
  const make=(tag,cls,text)=>{const e=document.createElement(tag);if(cls)e.className=cls;if(text!=null)e.textContent=text;return e;};
  function conversation(host,options){
    const {interactive,turns=[],prompt='',placeholder,onInput,onSubmit,onScroll,onMeasure,leading,top=interactive?36:28,maxHeight=220,scrollTop}=options;
    const body=make('div','generator-conversation-body');body.style.top=top+'px';body.classList.toggle('is-dialogue',!!interactive);host.append(body);
    if(leading)body.append(leading);
    let transcript=null;
    if(turns.length&&maxHeight>0){
      transcript=make('div','image-dialogue generator-transcript');transcript.tabIndex=0;transcript.dataset.captureWheel='true';transcript.setAttribute('aria-label',options.conversationLabel||options.label+' conversation');transcript.style.maxHeight=maxHeight+'px';
      for(const turn of turns){const p=make('div',turn.role==='user'?'dialogue-user':'dialogue-assistant');if(turn.id)p.dataset.turnId=turn.id;if(turn.selection?.text)p.append(make('q','generator-quoted-selection',turn.selection.text));p.append(make('p','',turn.text));if(turn.code)p.append(make('pre','',turn.code));transcript.append(p);}
      transcript.onpointerdown=e=>e.stopPropagation();transcript.onscroll=()=>onScroll?.(transcript.scrollTop);body.append(transcript);StudioScroll.attach(transcript,body);
    }
    const composer=make('textarea','image-prompt generator-composer'+(interactive?' dialogue-composer':''));composer.value=prompt;composer.placeholder=placeholder;composer.spellcheck=false;composer.setAttribute('aria-label',options.promptLabel||options.label+' prompt');if(interactive)composer.dataset.captureWheel='true';composer.onpointerdown=e=>e.stopPropagation();body.append(composer);
    const measure=()=>{composer.style.height='0px';composer.style.height=Math.max(interactive?20:32,Math.min(interactive?100:144,composer.scrollHeight))+'px';host.style.height=(top+body.offsetHeight)+'px';onMeasure?.(top+body.offsetHeight);};
    composer.oninput=()=>{onInput?.(composer.value);measure();};composer.onkeydown=e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();onSubmit?.();}if(e.key==='Escape')composer.blur();};
    measure();if(transcript)transcript.scrollTop=scrollTop??transcript.scrollHeight;StudioScroll.attach(composer,body);requestAnimationFrame(measure);return {body,composer,transcript,measure};
  }
  function count(value,label,onChange){const root=make('div','image-count');root.dataset.hint='Number of options to generate per request (1–8).';root.setAttribute('aria-label',label+' number of options');const text=make('span','',String(value));for(const delta of [-1,1]){const b=make('button','',delta<0?'‹':'›');b.setAttribute('aria-label',label+(delta<0?' fewer options':' more options'));b.dataset.hint=delta<0?'Generate one fewer option':'Generate one more option';b.onpointerdown=e=>e.stopPropagation();b.onclick=e=>{e.stopPropagation();value=Math.max(1,Math.min(8,value+delta));text.textContent=value;onChange(value);};if(delta===1)root.append(text);root.append(b);}return root;}
  function modelLabel(button,label,config,catalog){
    button.replaceChildren();const speed=make('span','agent-speed-mark'),bolts=catalog.speedInfo[config.speed]?.bolts||0;
    speed.innerHTML=Array.from({length:bolts},()=>'<svg viewBox="0 0 18 18" aria-hidden="true"><path d="M10 1L3 10h5l-1 7 7-10H9z"/></svg>').join('');speed.hidden=!bolts;button.append(speed,make('span','image-field-value',label));
    if(config.effort&&config.effort!=='automatic')button.append(make('span','agent-effort-label',catalog.labels[config.effort]));button.append(make('span','image-caret'));button.title=[label,catalog.labels[config.effort],catalog.labels[config.speed]].filter(Boolean).join(' · ');
  }
  function closeMenus(){window.ChatAgentPicker?.close();document.getElementById('menus')?.replaceChildren();document.querySelectorAll('.menu-pinned').forEach(e=>e.classList.remove('menu-pinned'));}
  document.addEventListener('pointerdown',e=>{if(!e.target.closest('.native-menu,.agent-picker,.image-menu-trigger,.model-name,.header-menu,.element-model,.element-more,.menu-studio-panel')){e.studioDismissedMenu=!!document.querySelector('.native-menu,.agent-picker');if(e.studioDismissedMenu)document.dispatchEvent(new CustomEvent('studio-outside-menu-dismiss',{detail:{target:e.target}}));closeMenus();}},true);
  window.StudioGeneratorUI={conversation,count,modelLabel,closeMenus};
})();
