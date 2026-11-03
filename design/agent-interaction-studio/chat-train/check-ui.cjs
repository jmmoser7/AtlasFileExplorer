/* Browser behavior smoke for the local chat-train baseline. No production app or provider is used. */
'use strict';

const assert = require('node:assert/strict');
const os = require('node:os');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || path.join(
  os.homedir(), '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
));

const url = process.env.CHAT_TRAIN_URL || 'http://127.0.0.1:8765/';
const result = { url, scope: 'Local prototype behavior; no native pixel-fidelity or live-provider claim.', checks: [], issues: [], browserErrors: [] };
const verify = (name, condition) => {
  assert.ok(condition, name);
  result.checks.push(name);
};
const inspect = page => page.evaluate(() => {
  const app = window.ChatTrainStudio;
  return {
    presentation: app.state.presentation,
    status: app.state.status,
    running: app.state.running,
    turns: app.state.turns,
    drafts: app.state.drafts,
    usage: app.state.usage,
    cards: app.cards,
    camera: app.camera
  };
});
const cardLocator = (page, id) => page.locator(`.chat-card[data-id="${id}"]`);
const ready = async page => {
  await page.waitForFunction(() => Boolean(window.ChatTrainStudio));
  await page.locator('.chat-card').first().waitFor();
};
const reset = async page => {
  await page.keyboard.press('Escape');
  await page.locator('#reset-example').click();
  await page.waitForFunction(() => window.ChatTrainStudio.state.turns.length === 6 && !window.ChatTrainStudio.state.running);
};
const fit = page => page.locator('#fit-view').click();
const tail = async page => (await inspect(page)).cards.find(card => card.isTail && !card.isDraft);
const openCardMenu = async (page, id) => {
  await cardLocator(page, id).getByRole('button', { name: 'Chat menu', exact: true }).click();
  await page.getByRole('menu').waitFor();
};

(async () => {
  const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe'
  });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
  const page = await context.newPage();
  page.on('pageerror', error => result.browserErrors.push(error.message));
  page.setDefaultTimeout(5000);
  const run = async (name, body) => {
    try { await body(); }
    catch (error) { result.issues.push({ scenario: name, message: error.message }); }
  };

  try {
    await page.clock.install();
    await page.goto(url);
    await ready(page);

    await run('Initial chat train', async () => {
      const state = await inspect(page);
      verify('Initial view contains three message-pair cards', state.presentation === 'pairs' && state.cards.length === 3 && state.cards.every(card => card.turns.length === 2));
      verify('Exactly one tail composer is present', await page.locator('.card-composer').count() === 1);
      verify('Cards sit directly on the canvas without a conversation wrapper', await page.evaluate(() =>
        [...document.querySelectorAll('.chat-card')].every(card => card.parentElement.id === 'cards') &&
        !document.querySelector('.conversation-wrapper,.conversation-window,.chat-wrapper,.review-frame,.speech-bubble')
      ));
    });

    await run('Continuation, send, and Stop', async () => {
      await reset(page);
      const before = await inspect(page);
      const last = before.cards.find(card => card.isTail);
      await cardLocator(page, last.id).getByRole('button', { name: 'Continue chat', exact: true }).click();
      let state = await inspect(page);
      verify('Output click creates an unsent continuation without changing history', state.cards.some(card => card.isDraft) && state.drafts.length === 1 && state.turns.length === before.turns.length && !state.running);
      verify('Continuation keeps exactly one active composer', await page.locator('.card-composer').count() === 1);
      await page.locator('.card-composer').fill('Add two trees to this courtyard.');
      await page.locator('.card-composer').press('Enter');
      await page.waitForFunction(() => Boolean(window.ChatTrainStudio.state.running));
      verify('Enter starts a local sample response', (await inspect(page)).turns.length === before.turns.length + 2 && await page.getByRole('button', { name: 'Stop response', exact: true }).count() === 1);
      await page.clock.runFor(420);
      state = await inspect(page);
      const assistant = state.turns.find(turn => turn.id === state.running.assistantId);
      verify('Sample reply visibly streams a partial response', Boolean(assistant?.text.length) && assistant.text.length < 100);
      const partial = assistant.text;
      await page.getByRole('button', { name: 'Stop response', exact: true }).click();
      state = await inspect(page);
      verify('Stop retains the partial assistant text and ends the run', !state.running && state.status === 'stopped' && state.turns.find(turn => turn.id === assistant.id).text === partial);
      await page.clock.runFor(1000);
      verify('Stopped text does not continue growing', (await inspect(page)).turns.find(turn => turn.id === assistant.id).text === partial);
    });

    await run('Presentation changes preserve conversation and draft', async () => {
      await reset(page);
      const draft = 'Keep this unsent draft through every presentation.';
      await page.locator('.card-composer').fill(draft);
      const before = await inspect(page);
      for (const [presentation, label] of [['messages', 'Chat train'], ['window', 'Single chat window'], ['pairs', 'Message pairs']]) {
        await fit(page);
        const currentTail = (await inspect(page)).cards.find(card => card.isTail);
        await openCardMenu(page, currentTail.id);
        await page.getByRole('menuitem', { name: label, exact: true }).click();
        const next = await inspect(page);
        verify(`${label} preserves canonical turns without starting inference`, next.presentation === presentation && JSON.stringify(next.turns) === JSON.stringify(before.turns) && !next.running && JSON.stringify(next.usage) === JSON.stringify(before.usage));
        verify(`${label} preserves the unsent draft`, await page.locator('.card-composer').count() === 1 && await page.locator('.card-composer').inputValue() === draft);
      }
    });

    await run('Historical branch', async () => {
      await reset(page);
      await fit(page);
      const before = await inspect(page), historical = before.cards[0];
      await openCardMenu(page, historical.id);
      await page.getByRole('menuitem', { name: 'Fork chat here', exact: true }).click();
      const state = await inspect(page), branch = state.drafts.find(draft => draft.fork);
      verify('Fork creates a draft at the selected historical checkpoint', Boolean(branch) && branch.parentId === historical.sourceTurnId && branch.sessionId !== historical.sessionId);
      verify('Fork preserves the original branch and sends no request', JSON.stringify(state.turns) === JSON.stringify(before.turns) && !state.running && state.cards.filter(card => !card.isDraft).length === before.cards.length);
    });

    await run('Title and model choice', async () => {
      await reset(page);
      const last = await tail(page);
      await cardLocator(page, last.id).getByRole('button', { name: 'Rename chat', exact: true }).click();
      await page.getByRole('textbox', { name: 'Chat title', exact: true }).fill('Courtyard review');
      await page.getByRole('textbox', { name: 'Chat title', exact: true }).press('Enter');
      verify('Title click renames the selected conversation', (await inspect(page)).cards.some(card => card.title === 'Courtyard review'));
      const renamed = await tail(page);
      await cardLocator(page, renamed.id).getByRole('button', { name: 'Agent settings', exact: true }).click();
      await page.getByRole('menuitem', { name: 'Model: Auto', exact: true }).click();
      await page.getByRole('menuitem', { name: 'Astra', exact: true }).click();
      verify('Model selection updates the next response choice without running', (await tail(page)).model === 'Astra' && !(await inspect(page)).running);
    });

    await run('Draft survives reload', async () => {
      await reset(page);
      const draft = 'Save this unfinished message for later.';
      await page.locator('.card-composer').fill(draft);
      const before = await inspect(page);
      await page.reload();
      await ready(page);
      verify('Unsent composer text survives reload', await page.locator('.card-composer').inputValue() === draft);
      verify('Reload retains history and does not submit the draft', JSON.stringify((await inspect(page)).turns) === JSON.stringify(before.turns) && !(await inspect(page)).running);
    });

    await run('Zoom preserves world-space wrapping', async () => {
      await reset(page);
      await page.locator('.card-composer').fill('A longer draft with enough words to wrap into several lines inside the same world-space text column as the board camera zoom changes.');
      const geometry = async () => page.evaluate(() => ({
        cards: window.ChatTrainStudio.cards.map(card => ({ id: card.id, width: card.width, height: card.height, text: card.turns.map(turn => turn.text) })),
        text: [...document.querySelectorAll('.turn,.card-composer')].map(node => ({ width: node.clientWidth, height: node.clientHeight, value: node.value ?? node.textContent }))
      }));
      const before = await geometry(page), zoomBefore = (await inspect(page)).camera.zoom;
      await page.getByRole('button', { name: 'Zoom in', exact: true }).click();
      verify('Zoom control changes the canvas camera', (await inspect(page)).camera.zoom > zoomBefore);
      verify('Zoom leaves world geometry and text wrapping unchanged', JSON.stringify(await geometry(page)) === JSON.stringify(before));
      await page.getByRole('button', { name: 'Reset to 100 percent', exact: true }).click();
      verify('Reset zoom returns to 100 percent', (await inspect(page)).camera.zoom === 1);
    });

    verify('No browser runtime errors', result.browserErrors.length === 0);
  } catch (error) {
    result.issues.push({ scenario: 'Browser setup or uncaught check', message: error.message });
  } finally {
    await context.close();
    await browser.close();
    result.passed = result.checks.length;
    result.failed = result.issues.length;
    process.stdout.write(JSON.stringify(result, null, 2) + '\n');
    if (result.failed || result.browserErrors.length) process.exitCode = 1;
  }
})().catch(error => { process.stderr.write(error.stack + '\n'); process.exitCode = 1; });
