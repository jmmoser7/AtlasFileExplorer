'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const context = vm.createContext({ window: {} });
vm.runInContext(fs.readFileSync(require('node:path').join(__dirname, 'model.js'), 'utf8'), context);
const M = context.window.ChatTrainModel;
const measure = card => ({ width: 240, height: card.isDraft ? 62 : card.fold === 'collapsed' ? 90 : card.fold === 'partial' ? 180 : 220 });
const cards = state => M.project(state, measure);
const tail = state => cards(state).filter(card => card.isTail).at(-1);

// Default projection and world-space rails are stable across repeated paint.
{
  const state = M.create();
  const first = cards(state);
  assert.equal(first.length, 3);
  assert.equal(first[0].turns[0].text, 'Plan the courtyard?');
  assert.equal(first[0].x, 40);
  assert.equal(first[0].y, 110);
  assert.equal(first[1].x, first[0].x + first[0].width + 96);
  assert.equal(first[1].y, first[0].y);
  assert.equal(first[0].isTail, false);
  assert.equal(first[2].isTail, true);
  assert.equal(cards(state)[2].id, first[2].id);
  assert.equal(M.continueFrom(state, first[0].id, { fork: false }), null);
  assert.equal(M.setModel(state, first[0].id, 'Other'), false);
}

// A continuation is an unsent draft; sending converts it at its authored position.
{
  const state = M.create();
  const oldTail = tail(state);
  const id = M.continueFrom(state, oldTail.id, { x: 1500, y: 400 });
  assert.equal(state.running, null);
  const draft = cards(state).find(card => card.id === id);
  assert.equal(draft.isDraft, true);
  assert.equal(draft.x, 1500);
  M.setDraft(state, id, 'Add a bench.');
  const sent = M.send(state, id, draft.draftText || 'Add a bench.');
  assert.ok(sent.assistantId);
  const reply = cards(state).find(card => card.id === sent.cardId);
  assert.equal(reply.x, 1500);
  assert.equal(reply.y, 400);
  assert.equal(reply.turns.length, 2);
  assert.equal(M.send(state, reply.id, 'Too soon'), null);
  M.append(state, sent.assistantId, 'Place it ', false);
  M.append(state, sent.assistantId, 'beneath a tree.', true);
  assert.equal(state.running, null);
  assert.equal(tail(state).turns.at(-1).text, 'Place it beneath a tree.');
  assert.equal(M.append(state, sent.assistantId, 'stale', false), false);
}

// Forks preserve checkpoint ancestry and receive a separate provider session.
{
  const state = M.create();
  const first = cards(state)[0];
  const draftId = M.continueFrom(state, first.id, { fork: true });
  const draft = cards(state).find(card => card.id === draftId);
  assert.notEqual(draft.sessionId, first.sessionId);
  assert.ok(draft.y > first.y);
  const sent = M.send(state, draftId, 'Pave it instead?');
  M.append(state, sent.assistantId, 'Granite setts.', true);
  assert.equal(state.turns.find(turn => turn.id === state.turns.at(-2).id).parentId, first.sourceTurnId);
  assert.equal(cards(state).length, 4);
  M.changePresentation(state, 'messages');
  assert.equal(cards(state).length, 8);
  M.changePresentation(state, 'window');
  assert.equal(cards(state).length, 3);
  assert.equal(cards(state).filter(card => card.parentId === cards(state)[0].id).length, 2);
}

// Draft text survives rechunking and is absorbed into the single-window composer.
{
  const state = M.create();
  const draftId = M.continueFrom(state, tail(state).id, {});
  M.setDraft(state, draftId, 'Keep this unsent.');
  M.changePresentation(state, 'window');
  assert.equal(cards(state).length, 1);
  assert.equal(cards(state)[0].draftText, 'Keep this unsent.');
  assert.equal(cards(state)[0].draftId, draftId);
  assert.equal(state.running, null);
  M.setDraft(state, cards(state)[0].id, 'Keep the newest unsent text.');
  assert.equal(M.undo(state), true);
  assert.equal(state.presentation, 'pairs');
  assert.equal(cards(state).find(card => card.id === draftId).draftText, 'Keep the newest unsent text.');
  assert.equal(M.redo(state), true);
  assert.equal(cards(state).length, 1);
  assert.equal(cards(state)[0].draftText, 'Keep the newest unsent text.');
}

// Renaming a fork draft does not rename the upstream conversation branch.
{
  const state = M.create();
  const first = cards(state)[0];
  const id = M.continueFrom(state, first.id, { fork: true });
  M.rename(state, id, 'Paving alternative');
  assert.equal(cards(state)[0].title, 'Cursor');
  assert.equal(cards(state).find(card => card.id === id).title, 'Paving alternative');
}

// Model choice follows the next request without rewriting historical answers.
{
  const state = M.create();
  const old = tail(state);
  M.setModel(state, old.id, 'Different model');
  assert.equal(tail(state).model, 'Different model');
  const sent = M.send(state, old.id, 'Next question');
  M.append(state, sent.assistantId, 'Next reply', true);
  assert.equal(cards(state).find(card => card.id === old.id).model, 'Auto');
  assert.equal(tail(state).model, 'Different model');
}

// Gestures undo as actions; streamed bytes do not rewind with a movement undo.
{
  const state = M.create();
  const old = tail(state);
  const sent = M.send(state, old.id, 'Another turn');
  M.append(state, sent.assistantId, 'First ', false);
  const reply = cards(state).find(card => card.id === sent.cardId);
  M.move(state, reply.id, 1800, 600);
  M.append(state, sent.assistantId, 'second.', false);
  M.undo(state);
  assert.equal(cards(state).find(card => card.id === reply.id).turns.at(-1).text, 'First second.');
  assert.equal(state.running.assistantId, sent.assistantId);
  M.stop(state);
  assert.equal(state.status, 'stopped');
  assert.equal(M.append(state, sent.assistantId, 'late', true), false);
}

// Authored sizes, folds, fitting and deletion preserve adjacent cards.
{
  const state = M.create();
  const original = cards(state);
  M.move(state, original[0].id, 80, 90);
  assert.equal(cards(state)[1].x, original[1].x);
  M.resize(state, original[2].id, 400, 330);
  assert.equal(tail(state).height, 330);
  M.setFold(state, original[2].id, 'collapsed');
  assert.equal(tail(state).width, 400);
  assert.equal(tail(state).height, 90);
  M.setFold(state, original[2].id, 'open');
  assert.equal(tail(state).customSize.width, 400);
  M.resize(state, original[2].id, 400, 330);
  M.fit(state, original[2].id);
  assert.equal(tail(state).width, 240);
  M.deleteLeaf(state, original[2].id);
  assert.equal(cards(state).length, 2);
  assert.equal(tail(state).id, original[1].id);
  M.undo(state);
  assert.equal(cards(state).length, 3);
}

// Optional exact reference fixture and serialization have no environment dependency.
{
  const state = M.create({ transcript: [{ role: 'user', text: 'Question' }, { role: 'assistant', text: 'Answer' }] });
  assert.equal(cards(state).length, 1);
  assert.equal(cards(state)[0].title, 'Cursor');
  assert.equal(cards(state)[0].model, 'Auto');
  assert.equal(cards(JSON.parse(JSON.stringify(state)))[0].turns[1].text, 'Answer');
}

// Temporary streaming expansion is separate from the authored collapsed fold.
{
  const state = M.create();
  M.changePresentation(state, 'window');
  const windowCard = cards(state)[0];
  M.setFold(state, windowCard.id, 'collapsed');
  const sent = M.send(state, windowCard.id, 'One more question');
  assert.equal(state.running.streamOpen, true);
  assert.equal(cards(state)[0].streamOpen, true);
  assert.equal(cards(state)[0].fold, 'collapsed');
  M.append(state, sent.assistantId, 'The answer.', true);
  assert.equal(state.running, null);
  assert.equal(cards(state)[0].streamOpen, false);
  assert.equal(cards(state)[0].fold, 'collapsed');
}

// An explicit fold while streaming cancels the temporary open, including when
// the chosen canonical fold is already collapsed. Open clears authored size.
{
  const state = M.create();
  M.changePresentation(state, 'window');
  const id = cards(state)[0].id;
  M.resize(state, id, 400, 330);
  M.setFold(state, id, 'collapsed');
  const sent = M.send(state, id, 'Continue');
  assert.equal(cards(state)[0].streamOpen, true);
  assert.equal(M.setFold(state, id, 'collapsed'), true);
  assert.equal(state.running.streamOpen, false);
  assert.equal(cards(state)[0].streamOpen, false);
  M.append(state, sent.assistantId, 'Partial text', false);
  M.setFold(state, id, 'open');
  assert.equal(cards(state)[0].customSize.width, 400);
  M.append(state, sent.assistantId, '', true);
  assert.equal(cards(state)[0].fold, 'open');
}

// Native Full conversation changes the view painter/detail without widening
// its checkpoint range or duplicating canonical turns. It is one undo step.
{
  const state = M.create();
  const original = cards(state)[1];
  assert.equal(original.detail, 'pair');
  const canonical = JSON.stringify(state.turns);
  const ownIds = JSON.stringify(original.turnIds);
  const ownText = JSON.stringify(original.turns);
  M.setDetail(state, original.id, 'full');
  const full = cards(state).find(card => card.id === original.id);
  assert.equal(full.detail, 'full');
  assert.equal(JSON.stringify(full.turnIds), ownIds);
  assert.equal(JSON.stringify(full.turns), ownText);
  assert.equal(JSON.stringify(state.turns), canonical);
  M.undo(state);
  assert.equal(cards(state).find(card => card.id === original.id).detail, 'pair');
  M.redo(state);
  assert.equal(cards(state).find(card => card.id === original.id).detail, 'full');
  M.changePresentation(state, 'messages');
  assert.ok(cards(state).every(card => card.detail === 'summary'));
  M.changePresentation(state, 'pairs');
  assert.ok(cards(state).every(card => card.detail === 'pair'));
  M.changePresentation(state, 'window');
  assert.ok(cards(state).every(card => card.detail === 'full'));
}

// North/west resizing moves the top-left and authors size in one undo step.
{
  const state = M.create();
  const original = tail(state);
  const depth = state._undo.length;
  M.resize(state, original.id, 350, 320, { x: original.x - 110, y: original.y - 100 });
  const resized = tail(state);
  assert.equal(state._undo.length, depth + 1);
  assert.equal(resized.x, original.x - 110);
  assert.equal(resized.y, original.y - 100);
  assert.equal(resized.width, 350);
  assert.equal(resized.height, 320);
  M.undo(state);
  const restored = tail(state);
  assert.equal(restored.x, original.x);
  assert.equal(restored.y, original.y);
  assert.equal(restored.width, original.width);
  assert.equal(restored.height, original.height);
  assert.equal(restored.customSize, undefined);
  M.redo(state);
  assert.equal(tail(state).x, resized.x);
  assert.equal(tail(state).height, 320);
}

// Single-window projection uses the requested readable measurement, never a
// stale ceiling inherited from a tiny pair (including old serialized saves).
{
  const state = M.create();
  cards(state);
  M.changePresentation(state, 'window');
  const id = M.project(state)[0].id;
  state.meta[id] = { windowHeight: 62 };
  state._measured[id] = { width: 190, height: 62 };
  delete state.configVersion;
  const restored = JSON.parse(JSON.stringify(state));
  const window = M.project(restored, () => ({ width: 440, height: 420 }))[0];
  assert.equal(window.width, 440);
  assert.equal(window.height, 420);
  assert.equal('windowHeight' in restored.meta[id], false);
  assert.equal(window.turns.length, 6);
  M.undo(restored);
  assert.equal(restored.presentation, 'pairs');
  assert.equal(cards(restored).length, 3);
  M.redo(restored);
  assert.equal(M.project(restored, () => ({ width: 440, height: 500 }))[0].height, 500);
}

// All four next-turn controls survive layout changes without changing history,
// then get captured on both the request and response turns when sent.
{
  const state = M.create();
  const selected = tail(state);
  const canonical = JSON.stringify(state.turns);
  assert.equal(M.setAgent(state, cards(state)[0].id, { model: 'Unavailable here' }), false);
  assert.equal(M.setAgent(state, selected.id, { provider: 'codex', model: 'Reasoning model', effort: 'high', speed: 'fast' }), true);
  assert.equal(JSON.stringify(state.turns), canonical);
  M.changePresentation(state, 'window');
  let window = M.project(state, () => ({ width: 440, height: 320 }))[0];
  assert.equal(window.provider, 'codex');
  assert.equal(window.model, 'Reasoning model');
  assert.equal(window.effort, 'high');
  assert.equal(window.speed, 'fast');
  M.setDraft(state, window.id, 'Preserve my configuration.');
  const restored = JSON.parse(JSON.stringify(state));
  M.changePresentation(restored, 'messages');
  M.changePresentation(restored, 'window');
  window = M.project(restored)[0];
  assert.equal(window.draftText, 'Preserve my configuration.');
  assert.equal(window.effort, 'high');
  const result = M.send(restored, window.id, window.draftText);
  assert.equal(restored.turns.length, 8);
  for (const turn of restored.turns.slice(-2)) {
    assert.equal(turn.provider, 'codex');
    assert.equal(turn.model, 'Reasoning model');
    assert.equal(turn.effort, 'high');
    assert.equal(turn.speed, 'fast');
  }
  assert.equal(M.setAgent(restored, result.cardId, { effort: 'low' }), false);
  M.append(restored, result.assistantId, 'Configured answer.', true);
  assert.equal(M.project(restored).length, 1);
  assert.equal(restored.turns[0].provider, 'cursor');
  assert.equal(restored.turns[0].effort, 'medium');
}

// Agent configuration is one undoable change and belongs to each draft and
// checkpoint independently. setModel remains compatible with old callers.
{
  const state = M.create();
  const selected = tail(state);
  const depth = state._undo.length;
  M.setAgent(state, selected.id, { effort: 'high', speed: 'fast' });
  assert.equal(state._undo.length, depth + 1);
  assert.equal(tail(state).effort, 'high');
  M.undo(state);
  assert.equal(tail(state).effort, 'medium');
  M.redo(state);
  assert.equal(tail(state).speed, 'fast');
  const continuation = M.continueFrom(state, tail(state).id, {});
  const continued = cards(state).find(card => card.id === continuation);
  assert.equal(continued.effort, 'high');
  assert.equal(continued.speed, 'fast');
  M.setModel(state, continuation, 'Other model');
  assert.equal(cards(state).find(card => card.id === continuation).model, 'Other model');
  assert.equal(cards(state).find(card => card.id === continuation).effort, 'high');
  const fork = M.continueFrom(state, cards(state)[0].id, { fork: true });
  assert.equal(cards(state).find(card => card.id === fork).effort, 'medium');
  assert.equal(cards(state).find(card => card.id === fork).model, 'Auto');
}

// Separate drafts at one fork point must never exchange text during undo,
// layout conversion or reload. The old parent-keyed recovery did exactly that.
{
  const state = M.create();
  const first = cards(state)[0];
  const a = M.continueFrom(state, first.id, { fork: true });
  M.setDraft(state, a, 'Branch A');
  const b = M.continueFrom(state, first.id, { fork: true });
  M.setDraft(state, b, 'Branch B');
  M.setAgent(state, a, { provider: 'codex', model: 'Model A', effort: 'low', speed: 'fast' });
  M.rename(state, b, 'Renamed B');
  M.setDraft(state, a, 'Newest A');
  M.setDraft(state, b, 'Newest B');
  M.undo(state);
  assert.equal(cards(state).find(card => card.id === a).draftText, 'Newest A');
  assert.equal(cards(state).find(card => card.id === b).draftText, 'Newest B');
  M.changePresentation(state, 'window');
  const restored = JSON.parse(JSON.stringify(state));
  const projected = M.project(restored);
  assert.equal(projected.find(card => card.id === a).draftText, 'Newest A');
  assert.equal(projected.find(card => card.id === b).draftText, 'Newest B');
  assert.equal(projected.find(card => card.id === a).model, 'Model A');
  assert.equal(projected.find(card => card.id === b).model, 'Auto');
  const before = restored.turns.length;
  const sent = M.send(restored, a, 'Send A');
  M.append(restored, sent.assistantId, 'Answer A', true);
  assert.equal(restored.turns.length, before + 2);
  assert.equal(M.project(restored).find(card => card.id === b).draftText, 'Newest B');
  assert.equal(M.project(restored).filter(card => card.turnIds.includes(sent.assistantId)).length, 1);
}

// Version-one saves receive missing fields and migrate a legacy tail selection
// to its checkpoint so it remains selected when entering a single window.
{
  const state = M.create();
  const selected = tail(state);
  for (const turn of state.turns) { delete turn.effort; delete turn.speed; }
  delete state.effort; delete state.speed; delete state.nextAgents; delete state.configVersion;
  state.meta[selected.id] = { pendingModel: 'Legacy selection' };
  const restored = JSON.parse(JSON.stringify(state));
  assert.equal(tail(restored).model, 'Legacy selection');
  assert.equal(tail(restored).effort, 'medium');
  M.changePresentation(restored, 'window');
  assert.equal(M.project(restored)[0].model, 'Legacy selection');
  assert.equal(restored.turns.length, 6);
}

// Provider changes rename only default provider titles. Human branch titles
// remain intact, and the automatic title survives entering a single window.
{
  const state = M.create();
  const id = tail(state).id;
  M.setAgent(state, id, { provider: 'codex', model: 'Default' });
  assert.equal(tail(state).title, 'Codex');
  M.setAgent(state, id, { effort: 'high' });
  assert.equal(tail(state).title, 'Codex');
  M.changePresentation(state, 'window');
  assert.equal(M.project(state)[0].title, 'Codex');
  const windowId = M.project(state)[0].id;
  M.rename(state, windowId, 'Courtyard exploration');
  assert.equal(M.project(state)[0].title, 'Courtyard exploration');
  M.setAgent(state, windowId, { provider: 'openai-text', model: 'API model' });
  assert.equal(M.project(state)[0].title, 'Courtyard exploration');
  M.undo(state);
  assert.equal(M.project(state)[0].provider, 'codex');
  assert.equal(M.project(state)[0].title, 'Courtyard exploration');
  const draft = M.continueFrom(state, windowId, { fork: true });
  M.setAgent(state, draft, { provider: 'local', model: 'Local model' });
  assert.equal(M.project(state).find(card => card.id === draft).title, 'Courtyard exploration');
}

// A logical draft absorbed by a window remains a valid edit/config/send target
// after reload, and clicking Continue again cannot create an invisible duplicate.
{
  const state = M.create();
  M.changePresentation(state, 'window');
  const windowId = M.project(state)[0].id;
  const draftId = M.continueFrom(state, windowId, {});
  const restored = JSON.parse(JSON.stringify(state));
  assert.equal(M.setDraft(restored, draftId, 'Use the returned draft identifier.'), true);
  assert.equal(restored.activeCardId, windowId);
  assert.equal(M.continueFrom(restored, windowId, {}), draftId);
  assert.equal(restored.drafts.length, 1);
  assert.equal(M.setAgent(restored, draftId, { provider: 'codex', model: 'Codex model', effort: 'high', speed: 'fast' }), true);
  const sent = M.send(restored, draftId);
  assert.ok(sent);
  M.append(restored, sent.assistantId, 'Same window.', true);
  assert.equal(M.project(restored).length, 1);
  assert.equal(restored.turns.at(-1).model, 'Codex model');
  assert.equal(restored.drafts.length, 0);
}
// Horizontal resizing retains the authored width while height follows wrapping.
{
  const s=M.create(),id=tail(s).id;
  M.resize(s,id,500,null);
  const fit=c=>({width:c.customSize?.width||320,height:100000/(c.customSize?.width||320)});
  assert.equal(M.project(s,fit).at(-1).height,200);
  M.setFold(s,id,'collapsed');M.project(s,fit);M.setFold(s,id,'open');
  assert.equal(M.project(s,fit).at(-1).width,500);
  M.resize(s,id,250,null);
  assert.equal(M.project(s,fit).at(-1).height,400);
  const loaded=JSON.parse(JSON.stringify(s));
  assert.equal(M.project(loaded,fit).at(-1).width,250);
  M.undo(s);assert.equal(M.project(s,fit).at(-1).width,500);
}
// Full expansion releases a fixed height but retains width and survives reload.
{
  const s=M.create();M.changePresentation(s,'window');const id=M.project(s)[0].id;
  M.resize(s,id,490,240);M.expandFull(s,id);
  const restored=JSON.parse(JSON.stringify(s));const c=M.project(restored,card=>({width:card.customSize?.width||440,height:card.fullExpanded?1200:560}))[0];
  assert.equal(c.width,490);assert.equal(c.height,1200);assert.equal(c.fullExpanded,true);
  M.setFold(restored,id,'collapsed');M.setFold(restored,id,'open');
  assert.equal(M.project(restored)[0].width,490);
}
console.log('Chat-train model: 22 behavior groups passed.');
