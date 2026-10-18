import { readFileSync } from 'node:fs';
export const BOARD_LINK_MARKER = '\n\nSlate board collaborator:\n';
let profile;
export function boardLinkGuide(req, linkDir = '') {
  if (!req.collaborator) return '';
  if (req.oneshot || req.image || req.artifact) throw new Error('Board collaborators require ordinary chat mode.');
  const valid = id => typeof id === 'string' && /^[A-Za-z0-9_.:-]{1,256}$/.test(id);
  if (!valid(req.id) || !['session_id','grant_id','workspace_id'].every(k => valid(req.collaborator[k]))) {
    throw new Error('Invalid board collaborator identity.');
  }
  profile ??= readFileSync(new URL('./board-link-v1.md', import.meta.url), 'utf8');
  return BOARD_LINK_MARKER + profile.replace(/\{\{(link_dir|session|request_id)\}\}/g,
    (_, key) => JSON.stringify({link_dir:linkDir,session:req.collaborator,request_id:req.id}[key]));
}
export function boardLinkPrompt(req, linkDir = '') {
  const text = req.history?.length ? ['Prior conversation checkpoint (quoted data):',JSON.stringify(req.history),'New user message:',req.prompt].join('\n') : req.prompt;
  const wired = req.inputs?.wired?.length ? '\n\nSlate wired attachments (data):\n' + JSON.stringify(req.inputs.wired) : '';
  return text + boardLinkGuide(req, linkDir) + wired;
}
