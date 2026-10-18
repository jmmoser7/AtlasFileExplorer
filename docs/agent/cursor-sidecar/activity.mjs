export const ACTIVITY_LIMIT = 64;
export const ACTIVITY_TEXT_LIMIT = 1024;
const KINDS = new Set(['commentary', 'tool_started', 'tool_finished']);
// Stable protocol IDs; face/prop choices live only in atlas-agent's catalog.
const ACTIONS = new Set(['idle', 'resting', 'listening', 'reasoning', 'reading', 'searching', 'writing', 'editing',
  'creating-image', 'editing-image', 'organizing', 'moving', 'resizing', 'frames',
  'presentations', 'wiring', 'working', 'tool-waiting', 'approval', 'question',
  'blocked', 'error', 'verifying', 'success', 'chat', 'exchanging', 'delegating',
  'subagent-work', 'discovery', 'cancel', 'retire', 'retired', 'restore', 'offline']);
const TOOL_ACTIONS = Object.freeze({ read: 'reading', grep: 'searching', glob: 'searching',
  ls: 'discovery', semSearch: 'searching', readLints: 'verifying', edit: 'editing',
  write: 'editing', delete: 'editing', applyAgentDiff: 'editing', generateImage: 'creating-image',
  task: 'delegating', shell: 'working', mcp: 'working' });

function boundedSemantics(value) {
  if (!value || !ACTIONS.has(value.action)) return undefined;
  const semantics = { action: value.action }, id = value.target?.node_id;
  if (typeof id === 'string' && id.trim() && Array.from(id).length <= 256 && !/[\p{Cc}]/u.test(id))
    semantics.target = { node_id: id };
  return semantics;
}

export function boundedActivity(items) {
  return (Array.isArray(items) ? items : []).filter((item) => item
    && typeof item.id === 'string' && typeof item.request === 'string'
    && KINDS.has(item.kind) && typeof item.text === 'string' && Number.isFinite(item.at))
    .slice(-ACTIVITY_LIMIT).map((item) => {
      const row = { id: item.id, request: item.request, kind: item.kind,
        text: Array.from(item.text).slice(0, ACTIVITY_TEXT_LIMIT).join(''), at: item.at };
      const semantics = boundedSemantics(item.semantics);
      if (semantics) row.semantics = semantics;
      return row;
    });
}

// Only public text and tool types enter activity. Thinking, tool arguments,
// command strings, and raw tool outputs never pass through this adapter.
export function recordActivity(items, update, request, at) {
  let kind, text, action;
  if (update?.type === 'text-delta' && typeof update.text === 'string') {
    kind = 'commentary';
    action = 'chat';
    text = update.text.replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, '');
  } else if (update?.type === 'tool-call-started' || update?.type === 'tool-call-completed') {
    kind = update.type === 'tool-call-started' ? 'tool_started' : 'tool_finished';
    const type = /^[a-z][a-z0-9_-]{0,63}$/i.test(update.toolCall?.type ?? '') ? update.toolCall.type : 'tool';
    const verb = kind === 'tool_started' ? 'Started' : update.toolCall?.result?.status === 'error' ? 'Failed' : 'Finished';
    text = `${verb} ${type === 'tool' ? 'tool' : `${type} tool`}.`;
    action = verb === 'Failed' ? 'error' : Object.hasOwn(TOOL_ACTIONS, type) ? TOOL_ACTIONS[type] : 'working';
  }
  if (!kind || !text) return items;
  const next = boundedActivity(items).filter((item) => item.request === request);
  const characters = Array.from(text);
  let sequence = Number(next.at(-1)?.id.split(':').at(-1)) || 0;
  if (kind === 'commentary' && next.at(-1)?.kind === 'commentary') {
    const previous = next.at(-1);
    const count = ACTIVITY_TEXT_LIMIT - Array.from(previous.text).length;
    previous.text += characters.splice(0, count).join('');
    previous.at = at;
    previous.semantics = { action };
  }
  for (let offset = 0; offset < characters.length; offset += ACTIVITY_TEXT_LIMIT) {
    next.push({ id: `${request}:activity:${++sequence}`, request, kind,
      text: characters.slice(offset, offset + ACTIVITY_TEXT_LIMIT).join(''), at, semantics: { action } });
  }
  return next.slice(-ACTIVITY_LIMIT);
}
