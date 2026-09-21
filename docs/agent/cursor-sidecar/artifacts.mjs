import path from 'node:path';

// Normalize only successful, structured tool results. Never parse assistant prose.
export function artifactFromTool(tool, callId, turn, cwd) {
  if (tool?.result?.status !== 'success') return null;
  const kinds = { read:'read', edit:'modified', write:'modified', delete:'deleted', webFetch:'web' };
  const kind = kinds[tool.type];
  const locator = kind === 'web' ? tool.args?.url : tool.args?.path;
  if (!kind || typeof locator !== 'string' || !locator) return null;
  if (kind === 'web' && !/^https?:\/\//i.test(locator)) return null;
  const source = kind === 'web' ? locator : path.resolve(cwd,locator);
  return { id:callId + ':' + source, turn, kind, source, title:locator };
}

export function messageText(message) {
  if (typeof message === 'string') return message;
  if (typeof message?.text === 'string') return message.text;
  return (message?.content ?? []).filter(b => b.type === 'text').map(b => b.text).join('\n');
}

// A train exchange has one reply even when a run emits several assistant messages.
export function transcript(messages, previous=[]) {
  const turns=[]; let lastRun;
  for (const m of messages) {
    if (m.type !== 'user' && m.type !== 'assistant') continue;
    const raw=messageText(m.message);
    const text=m.type==='user' ? displayPrompt(raw) : raw;
    if (!text) continue;
    if (m.type==='assistant' && m.run_id && lastRun===m.run_id && turns.at(-1)?.role==='assistant') {
      turns.at(-1).text+='\n'+text;
    } else {
      const old=previous[turns.length];
      const ownPrompt=m.type==='user' && text.startsWith('You are linked to a Slate Agent portal.') && old?.role==='user' && text.endsWith('User prompt:\n'+old.text);
      turns.push({role:m.type,text:ownPrompt?old.text:text,at:old?.at??0});
    }
    lastRun=m.run_id;
  }
  return turns;
}

// The attachment transport suffix is metadata, not part of the displayed message.
export function displayPrompt(text) {
  const marker='\n\nSlate wired attachments (data):\n';
  const i=text.lastIndexOf(marker);
  if(i>=0) { try { if(Array.isArray(JSON.parse(text.slice(i+marker.length)))) return text.slice(0,i); } catch {} }
  return text;
}
