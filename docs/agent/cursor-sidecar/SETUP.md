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
  Slate stores it in Windows Credential Manager for this Windows user
  (`atlas_core::secrets`, slot `cursor-api-key`) and retries the send.
  You do not need a system environment variable. Another person opening
  the same workbook does not receive the key.
- **Environment variable:** set `CURSOR_API_KEY` for your user, then restart
  Slate. The sidecar also reads this if it is already set. The environment
  variable wins over the stored key.

The key is never written into a `.slate` workbook, a journal entry, an
export, or the agent context beacon. A plaintext file left by an older
build is moved into Credential Manager on the next read and then deleted.

## 3. Node sidecar (only if the portal asks for it)

The portal talks to Cursor through `docs/agent/cursor-sidecar` (out of
process). On Send, Slate looks for `node.exe` in this order: `ATLAS_NODE`,
`C:\Program Files\nodejs\node.exe`, Cursor's bundled runtime
(`%LOCALAPPDATA%\Programs\cursor\resources\app\resources\helpers\node.exe`),
other well-known folders, then `PATH`.
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

## Project and conversation selection

Choose Cursor, choose a project, then select an SDK-local conversation or New conversation.
The SDK catalog is separate from legacy Cursor IDE composer history; an empty list is not an authentication failure.
Slate resumes the selected SDK identity instead of replaying its text into a new agent. Installed Cursor settings and SDK auto-review apply to local runs.

Successful structured read/edit/delete results populate the node's artifact circles. Left means references; right means changed documents. A click opens the list; selecting a listed artifact explicitly opens its portal. Text mentioning a filename alone never produces an artifact. Historical SDK transcripts may not expose past tool results, so an empty artifact list does not prove that no files were touched.
