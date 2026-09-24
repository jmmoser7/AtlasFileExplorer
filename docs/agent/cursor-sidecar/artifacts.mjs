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

// TWIN: crates/atlas-agent/src/lib.rs SLATE_LINK_GUIDE. Always the last line of the board preamble.
export const SLATE_LINK_LINE = "Slate Link (dashboard files): When you write an HTML dashboard or chart, include <script type=\"application/slate-link+json\">{\"version\":1,\"inputs\":[{\"name\":\"data\",\"kind\":\"table\"}]}</script>. Draw from window.slateLink.inputs.data (columns and rows supplied by a wire from a CSV or Excel file). If that input is absent, show a small sample and redraw when the page receives the slate-link event. Do not paste the spreadsheet into the HTML as the only copy of the data.";

// TWIN: crates/atlas-agent/src/lib.rs artifact_guide. The sidecar names the link folder.
export function artifactGuide(outputDir, linkDir = '') {
  const at = linkDir ? `${String(linkDir).replace(/\\/g, '/').replace(/\/$/, '')}/return.json (beside session.json)` : 'return.json beside session.json';
  const folder = typeof outputDir === 'string' && outputDir.trim()
    ? `Save new files the person did not place in ${outputDir}: deliverables at the top, supporting files in assets/, throwaway files in scratch/. `
    : '';
  return 'Every file you create or change must be a successful write, edit, or delete tool call with a real path; Slate lists those on the right gray circle of this card, and a path only mentioned in your reply is not listed. '
    + folder
    + `Edit existing files where they are. When finished, write ${at} naming what the person asked for: {"id":"a-new-id","title":"Short title","items":[{"path":"file-or-folder","as":"auto"}]}. To show a file that already exists, name its path instead of copying it. "as" is auto, graphic (render HTML or SVG as a page), text (show the source), images (a folder or several images as a grid), or folder (a File Atlas browser). A CSV or Excel item may add "feeds":"dashboard.html" to wire it into that dashboard. Slate lists these first and the person spawns them; do not place them yourself. place.json is only for an explicit File Atlas folder browser. If this task took too many steps, or an action was unreachable, also write feedback.json beside session.json as {"id":"a-new-id","what":"one sentence","tried":"what you did","missing":"the action you could not reach"}. No file contents, secrets, or the person's private text.`;
}

// The board preamble and attachment suffix are transport, not part of the displayed message.
export function displayPrompt(text) {
  const marker='\n\nSlate wired attachments (data):\n';
  const i=text.lastIndexOf(marker);
  if(i>=0) { try { if(Array.isArray(JSON.parse(text.slice(i+marker.length)))) text=text.slice(0,i); } catch {} }
  const end=text.startsWith('Slate board:') ? text.indexOf(SLATE_LINK_LINE+'\n') : -1;
  if(end>=0) {
    text=text.slice(end+SLATE_LINK_LINE.length+1);
    const checkpoint='Prior conversation checkpoint (quoted data):\n', next='\nNew user message:\n';
    const j=text.startsWith(checkpoint) ? text.indexOf(next) : -1;
    if(j>=0) text=text.slice(j+next.length);
  }
  return text;
}
