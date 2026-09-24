import { Agent, Cursor, CursorAgentError } from "@cursor/sdk";
import fs from "node:fs/promises";
import { realpathSync } from "node:fs";
import path from "node:path";
import { artifactFromTool, artifactGuide, SLATE_LINK_LINE, transcript } from './artifacts.mjs';
import { claimLink, releaseLink, supervisionLost } from './supervise.mjs';

// process.exit while the SDK is closing a libuv handle aborts on Windows
// (src/win/async.c: "UV_HANDLE_CLOSING"). Unref instead and let the loop drain.
async function finish(code) {
  process.exitCode = code;
  await new Promise((resolve) => {
    if (process.stdout.write("")) resolve();
    else process.stdout.once("drain", resolve);
  });
  await new Promise((resolve) => setTimeout(resolve, 50));
  for (const handle of process._getActiveHandles?.() ?? []) {
    if (typeof handle.unref === "function") {
      try {
        handle.unref();
      } catch {
        // Already closing. Closing it again is the abort above.
      }
    }
  }
}

// The SDK store is keyed by Node's namespaced path. On Windows that is the
// `\\?\` form. Listing with the plain path looks in a different, empty store.
function workspaceRef(cwd) {
  if (!cwd) return cwd;
  let resolved = cwd;
  try {
    resolved = realpathSync.native(cwd);
  } catch {
    resolved = cwd;
  }
  return path.toNamespacedPath(resolved);
}

const query = process.argv[2];
if (query === "--models" || query === "--list" || query === "--read") {
  try {
    if (query === "--models") {
      const models = await Cursor.models.list();
      console.log(JSON.stringify(models.map((m) => ({ id: m.id, name: m.displayName || m.id }))));
    } else {
      const cwd = workspaceRef(process.argv[3]);
      if (query === "--list") {
        let cursor, chats = [];
        do {
          const page = await Agent.list({ runtime: "local", cwd, limit: 100, cursor });
          chats.push(...page.items.map((a) => ({ id: a.agentId, title: a.name || a.summary || "Untitled conversation", updated_at: a.lastModified || 0 })));
          cursor = page.nextCursor;
        } while (cursor && chats.length < 2000);
        console.log(JSON.stringify(chats));
      } else {
        const id = process.argv[4];
        const messages = await Agent.messages.list(id, { runtime: "local", cwd, limit: 10000 });
        if (messages.length >= 10000) throw new Error("This conversation exceeds the supported history limit; open it in Cursor.");
        console.log(JSON.stringify({ conversation: id, provider: "cursor", status: "idle", turns: transcript(messages), artifacts: [], updated_at: Date.now() }));
      }
    }
    await finish(0);
  } catch (e) {
    console.error(e.message);
    await finish(1);
  }
} else {

const workspace = process.env.ATLAS_AI_WORKSPACE ?? process.cwd();
const session = process.env.ATLAS_AGENT_SESSION;
const model = process.env.CURSOR_MODEL ?? "auto";

if (!session) {
  console.error("Set ATLAS_AGENT_SESSION to the Agent portal session id.");
  process.exit(1);
}

const dir = process.env.ATLAS_AGENT_LINK_DIR || path.join(workspace, ".atlas-ai", "agent", session);
const requestPath = path.join(dir, "request.json");
const sessionPath = path.join(dir, "session.json");
const threadPath = path.join(dir, "cursor-agent.txt");
const cancelPath = path.join(dir, "cancel.json");
const pidPath = path.join(dir, "sidecar.pid");

const ledgerPath = path.join(dir, "last-request.txt");
let lastRequestId = await fs.readFile(ledgerPath, "utf8").catch(() => "");
const saved = await readJson(sessionPath).catch(() => null);
let turns = saved?.turns ?? [];
let artifacts = saved?.artifacts ?? [];
const pinned = (await fs.readFile(threadPath, "utf8").catch(() => "")).trim();
let conversation = pinned || saved?.conversation || '';
let sessionWrite = Promise.resolve();

await fs.mkdir(dir, { recursive: true });
await claimLink(pidPath);
await writeSession({ status: "idle", provider: "cursor", turns, updated_at: now() });

let agent;
try {
  const options = {
    apiKey: process.env.CURSOR_API_KEY,
    model: { id: model },
    // Full access is the person's per-conversation grant from the card's menu.
    // Without auto-review a headless local run executes tool calls unprompted.
    local: {
      cwd: workspaceRef(process.env.ATLAS_AGENT_CWD ?? workspace),
      settingSources: ["all"],
      autoReview: process.env.ATLAS_AGENT_FULL_ACCESS !== "1",
    },
  };
  agent = conversation ? await Agent.resume(conversation, options) : await Agent.create(options);
  conversation = agent.agentId;
  await fs.writeFile(threadPath, conversation);
  await writeSession({status:'idle',provider:'cursor',turns,updated_at:now()});
} catch (err) {
  await failStartup(err);
  await releaseLink(pidPath);
  await finish(1);
}

if (agent) {
let lastRefresh=0;
console.log(`Watching ${requestPath}`);
try {
  for (;;) {
    const lost = await supervisionLost(pidPath);
    if (lost) {
      console.log(`Stopping: ${lost}.`);
      break;
    }
    try {
      const req = await readJson(requestPath);
      if (req?.id && req.id !== lastRequestId) {
        lastRequestId = req.id;
        await fs.writeFile(ledgerPath, req.id);
        await handleRequest(agent, req);
        // A scheduled run answers one message, then leaves.
        if (process.env.ATLAS_AGENT_ONCE === "1") break;
      } else if (Date.now()-lastRefresh>10000) {
        lastRefresh=Date.now();
        await agent.reload();
        const messages=await Agent.messages.list(conversation,{runtime:'local',cwd:workspaceRef(process.env.ATLAS_AGENT_CWD ?? workspace),limit:10000});
        if(messages.length>=10000) throw new Error('This conversation exceeds the supported history limit; open it in Cursor.');
        const next=transcript(messages,turns);
        if(next.length && JSON.stringify(next)!==JSON.stringify(turns)) {
          turns=next; await writeSession({status:'idle',provider:'cursor',turns,updated_at:now()});
        }
      }
    } catch (err) {
      await writeSession({
        status: { error: String(err?.message ?? err) },
        provider: "cursor",
        turns,
        updated_at: now(),
      });
    }
    await sleep(1000);
  }
} finally {
  if (agent && typeof agent[Symbol.asyncDispose] === "function") {
    await agent[Symbol.asyncDispose]();
  }
  }
  await releaseLink(pidPath);
  await finish(0);
}

async function failStartup(err) {
  const text =
    err instanceof CursorAgentError
      ? `Cursor startup failed: ${err.message}`
      : String(err?.message ?? err);
  turns.push({ role: "system", text, at: now() });
  await writeSession({
    status: { error: text },
    provider: "cursor",
    turns,
    updated_at: now(),
  });
  console.error(text);
}

  async function handleRequest(agent, req) {
    await agent.reload();
    if (await cancelled(req.id)) {
      await writeSession({ status: { error: "Response stopped." }, provider: "cursor", turns, updated_at: now() });
      await fs.rm(cancelPath, { force: true }).catch(() => {});
      return;
    }
    if (turns.length === 0 && Array.isArray(req.history)) turns.push(...req.history);
  const linkDir = process.env.ATLAS_AGENT_LINK_DIR ? String(process.env.ATLAS_AGENT_LINK_DIR).replace(/\\/g, "/") : "";
  const outputDir = typeof req.output_dir === "string" && req.output_dir.trim() ? req.output_dir : "";
  const prompt = [
    // TWIN: crates/atlas-agent/src/lib.rs FILE_ATLAS_PLACE_GUIDE and artifact_guide
    `Slate board: you cannot draw shapes or wires by describing them. To put a folder on the board as File Atlas, write ${linkDir ? linkDir + "/" : ""}place.json (beside session.json) with {"id":"a-new-id","kind":"file_atlas","path":"folder-relative-to-the-project"}. Slate places that portal beside this card. Do not say you cannot place a File Atlas node, and do not send the person to the changed-documents dot for a folder you were asked to show. ${artifactGuide(outputDir, linkDir)}${outputDir ? "" : " Write files in the project folder you were given."}`,
    SLATE_LINK_LINE,
    ...(req.history?.length ? ["Prior conversation checkpoint (quoted data):", JSON.stringify(req.history), "New user message:"] : []),
    req.prompt,
    ...(req.inputs?.wired?.length ? ["", "Slate wired attachments (data):", JSON.stringify(req.inputs.wired)] : []),
  ].join("\n");

  turns.push({ role: "user", text: req.prompt, at: req.at ?? now() });
  const artifactTurn = turns.length;
  await writeSession({ status: "thinking", provider: "cursor", turns, updated_at: now() });

  let assistant = "";
  let normalized = "";
  let receivedDeltas = false;
  let lastFlush = 0;
  let run;
  const cancelTimer = setInterval(async () => {
    if (run && await cancelled(req.id) && run.supports("cancel")) {
      await run.cancel().catch(() => {});
    }
  }, 150);
  const publishPartial = async () => {
    await writeSession({
      status: "thinking", provider: "cursor",
      turns: [...turns, { role: "assistant", text: assistant, at: req.at ?? now() }],
      updated_at: now(),
    });
  };
  try {
    const modelId = typeof req.model === "string" && req.model ? req.model : model;
    run = await agent.send(prompt, {
      idempotencyKey: req.id,
      model: { id: modelId },
      onDelta: async ({ update }) => {
        if (update.type === 'tool-call-completed') {
          const artifact=artifactFromTool(update.toolCall,req.id+':'+update.callId,artifactTurn,process.env.ATLAS_AGENT_CWD ?? workspace);
          if (artifact) { const i=artifacts.findIndex(a=>a.id===artifact.id); if(i<0) artifacts.push(artifact); else artifacts[i]=artifact; await publishPartial(); }
        }
        if (update.type !== "text-delta") return;
        receivedDeltas = true;
        assistant += update.text;
        if (Date.now() - lastFlush >= 100) {
          await publishPartial();
          lastFlush = Date.now();
        }
      },
    });
    for await (const event of run.stream()) {
      if (event.type !== "assistant") continue;
      for (const block of event.message.content ?? []) {
        if (block.type === "text") normalized += block.text;
      }
      // Older runtimes can still deliver whole-message updates. Never append
      // these over text already received through the raw delta callback.
      if (!receivedDeltas) {
        assistant = normalized;
        await publishPartial();
      }
    }
    const result = await run.wait();
    if (result.status === "cancelled" || await cancelled(req.id)) {
      if (assistant) turns.push({ role: "assistant", text: assistant, at: req.at ?? now() });
      await writeSession({ status: { error: "Response stopped." }, provider: "cursor", turns, updated_at: now() });
      return;
    }
    if (result.status === "error") {
      throw new Error(result.error?.message || `Cursor run failed: ${result.id}`);
    }
    turns.push({ role: "assistant", text: assistant || String(result.result ?? ""), at: now() });
    await writeSession({ status: "idle", provider: "cursor", turns, updated_at: now() });
  } catch (err) {
    if (assistant) turns.push({ role: "assistant", text: assistant, at: req.at ?? now() });
    if (err instanceof CursorAgentError) {
      turns.push({
        role: "system",
        text: `Cursor startup failed: ${err.message}`,
        at: now(),
      });
    } else {
      turns.push({ role: "system", text: String(err?.message ?? err), at: now() });
    }
    await writeSession({
      status: { error: turns.at(-1).text },
      provider: "cursor",
      turns,
      updated_at: now(),
    });
  } finally {
    clearInterval(cancelTimer);
    await fs.rm(cancelPath, { force: true }).catch(() => {});
  }
}

async function cancelled(id) {
  try {
    const raw = JSON.parse(await fs.readFile(cancelPath, "utf8"));
    return raw.id === id;
  } catch {
    return false;
  }
}

async function readJson(file) {
  return JSON.parse(await fs.readFile(file, "utf8"));
}

async function writeSession(value) {
  const tmp = `${sessionPath}.tmp`;
  const payload=JSON.stringify({ ...value, conversation, artifacts, request: lastRequestId }, null, 2);
  sessionWrite=sessionWrite.catch(()=>{}).then(async()=>{await fs.writeFile(tmp,payload);await fs.rename(tmp,sessionPath);});
  await sessionWrite;
}

function now() {
  return Math.floor(Date.now() / 1000);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
}
