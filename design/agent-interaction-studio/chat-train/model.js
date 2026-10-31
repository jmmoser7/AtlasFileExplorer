(function () {
  'use strict';

  // Canonical turns form a tree; cards are views. No provider runs in this model.
  // state.running is null or {assistantId, userId, cardId, streamOpen}. append receives a
  // text DELTA. Cards use fold=open|partial|collapsed and customSize={width,height}.
  // A measure(card) callback returns final folded dimensions. Projected cards
  // add draftId, turnIds, sourceTurnId and sessionId to the public view fields.
  const MAIN = [
    ['user', 'Plan the courtyard?'], ['assistant', 'Start with the trees.'],
    ['user', 'Which trees?'], ['assistant', 'Two plane trees.'],
    ['user', 'Where?'], ['assistant', 'By the north wall.']
  ];
  const copy = value => JSON.parse(JSON.stringify(value));
  const finite = value => typeof value === 'number' && Number.isFinite(value);
  const keyFor = (presentation, turnId) => `${presentation}:${turnId}`;
  const nextId = (state, prefix) => `${prefix}-${state.nextId++}`;
  const meta = (state, id) => state.meta[id] || (state.meta[id] = {});
  const childTurns = (state, id) => state.turns.filter(turn => turn.parentId === id);
  const childDrafts = (state, id) => state.drafts.filter(draft => draft.parentId === id);
  const DEFAULT_AGENT = { provider: 'cursor', model: 'Auto', effort: 'medium', speed: 'standard' };
  const PROVIDER_TITLES = { cursor: 'Cursor', codex: 'Codex', 'openai-text': 'OpenAI API', openai: 'OpenAI API', api: 'OpenAI API', ollama: 'Local model', local: 'Local model' };

  function agentConfig(value = {}, fallback = DEFAULT_AGENT) {
    const result = {};
    for (const key of Object.keys(DEFAULT_AGENT)) {
      result[key] = typeof value[key] === 'string' && value[key].trim() ? value[key] : fallback[key] || DEFAULT_AGENT[key];
    }
    return result;
  }

  function ensureState(state) {
    Object.assign(state, agentConfig(state));
    state.nextAgents ||= {};
    state._measured ||= {};
    state._cards ||= [];
    for (const turn of state.turns) Object.assign(turn, agentConfig(turn, state));
    for (const draft of state.drafts) Object.assign(draft, agentConfig(draft, state));
    for (const [id, settings] of Object.entries(state.meta)) {
      if ('windowHeight' in settings) {
        delete settings.windowHeight;
        delete state._measured[id];
      }
    }
    if (state.configVersion !== 1) {
      state.configVersion = 1;
      state._cards = [];
    }
  }

  function create(options = {}) {
    const state = {
      version: 1, presentation: 'pairs', title: options.title || 'Cursor',
      ...agentConfig(options.agent || options), turns: [], drafts: [], meta: {}, nextAgents: {}, configVersion: 1,
      positions: {}, nextId: 1, layoutRevision: 0, activeCardId: null,
      running: null, status: 'idle', usage: { reported: null, estimated: 0 },
      _undo: [], _redo: [], _cards: [], _measured: {}
    };
    const transcript = Array.isArray(options.transcript) ? options.transcript : MAIN.map(([role, text]) => ({ role, text }));
    let parentId = null;
    for (const entry of transcript) {
      if (!entry || !['user', 'assistant'].includes(entry.role)) continue;
      const turn = {
        id: nextId(state, 'turn'), parentId, role: entry.role, text: String(entry.text || ''),
        title: state.title, ...agentConfig(entry, state), sessionId: 'session-main'
      };
      state.turns.push(turn);
      parentId = turn.id;
    }
    if (!state.turns.length) {
      state.drafts.push({ id: nextId(state, 'draft'), parentId: null, text: '', title: state.title, ...agentConfig(state), sessionId: 'session-main', fork: false });
    }
    const cards = cardsFor(state);
    state.activeCardId = cards.length ? cards[cards.length - 1].id : null;
    return state;
  }

  function segments(state) {
    const byParent = new Map();
    for (const turn of state.turns) {
      if (!byParent.has(turn.parentId)) byParent.set(turn.parentId, []);
      byParent.get(turn.parentId).push(turn);
    }
    const result = [];
    const visit = start => {
      const path = [];
      let at = start;
      while (at) {
        path.push(at);
        const children = byParent.get(at.id) || [];
        if (children.length === 1 && !childDrafts(state, at.id).length && children[0].sessionId === at.sessionId) at = children[0];
        else break;
      }
      result.push(path);
      for (const child of byParent.get(at.id) || []) visit(child);
    };
    const ids = new Set(state.turns.map(turn => turn.id));
    for (const root of state.turns.filter(turn => turn.parentId === null || !ids.has(turn.parentId))) visit(root);
    return result;
  }

  function cardsFor(state) {
    ensureState(state);
    const cards = [];
    const turnOwner = new Map();
    for (const path of segments(state)) {
      const groups = [];
      for (const turn of path) {
        if (!groups.length || state.presentation === 'messages' || (state.presentation === 'pairs' && turn.role === 'user')) groups.push([]);
        groups[groups.length - 1].push(turn);
      }
      let previous = null;
      for (const group of groups) {
        const first = group[0], last = group[group.length - 1];
        const id = keyFor(state.presentation, first.id);
        const settings = state.meta[id] || {};
        const card = {
          id, parentId: previous, parentTurnId: previous ? null : first.parentId,
          turns: group.map(turn => ({ id: turn.id, role: turn.role, text: turn.text, ...agentConfig(turn, state) })),
          turnIds: group.map(turn => turn.id), sourceTurnId: last.id,
          title: last.title || state.title, ...agentConfig(last, state), sessionId: last.sessionId,
          isDraft: false, isTail: false, running: Boolean(state.running && group.some(turn => turn.id === state.running.assistantId)),
          streamOpen: Boolean(state.running?.streamOpen && group.some(turn => turn.id === state.running.assistantId)),
          detail: state.presentation === 'window' ? 'full' : settings.detail || (state.presentation === 'pairs' ? 'pair' : 'summary'),
          fold: settings.fold || 'open', fullExpanded:Boolean(settings.fullExpanded), draftText: '', draftId: null,
          customSize: settings.customSize ? { ...settings.customSize } : undefined
        };
        cards.push(card);
        for (const turn of group) turnOwner.set(turn.id, id);
        previous = id;
      }
    }
    for (const card of cards) if (card.parentTurnId) card.parentId = turnOwner.get(card.parentTurnId) || null;
    for (const draft of state.drafts) {
      const parentId = turnOwner.get(draft.parentId) || null;
      const host = cards.find(card => card.id === parentId);
      const absorb = state.presentation === 'window' && host && !draft.fork && !childTurns(state, draft.parentId).length && childDrafts(state, draft.parentId).length === 1;
      if (absorb) {
        host.draftId = draft.id;
        host.draftText = draft.text;
        Object.assign(host, agentConfig(draft, state));
      } else {
        const settings = state.meta[draft.id] || {};
        cards.push({
          id: draft.id, parentId, parentTurnId: draft.parentId, turns: [], turnIds: [],
          sourceTurnId: draft.parentId, title: draft.title || state.title,
          ...agentConfig(draft, state),
          sessionId: draft.sessionId, isDraft: true, isTail: true, running: false, streamOpen: false,
          detail: state.presentation === 'window' ? 'full' : settings.detail || (state.presentation === 'pairs' ? 'pair' : 'summary'),
          fold: 'open', fullExpanded:Boolean(settings.fullExpanded), draftText: draft.text, draftId: draft.id,
          customSize: settings.customSize ? { ...settings.customSize } : undefined
        });
      }
    }
    for (const card of cards) {
      card.isTail = !cards.some(other => other.parentId === card.id);
      if (card.isTail && !card.draftId) card.draftText = (state.meta[card.id] || {}).draftText || '';
      const legacyModel = (state.meta[card.id] || {}).pendingModel;
      if (legacyModel && card.isTail && !card.draftId && !state.nextAgents[card.sourceTurnId]) {
        state.nextAgents[card.sourceTurnId] = agentConfig({ model: legacyModel }, card);
      }
      const pending = state.nextAgents[card.sourceTurnId];
      if (pending && card.isTail && !card.draftId) {
        Object.assign(card, agentConfig(pending, card));
        if (pending.title) card.title = pending.title;
      }
      if(card.isTail&&window.ChatAgentCatalog)Object.assign(card,window.ChatAgentCatalog.normalize(card));
    }
    return cards;
  }

  function project(state, measure) {
    const cards = cardsFor(state);
    for (const card of cards) {
      const measured = typeof measure === 'function' ? measure({ ...card }) : state._measured[card.id];
      if (measured && finite(measured.width) && finite(measured.height)) state._measured[card.id] = { width: measured.width, height: measured.height };
      let width = measured && finite(measured.width) ? measured.width : (state.presentation === 'window' ? 440 : card.isDraft ? 190 : 320);
      let height = measured && finite(measured.height) ? measured.height : (card.isDraft ? 62 : card.fold === 'collapsed' ? 98 : card.fold === 'partial' ? 196 : state.presentation === 'window' ? 320 : 230);
      if (card.customSize) {
        width = card.customSize.width;
        if (card.fold === 'open' && !card.customSize.autoHeight) height = card.customSize.height;
      }
      card.width = Math.max(176, width);
      card.height = Math.max(38, height);
    }
    const byId = new Map(cards.map(card => [card.id, card]));
    const visiting = new Set();
    function subtreeHeight(card) {
      const children = cards.filter(other => other.parentId === card.id);
      return Math.max(card.height, children.reduce((height, child) => height + subtreeHeight(child), 0) + Math.max(0, children.length - 1) * 72);
    }
    function place(card) {
      if (finite(card.x) || visiting.has(card.id)) return;
      visiting.add(card.id);
      const parent = byId.get(card.parentId);
      if (parent) place(parent);
      let position = state.positions[card.id];
      if (!position) {
        const siblings = cards.filter(other => other.parentId === card.parentId);
        const lane = siblings.indexOf(card);
        const preceding = siblings.slice(0, lane);
        let y = parent ? parent.y : 110;
        for (const sibling of preceding) {
          place(sibling);
          y = Math.max(y, sibling.y + subtreeHeight(sibling) + 72);
        }
        position = { x: parent ? parent.x + parent.width + 96 : 40, y };
        state.positions[card.id] = position;
      }
      card.x = position.x;
      card.y = position.y;
      visiting.delete(card.id);
    }
    for (const card of cards) place(card);
    state._cards = cards;
    if (state.running) {
      const owner = cards.find(card => card.turnIds.includes(state.running.assistantId));
      if (owner) state.running.cardId = owner.id;
    }
    return cards;
  }

  function getCard(state, id) {
    ensureState(state);
    return (state._cards || []).find(card => card.id === id || card.draftId === id) || project(state).find(card => card.id === id || card.draftId === id) || null;
  }

  function snapshot(state) {
    const result = {};
    for (const key of ['presentation', 'title', 'provider', 'model', 'effort', 'speed', 'nextAgents', 'turns', 'drafts', 'meta', 'positions', 'nextId', 'layoutRevision', 'activeCardId']) result[key] = copy(state[key]);
    return result;
  }

  function action(state, change) {
    const before = snapshot(state);
    const result = change();
    if (result === false || result === null) return result;
    state._undo.push(before);
    if (state._undo.length > 80) state._undo.shift();
    state._redo = [];
    state._cards = [];
    return result;
  }

  function continueFrom(state, cardId, options = {}) {
    const card = getCard(state, cardId);
    if (!card || card.isDraft || card.running || state.running || (!options.fork && !card.isTail)) return null;
    if (!options.fork && card.draftId) {
      state.activeCardId = card.id;
      return card.draftId;
    }
    return action(state, () => {
      const id = nextId(state, 'draft');
      const fork = Boolean(options.fork);
      const siblings = [...childTurns(state, card.sourceTurnId), ...childDrafts(state, card.sourceTurnId)].length;
      const draft = {
        id, parentId: card.sourceTurnId, text: '', title: card.title, ...agentConfig(card),
        sessionId: fork ? nextId(state, 'session') : card.sessionId,
        fork
      };
      state.drafts.push(draft);
      if (!fork) delete state.nextAgents[card.sourceTurnId];
      state.positions[id] = {
        x: finite(options.x) ? options.x : card.x + card.width + 96,
        y: finite(options.y) ? options.y : card.y + siblings * (62 + 72)
      };
      state.activeCardId = state.presentation === 'window' && !fork ? card.id : id;
      return id;
    });
  }

  function send(state, cardId, text) {
    const card = getCard(state, cardId);
    const prompt = String(text ?? card?.draftText ?? '').trim();
    if (!card || !prompt || state.running || (!card.isTail && !card.isDraft)) return null;
    return action(state, () => {
      const draft = state.drafts.find(item => item.id === card.draftId);
      const userId = nextId(state, 'turn'), assistantId = nextId(state, 'turn');
      const parentId = draft ? draft.parentId : card.sourceTurnId;
      const inherited = { title: card.title, ...(window.ChatAgentCatalog?window.ChatAgentCatalog.normalize(card):agentConfig(card)), sessionId: draft?.sessionId || card.sessionId || 'session-main' };
      state.turns.push({ id: userId, parentId, role: 'user', text: prompt, ...inherited });
      state.turns.push({ id: assistantId, parentId: userId, role: 'assistant', text: '', ...inherited });
      const newId = keyFor(state.presentation, state.presentation === 'messages' ? assistantId : userId);
      if (card.isDraft && state.positions[card.id]) state.positions[keyFor(state.presentation, userId)] = { ...state.positions[card.id] };
      if (draft) state.drafts = state.drafts.filter(item => item.id !== draft.id);
      delete state.nextAgents[parentId];
      if (state.meta[cardId]) { delete state.meta[cardId].draftText; delete state.meta[cardId].pendingModel; }
      const fresh = cardsFor(state);
      const owner = fresh.find(item => item.turnIds.includes(assistantId));
      const resultId = owner ? owner.id : newId;
      state.running = { assistantId, userId, cardId: resultId, streamOpen: state.presentation === 'window' && owner?.fold === 'collapsed' };
      state.status = 'running';
      state.usage = { reported: null, estimated: 0 };
      state.activeCardId = resultId;
      return { cardId: resultId, assistantId };
    });
  }

  function append(state, assistantId, text, done = false) {
    if (!state.running || state.running.assistantId !== assistantId) return false;
    const turn = state.turns.find(item => item.id === assistantId && item.role === 'assistant');
    if (!turn) return false;
    turn.text += String(text || '');
    state.usage = { reported: null, estimated: Math.floor(turn.text.length / 4) };
    state._cards = [];
    if (done) { state.running = null; state.status = 'completed'; }
    return true;
  }

  function stop(state) {
    if (!state.running) return false;
    state.running = null;
    state.status = 'stopped';
    state._cards = [];
    return true;
  }

  function changePresentation(state, presentation) {
    if (!['pairs', 'messages', 'window'].includes(presentation) || state.running || state.presentation === presentation) return false;
    const current = project(state);
    const active = current.find(card => card.id === state.activeCardId || card.draftId === state.activeCardId) || current[current.length - 1];
    // An unsent tail composer becomes a logical draft, so changing layouts never loses it.
    return action(state, () => {
      for (const card of current) {
        if (card.isTail && !card.isDraft && !card.draftId && card.draftText) {
          state.drafts.push({ id: nextId(state, 'draft'), parentId: card.sourceTurnId, text: card.draftText, title: card.title, ...agentConfig(card), sessionId: card.sessionId, fork: false });
          delete meta(state, card.id).draftText;
        }
      }
      state.presentation = presentation;
      state.layoutRevision++;
      state.positions = {};
      // Native rechunking resets each view's detail to its new presentation.
      for (const settings of Object.values(state.meta)) delete settings.detail;
      const after = cardsFor(state);
      const selected = active && after.find(card => (active.draftId && card.draftId === active.draftId) || card.turnIds.includes(active.sourceTurnId));
      state.activeCardId = selected ? selected.id : after[after.length - 1]?.id || null;
      return true;
    });
  }

  function rename(state, cardId, title) {
    const card = getCard(state, cardId);
    const value = String(title || '').trim();
    if (!card || !value || value === card.title) return false;
    return action(state, () => {
      if (card.isDraft) {
        const draft = state.drafts.find(item => item.id === card.draftId);
        if (draft) draft.title = value;
        return true;
      }
      const segment = segments(state).find(path => path.some(turn => card.turnIds.includes(turn.id) || turn.id === card.sourceTurnId));
      const ids = new Set((segment || []).map(turn => turn.id));
      for (const turn of state.turns) if (ids.has(turn.id)) turn.title = value;
      for (const draft of state.drafts) if (draft.id === card.draftId || ids.has(draft.parentId)) draft.title = value;
      for (const [id, config] of Object.entries(state.nextAgents)) if (ids.has(id)) delete config.title;
      return true;
    });
  }

  function setModel(state, cardId, model) {
    return setAgent(state, cardId, { model: String(model || 'Auto') });
  }

  function setAgent(state, cardId, config) {
    const card = getCard(state, cardId);
    if (!card || !card.isTail || state.running || !config || typeof config !== 'object') return false;
    const raw = agentConfig(config, card);
    const value = window.ChatAgentCatalog ? window.ChatAgentCatalog.normalize(raw) : raw;
    if (Object.keys(DEFAULT_AGENT).every(key => value[key] === card[key])) return false;
    if (value.provider !== card.provider && card.title === PROVIDER_TITLES[card.provider] && PROVIDER_TITLES[value.provider]) value.title = PROVIDER_TITLES[value.provider];
    else if (state.nextAgents[card.sourceTurnId]?.title) value.title = card.title;
    return action(state, () => {
      const draft = state.drafts.find(item => item.id === card.draftId);
      if (draft) Object.assign(draft, value);
      else state.nextAgents[card.sourceTurnId] = value;
      delete meta(state, cardId).pendingModel;
      return true;
    });
  }

  function setDraft(state, cardId, text) {
    const card = getCard(state, cardId);
    if (!card || (!card.isTail && !card.isDraft)) return false;
    const draft = state.drafts.find(item => item.id === card.draftId);
    if (draft) draft.text = String(text || '');
    else meta(state, cardId).draftText = String(text || '');
    state.activeCardId = card.id;
    state._cards = [];
    return true;
  }

  function setFold(state, cardId, fold) {
    const card = getCard(state, cardId);
    if (fold === 'full') fold = 'open';
    if (!card || card.isDraft || !['open', 'partial', 'collapsed'].includes(fold) || (card.fold === fold && !card.streamOpen)) return false;
    return action(state, () => {
      if (card.running && state.running) state.running.streamOpen = false;
      meta(state, cardId).fold = fold;
      if(fold==='open'){
        meta(state,cardId).fullExpanded=true;
        meta(state,cardId).customSize={width:card.width,autoHeight:true};
      }else meta(state,cardId).fullExpanded=false;
      return true;
    });
  }

  function setDetail(state, cardId, detail) {
    const card = getCard(state, cardId);
    if (!card || card.isDraft || detail !== 'full' || card.detail === detail) return false;
    return action(state, () => {
      // Native Full changes painter/detail, not checkpoint start/end. The
      // immediate 540-unit rect is subsequently refit to the same own content.
      meta(state, cardId).detail = detail;
      delete state._measured[cardId];
      return true;
    });
  }

  function move(state, cardId, x, y) {
    if (!getCard(state, cardId) || !finite(x) || !finite(y)) return false;
    const previous = state.positions[cardId];
    if (previous && previous.x === x && previous.y === y) return false;
    return action(state, () => { state.positions[cardId] = { x, y }; return true; });
  }

  function resize(state, cardId, width, height, position) {
    const card = getCard(state, cardId);
    if (!card || card.isDraft || !finite(width) || (height !== null && !finite(height)) || (position && (!finite(position.x) || !finite(position.y)))) return false;
    return action(state, () => {
      meta(state, cardId).customSize = height === null ? {width:Math.max(176,width),autoHeight:true} : { width: Math.max(176, width), height: Math.max(62, height) };
      meta(state, cardId).fold = 'open';
      meta(state, cardId).fullExpanded=height===null;
      if (position) state.positions[cardId] = { x: position.x, y: position.y };
      return true;
    });
  }

  function expandFull(state,cardId){
    const card=getCard(state,cardId);if(!card)return false;
    return action(state,()=>{meta(state,cardId).fold='open';meta(state,cardId).fullExpanded=true;meta(state,cardId).customSize={width:card.width,autoHeight:true};return true;});
  }

  function fit(state, cardId) {
    const card = getCard(state, cardId);
    if (!card || !card.customSize) return false;
    return action(state, () => { delete meta(state, cardId).customSize; return true; });
  }

  function deleteLeaf(state, cardId) {
    const card = getCard(state, cardId);
    if (!card || !card.isTail) return false;
    return action(state, () => {
      if (card.isDraft) state.drafts = state.drafts.filter(draft => draft.id !== card.id);
      else {
        const ids = new Set(card.turnIds);
        state.turns = state.turns.filter(turn => !ids.has(turn.id));
        for (const id of ids) delete state.nextAgents[id];
        state.drafts = state.drafts.filter(draft => draft.id !== card.draftId && !ids.has(draft.parentId));
        if (state.running && ids.has(state.running.assistantId)) stop(state);
      }
      delete state.positions[cardId];
      delete state.meta[cardId];
      state.activeCardId = card.parentId;
      return true;
    });
  }

  function restore(state, target) {
    const latest = new Map(state.turns.filter(turn => turn.role === 'assistant').map(turn => [turn.id, turn.text]));
    const draftTextById = new Map(state.drafts.map(draft => [draft.id, draft.text]));
    const pending = new Map(state.drafts.filter(draft => !draft.fork).map(draft => [draft.parentId, draft.text]));
    for (const card of cardsFor(state)) {
      if (Object.prototype.hasOwnProperty.call(state.meta[card.id] || {}, 'draftText')) pending.set(card.sourceTurnId, state.meta[card.id].draftText);
    }
    const highWater = state.nextId;
    Object.assign(state, copy(target));
    state.nextAgents = copy(target.nextAgents || {});
    ensureState(state);
    // Undoing a gesture never rewinds independently streamed answer bytes.
    for (const turn of state.turns) if (latest.has(turn.id)) turn.text = latest.get(turn.id);
    for (const draft of state.drafts) {
      if (draftTextById.has(draft.id)) draft.text = draftTextById.get(draft.id);
      else if (!draft.fork && pending.has(draft.parentId)) draft.text = pending.get(draft.parentId);
    }
    for (const card of cardsFor(state)) {
      if (Object.prototype.hasOwnProperty.call(state.meta[card.id] || {}, 'draftText') && pending.has(card.sourceTurnId)) state.meta[card.id].draftText = pending.get(card.sourceTurnId);
    }
    state.nextId = Math.max(state.nextId, highWater);
    if (state.running && !state.turns.some(turn => turn.id === state.running.assistantId)) stop(state);
    state._cards = [];
  }

  function undo(state) {
    const target = state._undo.pop();
    if (!target) return false;
    state._redo.push(snapshot(state));
    restore(state, target);
    return true;
  }

  function redo(state) {
    const target = state._redo.pop();
    if (!target) return false;
    state._undo.push(snapshot(state));
    restore(state, target);
    return true;
  }

  window.ChatTrainModel = { create, project, continueFrom, send, append, stop, changePresentation, rename, setAgent, setModel, setDraft, setFold, setDetail, move, resize, expandFull, fit, deleteLeaf, undo, redo };
}());
