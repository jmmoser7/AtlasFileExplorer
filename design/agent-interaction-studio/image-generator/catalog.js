/* Native image-model fixtures from runtime/models.rs and atlas-openai/src/lib.rs.
   Deliberately separate from the text-agent catalogue. No live availability claim. */
(() => {
  'use strict';
  const model=(id,label)=>({id,label,efforts:[],speeds:[]});
  const providers=[
    {id:'comfy',label:'ComfyUI',icon:'local',hint:'Use a local ComfyUI image workflow. This Studio does not connect to it.',models:[model('','Auto')]},
    {id:'codex-image',label:'ChatGPT',icon:'openai',hint:'Generate images using your ChatGPT sign-in. This is a preview connection.',models:[model('','Your sign-in')]},
    {id:'openai-image',label:'OpenAI API',icon:'openai',needsKey:true,hint:'Use an image model through the API. The separate chevron adds or changes its key.',models:[model('gpt-image-2','GPT Image 2'),model('gpt-image-1-mini','GPT Image 1 mini')]}
  ];
  function normalize(config={}){
    const provider=providers.find(p=>p.id===config.provider)||providers[0];
    const selected=provider.models.find(m=>m.id===config.model)||provider.models[0];
    return {provider:provider.id,model:selected.id,effort:'automatic',speed:'automatic'};
  }
  window.ImageAgentCatalog={providers,normalize,labels:{automatic:'Automatic'},speedInfo:{}};
})();
