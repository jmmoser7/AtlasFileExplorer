# Slate Cursor sidecar

This is a minimal local sidecar for Slate Agent portals. It reads:

- `<ai-workspace>/.atlas-ai/agent/<session>/context.json`
- `<ai-workspace>/.atlas-ai/agent/<session>/request.json`

and writes:

- `<ai-workspace>/.atlas-ai/agent/<session>/session.json`
- `<ai-workspace>/.atlas-ai/agent/<session>/sidecar.pid` (its own pid)
- optional proposals under `<ai-workspace>/.atlas-ai/stage/`

Only one sidecar watches a link folder: the newest one writes `sidecar.pid`,
and an older one that finds another pid there exits. When Slate starts it,
`ATLAS_PARENT_PID` names the Slate process, and the sidecar also exits once
that process is gone. See "Sidecar supervision" in `../../agent-link-contract.md`.

## Run

See [SETUP.md](SETUP.md) for the human path (mint a key, paste it in the
portal, install Node if asked). Manual start (`install.mjs` needs no npm;
any Node 22.13 or newer works, including Cursor's bundled `node.exe`):

```powershell
node install.mjs
$env:CURSOR_API_KEY = "cursor_..."
$env:ATLAS_AI_WORKSPACE = "C:\path\to\ai-workspace"
$env:ATLAS_AGENT_SESSION = "agent-..."
$env:ATLAS_AGENT_CWD = "C:\path\to\bound\project"
node index.mjs
```

`ATLAS_AGENT_SESSION` is shown in the selected Agent portal inspector. The
sidecar uses a local Cursor SDK runtime with `cwd = ATLAS_AGENT_CWD` (the
bound project folder) when set, otherwise the AI workspace. Slate may spawn
this script on Send: it finds `node.exe` in Program Files (not only PATH),
falls back to `Cursor.exe` run as Node, and runs `install.mjs` here when
`@cursor/sdk` is missing.

Agents must not edit `.slate` files directly. Board edits are proposed by
writing stage files described in `../../agent-link-contract.md`; Slate commits
accepted proposals as attributed journal groups.
