# Agent portal link contract — context, requests, sessions, and stage

This document is the file contract between Slate's **Agent portal** and local
agent sidecars such as the Cursor SDK template under `docs/agent/cursor-sidecar`.
Slate never embeds an agent runtime. It writes plain JSON into the shared AI
workspace; local sidecars read those files, write status back, and propose board
edits through the staging layer.

## 1. Purpose

The Agent portal is a host-class portal. Its frame is authored board data, but
its live contents are a foreign local process. Export is therefore a poster plus
a pointer. Board mutations from the agent are never applied directly; they are
ordinary `SceneCmd` proposals that the human accepts or rejects as a unit.

## 2. Paths

Given `<ai-workspace>`:

| File | Path | Writer | Reader |
|------|------|--------|--------|
| Context | `<ai-workspace>/.atlas-ai/agent/<session>/context.json` | Slate | Sidecars |
| Prompt request | `<ai-workspace>/.atlas-ai/agent/<session>/request.json` | Slate | Sidecars |
| Session state | `<ai-workspace>/.atlas-ai/agent/<session>/session.json` | Sidecars | Slate |
| Proposal | `<ai-workspace>/.atlas-ai/stage/<id>.json` | Sidecars | Slate |
| Proposal result | `<ai-workspace>/.atlas-ai/stage/<id>.result.json` | Slate | Sidecars |

Writes are atomic: write `*.tmp`, then rename. Slate polls at most once per
second and mtime-gates reads. Unknown fields are ignored.

## 3. `context.json`

```json
{
  "app": "slate",
  "session": "agent-18fb...",
  "provider": "cursor",
  "workbook": "C:/work/board.slate",
  "format_version": 2,
  "scope": "selection",
  "selection": ["node:42"],
  "viewport": { "x": 0, "y": 0, "w": 1440, "h": 900, "zoom": 1.0 },
  "board_summary": "17 board nodes, 1 selected",
  "generated_at": 1780000000
}
```

`scope` is `selection`, `frame`, or `board`. Presence, cursors, and remote
viewports are not part of this channel.

## 4. `request.json`

Slate overwrites this file when the human clicks **Send**:

```json
{
  "id": "req-1780000000",
  "prompt": "Arrange the selected screenshots into a clean comparison.",
  "at": 1780000000
}
```

Sidecars should ignore a request id they have already processed.

## 5. `session.json`

Sidecars write the visible portal transcript and status:

```json
{
  "status": "thinking",
  "provider": "cursor",
  "turns": [
    { "role": "user", "text": "Arrange the selected screenshots", "at": 1780000000 },
    { "role": "assistant", "text": "I drafted a staged collage proposal.", "at": 1780000004 }
  ],
  "updated_at": 1780000004
}
```

`status` is `idle`, `thinking`, `offline`, or `{ "error": "message" }`.

## 6. Staged proposals

Sidecars write `<ai-workspace>/.atlas-ai/stage/<id>.json`:

```json
{
  "id": "proposal-1",
  "author": "cursor-agent",
  "title": "Move selected nodes into a comparison row",
  "created_at": 1780000004,
  "target": { "workbook": "C:/work/board.slate", "format_version": 2 },
  "session": "agent-18fb...",
  "status": "pending",
  "cmds": []
}
```

`cmds` are serialized `SceneCmd`s. Slate normalizes `Add.index` to the top of
the z-list before applying, and accepts only when the whole command group still
applies. On acceptance the journal group author is `CmdAuthor::Agent(author)`.

Slate writes `<id>.result.json`:

```json
"accepted"
```

or:

```json
{ "stale": { "reason": { "unsupported_format": { "found": 3, "expected": 2 } } } }
```

## 7. Forward compatibility

This file contract is the stable local surface. A future `atlas-mcp` server will
wrap the same context/read/propose operations; sidecars that implement the file
contract continue to work unchanged.
