/* Responsive Studio chrome; existing bundled browser, isolated disposable storage. */
const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path'),os=require('node:os');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||path.join(os.homedir(),'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'));
const base=process.env.STUDIO_TEST_URL||'http://localhost:8765/';
test('wrapped full-name tabs and accessible options preserve canvas ownership', {timeout:90000},async()=>{
  const browser=await chromium.launch({channel:'msedge',headless:true});
  try{
    const context=await browser.newContext(),page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
    await context.addInitScript(()=>{localStorage.setItem('slate-design-studio-tabs-v1',JSON.stringify({tabs:[{id:'element-long',kind:'blank',name:'A long custom element with its complete review name retained'}]}));});
    for(const width of [960,768,640,390]){
      await page.setViewportSize({width,height:900});await page.goto(base+'?phase=02&tab=chat-train');
      await page.waitForFunction(()=>document.querySelector('#portal-phase').getBoundingClientRect().top>=document.querySelector('.studio-bar').getBoundingClientRect().bottom&&document.querySelector('#portal-phase').getBoundingClientRect().top-document.querySelector('.studio-bar').getBoundingClientRect().bottom<1);
      const tabs=await page.evaluate(()=>StudioShell.tabs);
      for(const tab of tabs){
        const button=page.getByRole('tab',{name:tab.name,exact:true});assert.ok(await button.isVisible());
        const rect=await button.boundingBox();assert.ok(rect.x>=0&&rect.x+rect.width<=width+0.5,`${width}: full ${tab.name} within viewport`);
        assert.equal(await button.evaluate(b=>b.scrollWidth<=b.clientWidth),true,`${width}: no clipped name`);
        await button.click();assert.equal(await page.evaluate(()=>StudioShell.route.tab),tab.id);
      }
      assert.ok(await page.locator('#new-element').isVisible());assert.equal(await page.locator('#notes-button').evaluate(b=>b.hidden),true);
      assert.ok((await page.locator('#new-element').boundingBox()).x<width);
      await page.locator('#studio-options-button').focus();await page.keyboard.press('ArrowDown');assert.equal(await page.evaluate(()=>document.activeElement.id),'phase-instructions');
      await page.keyboard.press('End');assert.equal(await page.evaluate(()=>document.activeElement.id),'theme-button');await page.keyboard.press('Escape');assert.equal(await page.locator('#studio-options-menu').isVisible(),false);assert.equal(await page.evaluate(()=>document.activeElement.id),'studio-options-button');
      await page.locator('#studio-options-button').click();await page.locator('.portal-phase-heading').click();assert.equal(await page.locator('#studio-options-menu').isVisible(),false);
      await page.locator('#studio-options-button').click();await page.locator('#phase-instructions').click();await page.frameLocator('.debrief-dialog iframe').locator('#document h1').first().waitFor();assert.equal(await page.locator('#studio-options-menu').isVisible(),false);await page.getByRole('button',{name:'Close',exact:true}).last().click();
      await page.selectOption('#phase-switch','01');
      for(const [tab,canvas]of [['chat-train','#canvas'],['image-generator','#image-generator-canvas'],['dashboard-builder','#dashboard-builder-canvas'],['frame','#frame-canvas']]){
        await page.locator('#studio-tabs').getByRole('tab',{name:tabs.find(t=>t.id===tab).name,exact:true}).click();
        await page.waitForFunction(selector=>document.querySelector(selector).getBoundingClientRect().top>=document.querySelector('.studio-bar').getBoundingClientRect().bottom,canvas);
        assert.ok(await page.locator(canvas).isVisible());
      }
      await page.getByRole('tab',{name:'Chat Train',exact:true}).click();await page.locator('#studio-options-button').click();assert.ok(await page.locator('#notes-button').isVisible());assert.ok(await page.locator('#reference-button').isVisible());await page.locator('#notes-button').click();assert.ok(await page.locator('#notes-panel').isVisible());
      await page.locator('#studio-options-button').click();await page.locator('#debrief-button').click();await page.frameLocator('.debrief-dialog iframe').locator('#document h1').first().waitFor();await page.getByRole('button',{name:'Close',exact:true}).last().click();
      await page.selectOption('#phase-switch','03');assert.equal(await page.locator('#notes-button').evaluate(b=>b.hidden),true);assert.equal(await page.locator('#notes-panel').isVisible(),false);
      await page.locator('#portal-home-button').click();assert.equal(await page.locator('#studio-tab-row').isVisible(),false);assert.ok((await page.locator('.studio-bar').boundingBox()).height<55);
    }
    assert.deepEqual(errors,[]);await context.close();
  }finally{await browser.close();}
});
