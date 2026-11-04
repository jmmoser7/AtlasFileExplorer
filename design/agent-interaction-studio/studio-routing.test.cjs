/* Isolated project browser regression: no user profile, no provider calls. */
const {test}=require('node:test');const assert=require('node:assert/strict');const path=require('node:path'),os=require('node:os');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||path.join(os.homedir(),'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'));
const base=process.env.STUDIO_TEST_URL||'http://localhost:8765/';
test('Studio routes retain identity/state and isolate legacy tools', {timeout:90000},async()=>{
  const browser=await chromium.launch({channel:'msedge',headless:true});
  try{
    const context=await browser.newContext({viewport:{width:1500,height:950}});const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await context.addInitScript(()=>{
      if(sessionStorage.getItem('portal-test-seeded'))return;
      localStorage.setItem('slate-design-studio-tabs-v1',JSON.stringify({tabs:[{id:'element-retained',name:'Retained custom',kind:'blank',fixture:'keep'}]}));
      localStorage.setItem('slate-chat-train-review-notes','retained note');localStorage.setItem('portal-unrelated-fixture','preserve');sessionStorage.setItem('portal-test-seeded','yes');
    });
    await page.goto(base);await page.waitForFunction(()=>window.StudioShell&&document.body.dataset.studioPhase==='home');
    assert.equal(await page.locator('.phase-card').count(),6);const datums=await page.locator('.phase-card').evaluateAll(cards=>cards.map(c=>({scope:c.querySelector('.phase-caption').getBoundingClientRect().top,body:c.querySelector('.phase-detail').getBoundingClientRect().top})));assert.ok(datums.every(d=>Math.abs(d.scope-datums[0].scope)<0.5&&Math.abs(d.body-datums[0].body)<0.5),'home scopes and descriptions share exact row datums');assert.equal(await page.locator('#canvas').isVisible(),false);assert.equal(await page.evaluate(()=>StudioShell.activeId),'studio-portal-inactive');
    await page.locator('.phase-card[data-phase="02"]').click();await page.locator('#studio-tabs button').filter({hasText:'Retained custom'}).click();
    const ids=await page.evaluate(()=>StudioShell.tabs.map(t=>t.id));assert.equal(ids[0],'chat-train');assert.equal(ids.at(-1),'studio');
    assert.equal(await page.locator('.portal-native-world').evaluate(e=>e.childElementCount),0);
    for(const phase of ['03','00','05','04','01','02']){
      await page.selectOption('#phase-switch',phase);assert.deepEqual(await page.evaluate(()=>StudioShell.tabs.map(t=>t.id)),ids);assert.equal(await page.evaluate(()=>StudioShell.route.tab),'element-retained');
      assert.equal(await page.evaluate(()=>StudioShell.activeId),phase==='01'?'element-retained':'studio-portal-inactive');
      assert.equal(await page.locator('#notes-button').isVisible(),false);assert.equal(await page.locator('.studio-bottom:visible').count(),0);assert.equal(await page.locator('#menus').evaluate(e=>e.childElementCount),0);
    }
    await page.selectOption('#phase-switch','03');await page.goBack();assert.equal(await page.evaluate(()=>StudioShell.route.phase),'02');
    await page.locator('#new-element').click();await page.fill('#element-name','One shared custom');await page.getByRole('button',{name:'Create tab',exact:true}).click();
    const custom=await page.evaluate(()=>StudioShell.route.tab);assert.equal(await page.evaluate(id=>StudioShell.tabs.filter(t=>t.id===id).length,custom),1);
    await page.selectOption('#phase-switch','01');assert.equal(await page.evaluate(()=>StudioShell.route.tab),custom);await page.reload();assert.equal(await page.evaluate(()=>StudioShell.route.tab),custom);
    assert.equal(await page.evaluate(()=>localStorage.getItem('slate-chat-train-review-notes')),'retained note');assert.equal(await page.evaluate(()=>localStorage.getItem('portal-unrelated-fixture')),'preserve');
    await page.goto(base+'?tab=chat-train');await page.waitForFunction(()=>document.body.dataset.studioPhase==='01');await page.locator('#studio-options-button').click();assert.ok(await page.locator('#notes-button').isVisible());await page.locator('#studio-options-button').press('Escape');
    await page.goto(base+'?tab=image-generator');await page.waitForFunction(()=>document.body.dataset.studioPhase==='01');assert.equal(await page.evaluate(()=>StudioShell.activeId),'image-generator');assert.ok(await page.locator('#image-generator-canvas').isVisible());
    const prompt=page.locator('#image-generator-canvas textarea').first();await prompt.fill('Retained image prompt');await prompt.dispatchEvent('input');const imageKey=await page.evaluate(()=>Object.keys(localStorage).find(k=>k.includes('image')&&localStorage.getItem(k).includes('Retained image prompt')));assert.ok(imageKey,'real image prompt persisted by its existing owner');const persisted=await page.evaluate(key=>localStorage.getItem(key),imageKey);
    await page.selectOption('#phase-switch','02');assert.equal(await page.locator('#image-generator-canvas').isVisible(),false);assert.equal(await page.locator('.studio-bottom:visible').count(),0);assert.equal(await page.evaluate(key=>localStorage.getItem(key),imageKey),persisted);
    await page.goto(base+'?tab=frame');await page.waitForFunction(()=>document.body.dataset.studioPhase==='01');assert.equal(await page.evaluate(()=>StudioShell.activeId),'frame');assert.ok(await page.locator('#frame-canvas').isVisible());
    for(const phase of ['00','01','02','03','04','05']){
      await page.selectOption('#phase-switch',phase);await page.locator('#studio-options-button').click();await page.locator('#phase-instructions').click();const frame=page.frameLocator('.debrief-dialog iframe');await frame.locator('#document h1').waitFor();assert.doesNotMatch(await frame.locator('#document').innerText(),/Invalid document path|could not be loaded/);await page.getByRole('button',{name:'Close',exact:true}).last().click();
    }
    await page.goto(base+'?phase=05&tab=studio');await page.locator('.portal-records').waitFor();assert.equal(await page.locator('.portal-records').getAttribute('open'),null);await page.locator('.portal-records>summary').click();assert.ok(await page.locator('.portal-records a[target=_blank]').count()>0);assert.doesNotMatch(await page.locator('.portal-records').innerText(),/""/);assert.ok((await page.locator('.portal-records').innerText()).includes(await page.evaluate(()=>Intl.DateTimeFormat().resolvedOptions().timeZone)));
    await page.locator('#portal-home-button').click();assert.ok(await page.locator('#portal-home').isVisible());assert.ok(await page.locator('#portal-resume').isVisible());assert.deepEqual(errors,[]);
    await context.close();
  }finally{await browser.close();}
});
