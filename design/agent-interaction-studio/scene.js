(function () {
  'use strict';

  const escape = value => String(value ?? '').replace(/[&<>"']/g, char => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  }[char]));
  const lines = value => escape(value).replace(/\r?\n/g, '<br>');
  const number = value => Math.max(0, Number(value) || 0);
  const clock = seconds => {
    const value = Math.floor(number(seconds));
    return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, '0')}`;
  };
  const button = (action, text, className = '', extra = '') =>
    `<button type="button" class="${className}" data-action="${action}" ${extra}>${text}</button>`;

  const labels = {
    chat: ['Chat', 'A conversation on the canvas'],
    'source-chat': ['Chat from an item', 'The source stays attached to the conversation'],
    image: ['Image generator', 'Keep the result visible as the conversation continues'],
    text: ['Text generator', 'Read, revise, and compare the generated text'],
    dashboard: ['Dashboard generator', 'Preview a revision before applying it'],
    'free-agent': ['Free agent', 'One conversation, with work and replies together']
  };

  function sceneHeader(state, title) {
    return `<header class="scene-card-header"><div class="scene-card-identity"><span class="scene-card-dot" aria-hidden="true"></span><span>${escape(title)}</span></div><div class="scene-toolbar-slot" data-toolbar-slot></div></header>`;
  }

  function imageSvg(index, thumbnail = false, suffix = 'main') {
    const palettes = [
      ['#dce7e5', '#617e81', '#263e45', '#e7c8a3'],
      ['#eadbd0', '#9b7167', '#493d46', '#dbaa71'],
      ['#dbe0ed', '#7785a6', '#343e60', '#d8bd91'],
      ['#e2e6d1', '#899476', '#3c5149', '#e4c997']
    ];
    const colors = palettes[Math.floor(number(index)) % palettes.length];
    const id = `scene-sky-${Math.floor(number(index))}-${suffix}`;
    return `<svg class="sample-image" viewBox="0 0 800 430" role="img" aria-label="Architectural image study, variation ${Math.floor(number(index)) + 1}" preserveAspectRatio="xMidYMid slice"><defs><linearGradient id="${id}" x2="0" y2="1"><stop stop-color="${colors[0]}"/><stop offset="1" stop-color="#f5efe3"/></linearGradient></defs><rect width="800" height="430" fill="url(#${id})"/><circle cx="620" cy="105" r="45" fill="${colors[3]}" opacity=".8"/><path d="M0 260 160 205 310 250 470 195 800 260V430H0Z" fill="${colors[1]}" opacity=".28"/><path d="M0 346 800 300V430H0Z" fill="${colors[1]}"/><path d="m151 312 214-35 265 31-220 53Z" fill="${colors[2]}" opacity=".2"/><path d="m237 145 193-35 164 55-194 37Z" fill="#fff9ed"/><path d="m237 145 163 57v127l-163-58Z" fill="${colors[3]}"/><path d="m400 202 194-37v127l-194 37Z" fill="${colors[2]}"/><path d="m273 194 96 33v62l-96-33Z" fill="${colors[2]}"/><path d="m427 218 139-27v76l-139 28Z" fill="${colors[0]}" opacity=".72"/><path d="m473 209 1 77m46-86 1 77" stroke="${colors[2]}" stroke-width="5"/><path d="m208 287 192 68 222-46" fill="none" stroke="#f8f1e4" stroke-width="8"/><path d="M110 286v65m-20-35 20-38 20 38" stroke="${colors[2]}" stroke-width="7" stroke-linecap="round" fill="none"/><path d="M670 261v65m-20-35 20-38 20 38" stroke="${colors[2]}" stroke-width="7" stroke-linecap="round" fill="none"/>${thumbnail ? '' : '<path d="M30 400h42" stroke="#fff" stroke-width="2" opacity=".7"/>'}</svg>`;
  }

  function dashboardSvg(index) {
    const variation = Math.floor(number(index)) % 3;
    const accent = ['#587f79', '#a27353', '#5c71a4'][variation];
    const bars = [70, 112, 92, 147, 126, 174].map((height, i) =>
      `<rect x="${96 + i * 61}" y="${335 - height}" width="33" height="${height}" rx="4" fill="${accent}" opacity="${.4 + i * .11}"/><text x="${112 + i * 61}" y="354" text-anchor="middle">${['May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct'][i]}</text>`).join('');
    return `<svg class="sample-dashboard" viewBox="0 0 800 430" role="img" aria-label="Sample project activity dashboard"><rect width="800" height="430" fill="#f5f6f2"/><g font-family="system-ui,sans-serif" fill="#36413b"><rect width="800" height="48" fill="#fff"/><circle cx="29" cy="24" r="8" fill="${accent}"/><text x="48" y="29" font-size="14" font-weight="600">Project overview</text><text x="668" y="28" font-size="11" fill="#7e877f">May – October</text><g font-size="11" fill="#788078"><text x="40" y="86">COMPLETED</text><text x="286" y="86">IN PROGRESS</text><text x="532" y="86">ON SCHEDULE</text></g><g font-size="33" font-weight="600"><text x="40" y="124">${128 + variation * 9}</text><text x="286" y="124">${24 - variation * 2}</text><text x="532" y="124">${91 + variation}%</text></g><path d="M40 145H760" stroke="#dfe4dc"/><text x="40" y="177" font-size="13" font-weight="600">Completed by month</text><g font-size="9" fill="#7e877f"><path d="M75 220H465M75 277H465M75 335H465" stroke="#e1e5de"/>${bars}</g><rect x="504" y="167" width="256" height="211" rx="9" fill="#fff"/><text x="524" y="195" font-size="13" font-weight="600">Work by team</text><g font-size="11"><text x="524" y="229">Design</text><text x="524" y="276">Engineering</text><text x="524" y="323">Operations</text></g><g fill="#e6ebe4"><rect x="524" y="239" width="210" height="9" rx="4"/><rect x="524" y="286" width="210" height="9" rx="4"/><rect x="524" y="333" width="210" height="9" rx="4"/></g><g fill="${accent}"><rect x="524" y="239" width="162" height="9" rx="4"/><rect x="524" y="286" width="193" height="9" rx="4"/><rect x="524" y="333" width="126" height="9" rx="4"/></g><text x="40" y="403" font-size="10" fill="#899187">Sample data · selected revision ${Math.floor(number(index)) + 1}</text></g></svg>`;
  }

  function messages(state) {
    return Array.isArray(state.messages) ? state.messages : [];
  }

  function message(message, index, bubbles = true) {
    const role = message.role === 'user' ? 'user' : 'assistant';
    return `<div class="chat-message is-${role}${bubbles ? ' is-bubble' : ''}" data-message-index="${index}"><span class="message-role">${role === 'user' ? 'You' : 'Agent'}</span><div class="message-text">${lines(message.text)}</div></div>`;
  }

  function conversation(state, forceWindow = false) {
    const entries = messages(state);
    if (!entries.length) {
      return '<div class="scene-empty"><span class="empty-mark" aria-hidden="true">↗</span><p>Start the conversation below.</p></div>';
    }
    if (forceWindow || state.presentation === 'window') {
      return `<div class="conversation-window"><div class="conversation-transcript">${entries.map((entry, index) => message(entry, index)).join('')}</div></div>`;
    }
    let groups;
    if (state.presentation === 'messages') {
      groups = entries.map((entry, index) => [{ entry, index }]);
    } else {
      groups = [];
      entries.forEach((entry, index) => {
        if (entry.role === 'user' || !groups.length) groups.push([]);
        groups[groups.length - 1].push({ entry, index });
      });
    }
    return `<div class="conversation-train is-${state.presentation === 'messages' ? 'messages' : 'pairs'}" aria-label="Conversation train">${groups.map((group, index) => `${index ? '<span class="train-connector" aria-hidden="true">→</span>' : ''}<article class="train-card"><div class="train-card-header"><span>${state.presentation === 'messages' ? 'Message' : 'Turn'} ${index + 1}</span><span class="train-history-port" title="Conversation history" aria-hidden="true"></span></div>${group.map(item => message(item.entry, item.index, false)).join('')}</article>`).join('')}</div>`;
  }

  function revisionState(state) {
    const revisions = Array.isArray(state.revisions) ? state.revisions : [];
    const selected = Math.min(revisions.length - 1, Math.floor(number(state.selectedRevision)));
    return { revisions, selected, current: revisions[selected] };
  }

  function revisions(state, gallery = false) {
    const { revisions: entries, selected } = revisionState(state);
    if (!entries.length) return '';
    return `<div class="${gallery ? 'result-gallery' : 'revision-strip'}" role="group" aria-label="${gallery ? 'Generated image results' : 'Artifact revisions'}">${entries.map((entry, index) => button('select-revision', `${gallery ? `<span class="result-thumb-image">${imageSvg(entry.variant ?? index, true, `thumb-${index}`)}</span>` : '<span class="revision-marker" aria-hidden="true"></span>'}<span>${escape(entry.title || `Revision ${index + 1}`)}</span>`, `${gallery ? 'result-thumb' : 'revision-tab'}${index === selected ? ' is-selected' : ''}`, `data-value="${index}" aria-pressed="${index === selected}"`)).join('')}</div>`;
  }

  function continuity(state) {
    const connected = Boolean(state.continuity);
    const contextTitle = state.sourceTitle || (state.modality === 'image' ? 'Previous image + conversation' : 'Previous dashboard + conversation');
    const inputs = state.modality === 'image' ? ['Primary image', 'Text', 'Style image'] : ['Source', 'Data', 'Reference'];
    return `<div class="continuity-rail${connected ? ' is-connected' : ' is-disconnected'}"><div class="continuity-node"><span class="continuity-node-label">Conversation context</span><strong>${escape(contextTitle)}</strong><span class="continuity-context-state">${connected ? 'Connected to this generator' : 'Available to connect'}</span></div><div class="continuity-link"><svg class="continuity-wire" viewBox="0 0 150 54" aria-hidden="true" preserveAspectRatio="none"><path d="M3 27C45 27 105 27 147 27" fill="none" stroke="currentColor" stroke-width="2" ${connected ? '' : 'stroke-dasharray="5 5"'}/><circle cx="3" cy="27" r="3" fill="currentColor"/><circle cx="147" cy="27" r="3" fill="currentColor"/></svg>${button(connected ? 'cut-continuity' : 'connect-continuity', connected ? '<span aria-hidden="true">✂</span><span>Cut wire</span>' : '<span aria-hidden="true">+</span><span>Connect</span>', 'wire-action', `aria-label="${connected ? 'Cut the continuity wire and start fresh downstream; keep all results' : 'Connect conversation context to the fourth input'}"`)}</div><div class="input-ports" aria-label="Generator inputs">${inputs.map((name, index) => `<span class="input-port"><i aria-hidden="true"></i><span>${index + 1}. ${name}</span></span>`).join('')}${button(connected ? 'cut-continuity' : 'connect-continuity', '<i aria-hidden="true"></i><span>4. Continuity</span>', `input-port continuity-port${connected ? ' is-connected' : ''}`, `aria-pressed="${connected}"`)}</div><p class="continuity-help">${connected ? 'The next turn continues with this context.' : 'The next turn starts fresh. Existing outputs stay available.'}</p></div>`;
  }

  function activity(state) {
    const entries = Array.isArray(state.activity) ? state.activity : [];
    const status = ['running', 'paused', 'waiting', 'approval', 'error', 'completed', 'stopped'].includes(state.status) ? state.status : 'idle';
    const statusText = {
      idle: 'Ready', running: 'Working', paused: 'Paused', waiting: 'Waiting for provider',
      approval: 'Waiting for permission', error: 'Run interrupted', completed: 'Complete', stopped: 'Stopped'
    }[status];
    const latest = entries.length ? entries[entries.length - 1] : null;
    const visible = state.activityExpanded ? entries : entries.slice(-2);
    return `<section class="activity-panel is-${status}${state.activityExpanded ? ' is-expanded' : ''}" aria-label="Public activity"><div class="activity-heading"><span class="activity-status"><i class="activity-status-dot" aria-hidden="true"></i>${statusText}</span><span class="activity-meta">${status !== 'idle' ? `${clock(state.elapsedSeconds)} elapsed` : 'No run in progress'}${latest && status !== 'idle' ? ` · Last update ${clock(state.lastEventSeconds)} ago` : ''}</span>${button('expand-activity', `${state.activityExpanded ? '−' : '+'}<span>${state.activityExpanded ? 'Less activity' : 'More activity'}</span>`, 'activity-expand', `aria-expanded="${Boolean(state.activityExpanded)}"`)}</div><div class="activity-log" aria-live="polite">${visible.length ? visible.map(entry => `<div class="activity-row is-${['commentary', 'tool', 'status', 'error'].includes(entry.kind) ? entry.kind : 'status'}"><span class="activity-event-dot" aria-hidden="true"></span><span>${lines(entry.text)}</span>${entry.time == null ? '' : `<time>${escape(entry.time)}</time>`}</div>`).join('') : `<div class="activity-row activity-empty"><span>${status === 'idle' ? 'Public updates will appear here when work starts.' : 'No public update received yet.'}</span></div>`}</div>${status === 'waiting' ? '<p class="activity-wait-note">The provider has not sent a new event. Elapsed time continues; no additional work is being claimed.</p>' : ''}${state.pendingSteering ? `<div class="pending-steering"><span class="pending-steering-label">Next direction</span><span>${lines(state.pendingSteering)}</span></div>` : ''}</section>`;
  }

  function imageContent(state) {
    const { current, selected } = revisionState(state);
    const dimensions = typeof current?.aspect === 'string' && /^\d+:\d+$/.test(current.aspect) ? current.aspect.split(':').map(Number) : [3, 2];
    const [width, height] = dimensions.every(value => Number.isFinite(value) && value > 0) ? dimensions : [3, 2];
    const metadata = `${width}:${height}${current?.seed == null ? '' : ` · Seed ${escape(current.seed)}`}`;
    return `${continuity(state)}${state.interactive ? `<div class="generator-dialogue"><div class="section-kicker">Conversation <span class="conversation-mode-label">Interactive</span></div>${conversation(state)}</div>` : '<div class="generation-mode-note"><span class="mode-dot" aria-hidden="true"></span>Single image request</div>'}<div class="result-preview" style="aspect-ratio:${width} / ${height}">${current ? `${imageSvg(current.variant ?? selected)}<div class="result-preview-caption"><span class="result-preview-label"><span>${escape(current.title || `Result ${selected + 1}`)}</span><small>${metadata}</small></span>${button('open-result-menu', '<span aria-hidden="true">···</span><span class="sr-only">Result actions</span>', 'result-menu-button', `data-value="${selected}" aria-label="Open result actions"`)}</div>` : '<div class="scene-empty"><span class="empty-mark" aria-hidden="true">◇</span><p>Your generated image will appear here.</p></div>'}</div>${revisions(state, true)}${state.interactive && current ? `<div class="artifact-actions">${button('fork', 'Fork conversation', 'scene-text-button')}${button('open-memory', 'Inspect context', 'scene-text-button')}</div>` : ''}`;
  }

  function dashboardContent(state) {
    const { current, selected } = revisionState(state);
    return `${continuity(state)}${revisions(state)}<div class="result-preview dashboard-preview">${current ? dashboardSvg(selected) : '<div class="scene-empty"><span class="empty-mark" aria-hidden="true">▥</span><p>Your dashboard preview will appear here.</p></div>'}</div>${current ? `<div class="artifact-actions"><span class="artifact-preview-label">Selected preview · ${escape(current.title || `Revision ${selected + 1}`)}</span>${button('apply', 'Apply preview', 'scene-primary-button')}${button('discard', 'Discard', 'scene-text-button')}${button('fork', 'Fork', 'scene-text-button')}${button('open-memory', 'Inspect memory', 'scene-text-button')}</div>` : ''}${state.interactive ? `<div class="generator-dialogue dashboard-dialogue"><div class="section-kicker">Conversation <span class="conversation-mode-label">Interactive</span></div>${conversation(state)}</div>` : ''}`;
  }

  function textContent(state) {
    const entries = messages(state);
    const output = [...entries].reverse().find(entry => entry.role === 'assistant');
    return `${revisions(state)}<div class="text-output"><div class="section-kicker">Generated text</div>${output ? `<div class="generated-text">${lines(output.text)}</div>` : '<div class="scene-empty"><p>Your generated text will appear here.</p></div>'}</div>`;
  }

  function sourceCard(state) {
    const hasImage = typeof state.sourceImageIndex === 'number' && Number.isFinite(state.sourceImageIndex);
    const thumbnail = hasImage ? `<span class="source-card-thumbnail">${imageSvg(state.sourceImageIndex, true, 'source')}</span>` : '<span class="source-card-icon" aria-hidden="true">▧</span>';
    return `<aside class="scene-source-card${hasImage ? ' has-image' : ''}">${thumbnail}<span class="source-card-label">Attached source</span><strong>${escape(state.sourceTitle || 'Selected item')}</strong><p>The source is part of this conversation.</p><span class="source-card-port" aria-hidden="true"></span></aside><div class="source-connection" aria-hidden="true"><svg class="scene-wire" viewBox="0 0 94 110" preserveAspectRatio="none"><path d="M0 25C48 25 46 83 94 83" stroke="currentColor" stroke-width="2" fill="none"/><circle cx="90" cy="83" r="4" fill="currentColor"/></svg></div>`;
  }

  function placedResults(state) {
    const entries = Array.isArray(state.placedResults) ? state.placedResults : [];
    if (!entries.length) return '';
    const visible = entries.slice(-24);
    return `<section class="placed-results" aria-label="Copies on sample board"><div class="placed-results-heading"><span>Copies on sample board</span><span>${entries.length} ${entries.length === 1 ? 'copy' : 'copies'}${entries.length > visible.length ? ` · latest ${visible.length} shown` : ''}</span></div><div class="placed-results-gallery">${visible.map((entry, index) => `<figure class="placed-result-card"><div class="placed-result-image">${imageSvg(entry.index, true, `placed-${index}`)}</div><figcaption>${escape(entry.title || `Image ${Math.floor(number(entry.index)) + 1}`)}</figcaption></figure>`).join('')}</div></section>`;
  }

  function render(input = {}) {
    const state = { ...input };
    const modality = Object.prototype.hasOwnProperty.call(labels, state.modality) ? state.modality : 'chat';
    state.modality = modality;
    const [label, description] = labels[modality];
    const free = modality === 'free-agent';
    const generator = ['image', 'text', 'dashboard'].includes(modality);
    const title = state.conversationTitle || (free ? 'Mira · board collaborator' : label);
    let content;
    if (modality === 'image') content = imageContent(state);
    else if (modality === 'dashboard') content = dashboardContent(state);
    else if (modality === 'text') content = textContent(state);
    else content = `${free ? `<div class="free-agent-context"><span class="agent-avatar" aria-hidden="true">◉</span><div><strong>Mira</strong><span>Board collaborator</span></div>${button('locate-agent', 'Find on board', 'scene-text-button')}</div>` : ''}${conversation(state, free)}`;
    return `<section class="studio-scene modality-${modality}"><div class="scene-caption"><span class="scene-type">${escape(label)}</span><p>${escape(description)}</p>${!generator && !free ? button('open-presentation', 'Conversation layout <span aria-hidden="true">⌄</span>', 'scene-presentation-button') : ''}</div><div class="scene-stage${modality === 'source-chat' ? ' has-source' : ''}">${modality === 'source-chat' ? sourceCard(state) : ''}<div class="scene-workspace"><section class="scene-card ${generator ? 'generator-card' : 'conversation-card'}${modality === 'text' ? ' text-card' : ''}${free ? ' free-agent-card' : ''}">${sceneHeader(state, title)}<div class="scene-card-body">${content}</div>${activity(state)}<footer class="scene-card-footer"><div class="scene-composer-slot" data-composer-slot></div></footer></section></div></div>${placedResults(state)}</section>`;
  }

  window.AgentStudioScene = { render };
}());
