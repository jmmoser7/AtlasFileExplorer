# Slate Cursor sidecar

This is a minimal local sidecar for Slate Agent portals. It reads:

- `<ai-workspace>/.atlas-ai/agent/<session>/context.json`
- `<ai-workspace>/.atlas-ai/agent/<session>/request.json`

and writes:

- `<ai-workspace>/.atlas-ai/agent/<session>/session.json`
- optional proposals under `<ai-workspace>/.atlas-ai/stage/`

## Run

```powershell
npm install
$env:CURSOR_API_KEY = "cursor_..."
$env:ATLAS_AI_WORKSPACE = "C:\path\to\ai-workspace"
$env:ATLAS_AGENT_SESSION = "agent-..."
npm start
```

`ATLAS_AGENT_SESSION` is shown in the selected Agent portal inspector. The
sidecar uses a local Cursor SDK runtime with `cwd = ATLAS_AI_WORKSPACE`.

Agents must not edit `.slate` files directly. Board edits are proposed by
writing stage files described in `../../agent-link-contract.md`; Slate commits
accepted proposals as attributed journal groups.
