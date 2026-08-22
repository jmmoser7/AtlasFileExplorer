# Reach a Cursor agent from a Slate portal

A send that cannot reach Cursor must name the next step — never a blank
card, and never a dead-end sentence.

## 1. Mint a user API key

Open [Cursor Dashboard → API Keys](https://cursor.com/dashboard/api) and
create a **User API key**. The full secret is shown once; copy it from the
create dialog, not from the masked table later.

Background: [Cursor CLI authentication](https://cursor.com/docs/cli/reference/authentication).

## 2. Give Slate the key

Pick one:

- **In the portal** (preferred): click **Paste key**, paste, press Enter.
  Slate stores it next to `ai-config.json` on this machine and retries the
  send. You do not need a system environment variable.
- **Environment variable:** set `CURSOR_API_KEY` for your user, then restart
  Slate. The sidecar also reads this if it is already set.

The key is never written into a `.slate` workbook.

## 3. Node sidecar (only if the portal asks for it)

The portal talks to Cursor through `docs/agent/cursor-sidecar` (out of
process). On Send, Slate looks for `node.exe` in this order: `ATLAS_NODE`,
`C:\Program Files\nodejs\node.exe`, other well-known folders, then `PATH`.
A GUI launch often misses a terminal-only PATH, so "Node not found" does
not always mean Node is missing. If discovery fails, the portal lists the
paths it tried.

The first Send also runs `npm install` in this folder when `@cursor/sdk`
is missing (off the UI thread). You do not have to do that by hand unless
the portal reports that install failed.

If the portal still cannot see Node:

1. Confirm `C:\Program Files\nodejs\node.exe` exists, or set `ATLAS_NODE`
   to your `node.exe` and restart Slate.
2. Otherwise install [Node.js](https://nodejs.org/en/download).
3. Send again.
