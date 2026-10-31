const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const os=require('node:os');
const path=require('node:path');
const vm=require('node:vm');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE || path.join(os.homedir(),'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'));
const base=process.env.STUDIO_URL || 'http://127.0.0.1:8765/';
const output=path.join(__dirname,'validation');
const result={checks:[],errors:[]};
const verify=(text,value)=>{assert.ok(value,text);result.checks.push(text);};
const snapshot=page=>page.evaluate(()=>window.AgentStudio.snapshot());
const mode=async(page,id)=>{await page.selectOption('#mode-select',id);};
const scenario=async(page,id)=>{await page.selectOption('#scenario-select',id);};
const root=async(page,name)=>{await page.locator(`[data-menu="${name}"]`).first().click();};
const submenu=async(page,parent,name)=>{await page.locator(`.menu-panel[data-menu-name="${parent}"] button[data-submenu="${name}"]`).click();};
const choose=async(page,action,value)=>{await page.locator(`.menu-panel button[data-menu-action="${action}"]${value==null?'':`[data-value="${value}"]`}`).click();};
async function selectProvider(page,id){await root(page,'agent');await submenu(page,'agent','providers');await choose(page,'provider',id);}
async function openKeys(page){await root(page,'agent');await submenu(page,'agent','providers');await page.getByRole('menuitem',{name:'API key actions',exact:true}).click();}
(async()=>{
await fs.mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe'});
try{
const context=await browser.newContext({viewport:{width:1440,height:1050},acceptDownloads:true});
const page=await context.newPage();
page.on('pageerror',e=>result.errors.push(e.message));
await page.clock.install();await page.goto(base);await page.locator('.provider-chip').waitFor();
verify('Studio setup and feedback are collapsed by default',!await page.locator('#studio-setup').isVisible()&&!await page.locator('#studio-feedback').isVisible());
verify('Access control belongs to the evaluated agent toolbar',await page.locator('.agent-toolbar #global-access').count()===1&&await page.locator('.studio-toolbar #global-access').count()===0);
verify('A single frame identifies the interface under review',await page.locator('.review-stage').count()===1&&await page.locator('.review-frame-caption').innerText().then(text=>text.toLowerCase().includes('interface under review')));
await page.screenshot({path:path.join(output,'studio-chat.png'),fullPage:true});
for(const id of ['chat','source-chat','image','text','dashboard','free-agent']){
 await mode(page,id);verify(`Shared picker visible before send: ${id}`,await page.locator('.provider-chip').isVisible());
 await root(page,'agent');verify(`Compact shared menu: ${id}`,await page.locator('.menu-panel button').count()>=3&&await page.locator('.menu-panel p,.menu-panel label,.menu-panel textarea').count()===0);await page.keyboard.press('Escape');
}
await scenario(page,'source-chat');verify('Source-chat picker is present immediately',await page.locator('.provider-chip').isVisible());
await page.locator('#agent-prompt').fill('Unsent draft preserved through every layout.');
let old=await snapshot(page);
for(const value of ['window','messages','pairs']){await root(page,'settings');await submenu(page,'settings','presentation');await choose(page,'presentation',value);const next=await snapshot(page);verify(`Layout ${value} preserves draft and transcript`,next.state.prompt===old.state.prompt&&JSON.stringify(next.state.messages)===JSON.stringify(old.state.messages)&&next.state.runCount===old.state.runCount);}
await scenario(page,'image-independent');await selectProvider(page,'openai');
verify('Selecting API provider does not force credential entry',await page.locator('[role=dialog]').count()===0);
old=await snapshot(page);await openKeys(page);await choose(page,'change-key');
verify('Chevron opens dedicated masked key entry',await page.locator('#demo-key[type=password]').isVisible());
await page.locator('[data-action=sample-key]').click();await page.locator('[data-action=save-key]').click();let next=await snapshot(page);
verify('Key replacement retains prompt and existing results',next.state.keyState==='valid'&&next.state.prompt===old.state.prompt&&next.state.revisions.length===old.state.revisions.length);
verify('Credential text is never saved',!await page.evaluate(()=>JSON.stringify(localStorage).includes('demo-sample-key')));
await mode(page,'text');await selectProvider(page,'openai');verify('Provider credential status is shared across modes',(await snapshot(page)).state.keyState==='valid');
verify('API text mode offers a compatible sample model',(await snapshot(page)).state.model==='sample-api-text');
await scenario(page,'expired-key');old=await snapshot(page);await page.locator('[data-action=change-key]').click();await page.locator('[data-action=sample-key]').click();await page.locator('[data-action=save-key]').click();next=await snapshot(page);
verify('Expired key remains explicitly replaceable',next.state.keyState==='valid'&&next.state.prompt===old.state.prompt&&next.state.revisions.length===old.state.revisions.length);
await mode(page,'text');await selectProvider(page,'cursor');await page.locator('#agent-prompt').fill('Remember this text draft.');await mode(page,'chat');await mode(page,'text');
verify('Model choice and draft are remembered per mode',(await snapshot(page)).state.provider==='cursor'&&await page.locator('#agent-prompt').inputValue()==='Remember this text draft.');
await page.reload();verify('Model and draft survive a reload',(await snapshot(page)).state.provider==='cursor'&&await page.locator('#agent-prompt').inputValue()==='Remember this text draft.');
await page.locator('#global-access').click();await choose(page,'access','full');
const second=await context.newPage();await second.goto(base);await second.locator('.provider-chip').waitFor();verify('Access preference is global in another instance',(await snapshot(second)).access.policy==='full');
await second.locator('#global-access').click();await choose(second,'access','workspace');await page.waitForFunction(()=>window.AgentStudio.snapshot().access.policy==='workspace');
verify('Global access synchronizes across live instances',(await snapshot(page)).access.policy==='workspace');await second.close();await page.reload();verify('Global access survives reopening',(await snapshot(page)).access.policy==='workspace');
await scenario(page,'dashboard-steering');await root(page,'effort');await choose(page,'effort','High');verify('Supported effort changes apply during work',(await snapshot(page)).state.effectiveEffort==='High');
await page.locator('#studio-setup-toggle').click();await page.locator('#live-effort-support').uncheck();await page.locator('#studio-setup-toggle').click();await root(page,'effort');await choose(page,'effort','Low');next=await snapshot(page);verify('Unsupported effort changes are queued and labelled',next.state.effectiveEffort==='High'&&next.state.effort==='Low'&&!next.state.liveEffort);
await page.locator('#agent-prompt').fill('Make the timeline more compact.');await page.locator('[data-action=send]').click();verify('Running dashboard accepts a delivered follow-up',(await snapshot(page)).state.pendingSteering.startsWith('Delivered'));
await page.screenshot({path:path.join(output,'studio-dashboard.png'),fullPage:true});
await page.locator('[data-action=stop]').click();await scenario(page,'continuity-cutoff');await page.locator('#agent-prompt').fill('Keep this draft when the wire is cut.');old=await snapshot(page);await page.locator('.wire-action[data-action=cut-continuity]').click();next=await snapshot(page);
verify('Cut resets downstream context but retains draft and outputs',!next.state.continuity&&next.state.messages.length===0&&next.state.prompt===old.state.prompt&&next.state.revisions.length===old.state.revisions.length&&next.state.upstreamMessages.length>0);
const upstream=JSON.stringify(next.state.upstreamMessages);await page.locator('.wire-action[data-action=connect-continuity]').click();await page.locator('[data-action=send]').click();next=await snapshot(page);verify('Reconnected wire supplies the retained context to the next request',next.state.lastRunContext.inherited.length>0);
await page.locator('.wire-action[data-action=cut-continuity]').click();verify('Repeated cuts do not overwrite upstream history',JSON.stringify((await snapshot(page)).state.upstreamMessages)===upstream);
await scenario(page,'image-independent');await selectProvider(page,'codex');await page.locator('#agent-prompt').fill('Generate an independent study.');await page.locator('[data-action=send]').click();old=await snapshot(page);await page.locator('#agent-prompt').fill('This is a later draft.');await page.locator('#agent-prompt').press('Control+Enter');next=await snapshot(page);
verify('Keyboard follows independent-mode send eligibility',next.state.messages.length===old.state.messages.length&&next.state.queuedMessages.length===0&&next.state.prompt==='This is a later draft.');
verify('Independent image Play becomes the one Stop action',await page.locator('.composer-actions [data-action=stop]').count()===1&&await page.locator('.composer-actions .primary-button').innerText()==='Stop');await page.locator('.primary-button[data-action=stop]').click();
await root(page,'generation');await choose(page,'generation','inputs');await root(page,'image-options');await choose(page,'sample-input-change');verify('Input-change generation mode demonstrates an actual sample refresh',(await snapshot(page)).state.status==='running');await page.locator('.primary-button[data-action=stop]').click();
await page.locator('.result-thumb').nth(2).click();await page.locator('[data-action=open-result-menu]').click();await choose(page,'place-result');verify('Place on board renders a visible copy',await page.locator('.placed-result-card').count()>0||await page.locator('.placed-copy-card').count()>0);
await page.locator('[data-action=open-result-menu]').click();await choose(page,'source-chat');next=await snapshot(page);verify('Image-to-chat starts with the selected result and a clean dialogue',next.mode==='source-chat'&&next.state.sourceImageIndex===2&&next.state.messages.length===0&&await page.locator('.source-card-thumbnail svg').count()===1);
await scenario(page,'free-agent-pause');await page.locator('[data-action=pause]').click();verify('Pause is distinct from Stop',(await snapshot(page)).state.status==='paused'&&await page.locator('[data-action=resume]').isVisible());await page.locator('[data-action=resume]').click();verify('Resume continues the same task',(await snapshot(page)).state.status==='running');await page.locator('[data-action=stop]').click();verify('Stop has a distinct visible state',(await snapshot(page)).state.status==='stopped'&&await page.locator('#flow-status').innerText()==='Stopped');
await selectProvider(page,'cursor');await page.locator('#agent-prompt').fill('Start a sample task.');await page.locator('[data-action=send]').click();await page.locator('#agent-prompt').fill('Follow this up next.');await page.locator('[data-action=send]').click();verify('Unsupported running-message delivery is queued',(await snapshot(page)).state.queuedMessages.length===1);await page.locator('#agent-prompt').fill('Keep this unsent draft.');const count=(await snapshot(page)).state.runCount;await page.clock.runFor(17000);next=await snapshot(page);verify('Queued update really becomes the next turn without losing the draft',next.state.runCount===count+1&&next.state.messages.some(item=>item.text==='Follow this up next.')&&next.state.prompt==='Keep this unsent draft.');await page.locator('[data-action=stop]').click();
await scenario(page,'quiet-provider');old=await snapshot(page);await page.clock.runFor(5000);next=await snapshot(page);verify('Quiet service shows waiting and elapsed time without fabricated events',next.state.activity.length===old.state.activity.length&&next.state.elapsedSeconds>old.state.elapsedSeconds&&next.state.status==='waiting');
await page.locator('[data-action=stop]').click();await mode(page,'chat');await page.locator('[data-menu=more]').click();await choose(page,'rename');await page.locator('#conversation-name').fill('My design conversation');await page.locator('[data-action=save-name]').click();verify('Rename changes the conversation title',await page.locator('.scene-card-identity').innerText()==='My design conversation');
await page.locator('#studio-feedback-toggle').click();await page.locator('#design-comment').fill('Keep the model menu as simple as the right-click menu.');await mode(page,'image');await mode(page,'chat');verify('Design comments persist per mode',await page.locator('#design-comment').inputValue()==='Keep the model menu as simple as the right-click menu.');await page.locator('#studio-feedback-toggle').click();
for(const width of [1024,768,390,320]){await page.setViewportSize({width,height:1050});await page.clock.runFor(50);const bounds=await page.evaluate(()=>({scroll:document.documentElement.scrollWidth,width:innerWidth}));verify(`No document horizontal overflow at ${width}px`,bounds.scroll<=bounds.width+1);}
await page.setViewportSize({width:390,height:1000});await root(page,'agent');await submenu(page,'agent','providers');const rect=await page.locator('.menu-panel').last().boundingBox();verify('Nested menu stays inside a narrow viewport',rect.x>=0&&rect.x+rect.width<=390);await page.keyboard.press('Escape');await page.keyboard.press('Escape');
await page.screenshot({path:path.join(output,'studio-mobile.png'),fullPage:true});
await page.setViewportSize({width:1440,height:1050});await page.locator('#studio-setup-toggle').click();await page.locator('#theme-toggle').click();await page.locator('#studio-setup-toggle').click();await page.screenshot({path:path.join(output,'studio-dark.png'),fullPage:true});
verify('No browser runtime errors',result.errors.length===0);
const contextObject={window:{}};vm.runInNewContext(await fs.readFile(path.join(__dirname,'data.js'),'utf8'),contextObject);const preferences=JSON.parse(await fs.readFile(path.join(__dirname,'survey-preferences.json'),'utf8'));verify('Survey inventory remains 78 rows',contextObject.window.AgentStudioData.patterns.length===78);verify('Submitted preference data retained',Object.keys(preferences.choices||preferences.submittedState?.choices||preferences.state?.choices||{}).length>0);
await fs.writeFile(path.join(output,'checks.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result,null,2));
}finally{await browser.close();}
})().catch(async error=>{result.errors.push(error.stack);await fs.mkdir(output,{recursive:true});await fs.writeFile(path.join(output,'checks.json'),JSON.stringify(result,null,2));console.error(error);process.exitCode=1;});
