/* Actual browser screenshots only. Run against the existing local Studio server. */
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || path.join(os.homedir(),
  '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'));
const base = new URL(process.env.STUDIO_URL || 'http://127.0.0.1:8765/');
assert.ok(['127.0.0.1', 'localhost'].includes(base.hostname), 'Use the local Studio only');
const viewport = { width: 960, height: 900 };
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const imageExamples = ['small', 'medium', 'large', 'inputs', 'series'];

async function stable(page, canvas) {
  await page.evaluate(() => document.fonts.ready);
  await page.waitForFunction(selector => {
    const body = document.querySelector(selector), header = document.querySelector('.studio-bar');
    return body && !body.hidden && body.getBoundingClientRect().top >= header.getBoundingClientRect().bottom - .5;
  }, canvas);
  await page.evaluate(async selector => {
    const geometry = () => {
      const body = document.querySelector(selector);
      return [body.getBoundingClientRect().toJSON(), ...[...body.querySelectorAll('[style*="transform"]')]
        .map(node => node.style.transform)];
    };
    let previous = '', same = 0;
    for (let frame = 0; frame < 120; frame++) {
      await new Promise(requestAnimationFrame);
      const current = JSON.stringify(geometry());
      same = current === previous ? same + 1 : 0;
      if (same >= 3) return;
      previous = current;
    }
    throw new Error('Canvas geometry did not settle');
  }, canvas);
  await page.evaluate(async () => {
    await Promise.all([...document.images].filter(image => !image.closest('[hidden]')).map(image => {
      if (image.complete) return Promise.resolve();
      return new Promise((resolve, reject) => {
        image.addEventListener('load', resolve, { once: true });
        image.addEventListener('error', () => reject(new Error(`Image failed: ${image.src}`)), { once: true });
      });
    }));
  });
}

(async () => {
  const registry = {};
  vm.runInNewContext(await fs.readFile(path.join(__dirname, 'studio-model.js'), 'utf8'), registry);
  const elements = registry.StudioPortalModel.elements(null);
  const prior = { window: { StudioPortalData: { images: {} } } };
  try { vm.runInNewContext(await fs.readFile(path.join(__dirname, 'js-baselines.js'), 'utf8'), prior); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  const priorEntries = Object.values(prior.window.StudioPortalData.images).flat();
  const stamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d+Z$/, 'Z');
  const relativeDirectory = `assets/js-baselines/20261103-${stamp.slice(9)}`;
  const directory = path.join(__dirname, relativeDirectory);
  await fs.mkdir(directory, { recursive: true });
  assert.equal((await fs.readdir(directory)).length, 0, 'Never overwrite an earlier capture set');
  const browser = await chromium.launch({ headless: true,
    executablePath: process.env.EDGE_PATH || 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' });
  const entries = [], allHashes = new Map();
  try {
    for (const element of elements.filter(element => element.kind !== 'blank')) {
      const examples = element.id === 'image-generator' ? imageExamples : [null];
      for (const example of examples) {
        const context = await browser.newContext({ viewport, deviceScaleFactor: 1, colorScheme: 'light' });
        const blocked = [];
        await context.route('**/*', route => {
          const url = new URL(route.request().url());
          if (['data:', 'blob:'].includes(url.protocol) || url.origin === base.origin) return route.continue();
          blocked.push(url.href); return route.abort();
        });
        const page = await context.newPage(), errors = [], pending = [], hashes = new Map();
        page.on('pageerror', error => errors.push(error.message));
        page.on('response', response => {
          const url = new URL(response.url());
          const name = decodeURIComponent(url.pathname.replace(/^\//, '') || 'index.html');
          if (url.origin !== base.origin || !/\.(html|js|css|json)$/.test(name) || name === 'js-baselines.js') return;
          pending.push((async () => {
            assert.ok(response.ok(), `${response.status()} ${url}`);
            hashes.set(name, hash(await response.body()));
          })());
        });
        const home = element.id === 'studio';
        const sourceUrl = new URL(home ? '?sample=1' : `?phase=01&tab=${element.id}&sample=1`, base).href;
        await page.goto(sourceUrl, { waitUntil: 'networkidle' });
        const canvas = home ? '#portal-home' : element.id === 'chat-train' ? '#canvas'
          : element.id === 'image-generator' ? '#image-generator-canvas' : `#${element.id}-canvas`;
        await page.locator(canvas).waitFor({ state: 'visible' });
        await stable(page, canvas);
        const actions = [];
        if (example) {
          await page.locator(`[data-image-scene="${example}"]`).click();
          actions.push(`Clicked existing ${example} image-example footer button`);
          await page.waitForFunction(name => document.querySelector(`[data-image-scene="${name}"]`)?.getAttribute('aria-current') === 'true', example);
        } else if (element.id === 'frame') {
          await page.locator('#frame-fit').click(); actions.push('Clicked existing Frame Fit control');
        } else if (!home && element.id !== 'chat-train') {
          await page.locator(`.element-footer[data-element-studio="${element.id}"]`).getByRole('button', { name: 'Fit', exact: true }).click();
          actions.push('Clicked existing element Fit control');
        }
        await page.mouse.move(950, 880);
        await stable(page, canvas);
        await Promise.all(pending);
        assert.deepEqual(errors, [], `Page errors: ${element.id}`);
        assert.deepEqual(blocked, [], 'Unexpected external resource request; no external access allowed');
        const observed = await page.evaluate(selector => {
          const rect = document.querySelector(selector).getBoundingClientRect();
          const header = document.querySelector('.studio-bar').getBoundingClientRect();
          const visible = selector => [...document.querySelectorAll(selector)].filter(node => node.getClientRects().length);
          const tabLabels = visible('.studio-tab').map(node => ({ text: node.textContent,
            width: node.clientWidth, scrollWidth: node.scrollWidth }));
          const relevantWorld = document.querySelector(`${selector} .element-world,${selector} #image-world`);
          return { theme: document.documentElement.dataset.theme || 'light', devicePixelRatio,
            browserZoom: visualViewport.scale, canvas: rect.toJSON(), header: header.toJSON(), tabLabels,
            canvasTransform: relevantWorld ? getComputedStyle(relevantWorld).transform : null,
            chat: selector === '#canvas' ? { presentation: ChatTrainStudio.state.presentation,
              turnCount: ChatTrainStudio.state.turns.length, camera: ChatTrainStudio.camera } : null,
            frame: selector === '#frame-canvas' ? FrameStudio.metrics() : null,
            openMenus: visible('.native-menu,.menu-panel,[role="dialog"],#studio-options-menu').length };
        }, canvas);
        assert.equal(observed.openMenus, 0, 'Baseline must show no incidental open menu');
        assert.ok(observed.tabLabels.every(label => label.scrollWidth <= label.width + 1), 'Tab label clipped');
        const filename = `${element.id}${example ? '-' + example : ''}-js-baseline.png`;
        const capturedAt = new Date().toISOString();
        await page.screenshot({ path: path.join(directory, filename), fullPage: false });
        const sourceHashes = Object.fromEntries([...hashes].sort(([a], [b]) => a.localeCompare(b)));
        for (const [name, digest] of hashes) {
          if (allHashes.has(name)) assert.equal(allHashes.get(name), digest, `Source changed during captures: ${name}`);
          allHashes.set(name, digest);
        }
        entries.push({ id: `js-baseline-${stamp}-${element.id}${example ? '-' + example : ''}`,
          elementId: element.id, title: `JS baseline · ${home ? 'Studio home' : element.name}${example ? ' · ' + example : ''}`,
          src: `${relativeDirectory}/${filename}`, download: `${relativeDirectory}/${filename}`,
          caption: `Actual current JavaScript Studio fixture${example ? ' (' + example + ')' : ''}. Simulated providers; no Rust/native parity or automatic approval.`,
          approval: 'baseline-capture', sourcePhase: '01', sourceUrl, jsSourceUrl: sourceUrl,
          viewport, captureViewport: viewport, capture: { url: sourceUrl, viewport },
          theme: observed.theme, capturedAt, captureKind: 'browser-screenshot',
          browser: { name: 'Microsoft Edge', version: browser.version(), headless: true },
          dpi: 96 * observed.devicePixelRatio, devicePixelRatio: observed.devicePixelRatio,
          browserZoom: observed.browserZoom, fixture: home ? 'Actual Studio home, not recursive gallery'
            : example ? `Existing ${example} footer example` : element.id === 'chat-train'
              ? 'Default long conversation, existing pairs mode, unmodified sample=1' : 'Current default sample fixture',
          actions, observed, relevantSourceHashes: sourceHashes,
          screenshotSHA256: hash(await fs.readFile(path.join(directory, filename))) });
        console.log(`Captured ${element.id}${example ? '/' + example : ''}`);
        await context.close();
      }
    }
    for (const [name, digest] of allHashes) {
      assert.equal(hash(await fs.readFile(path.join(__dirname, name))), digest, `Source changed: ${name}`);
    }
    assert.equal(entries.length, 11);
    const manifest = { version: 1, createdAt: new Date().toISOString(), base: base.href,
      scope: 'Current JS fixtures only; isolated browser contexts; no user profiles, runtime services or native claims.', entries };
    await fs.writeFile(path.join(directory, 'capture-manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
    const catalog = '/* Actual JS screenshots; generated by capture-baselines.cjs. Prior gallery entries are preserved. */\n'
      + '(() => {\n  const entries = ' + JSON.stringify([...priorEntries, ...entries], null, 2) + ';\n'
      + '  const images = (window.StudioPortalData ||= {}).images ||= {};\n'
      + '  for (const entry of entries) {\n    const list = images[entry.elementId] ||= [];\n'
      + '    if (!list.some(item => item.id === entry.id)) list.push(entry);\n  }\n})();\n';
    await fs.writeFile(path.join(__dirname, 'js-baselines.js'), catalog);
    console.log(`PASS: ${entries.length} captures; ${relativeDirectory}/capture-manifest.json`);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
