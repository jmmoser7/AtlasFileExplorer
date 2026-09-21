import { Agent, CursorAgentError } from "@cursor/sdk";
import fs from "node:fs/promises";
import path from "node:path";
import { artifactFromTool, transcript } from './artifacts.mjs';

if (process.argv[2] === '--list' || process.argv[2] === '--read') {
  try {
    const cwd = process.argv[3];
    if (process.argv[2] === '--list') {
      let cursor, chats = [];
      do {
        const page = await Agent.list({runtime:'local',cwd,limit:100,cursor});
        chats.push(...page.items.map(a => ({id:a.agentId,title:a.name || a.summary || 'Untitled conversation',updated_at:a.lastModified || 0})));
        cursor = page.nextCursor;
      } while (cursor && chats.length < 2000);
      console.log(JSON.stringify(chats));
    } else {
      const id=process.argv[4];
      const messages=await Agent.messages.list(id,{runtime:'local',cwd,limit:10000});
      if(messages.length>=10000) throw new Error('This conversation exceeds the supported history limit; open it in Cursor.');
      console.log(JSON.stringify({conversation:id,provider:'cursor',status:'idle',turns:transcript(messages),artifacts:[],updated_at:Date.now()}));
    }
    process.exit(0);
  } catch(e) { console.error(e.message); process.exit(1); }
}

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

const ledgerPath = path.join(dir, "last-request.txt");
let lastRequestId = await fs.readFile(ledgerPath, "utf8").catch(() => "");
const saved = await readJson(sessionPath).catch(() => null);
let turns = saved?.turns ?? [];
let artifacts = saved?.artifacts ?? [];
let conversation = saved?.conversation ?? '';
let sessionWrite = Promise.resolve();

await fs.mkdir(dir, { recursive: true });
await writeSession({ status: "idle", provider: "cursor", turns, updated_at: now() });

let agent;
try {
  const options = {
    apiKey: process.env.CURSOR_API_KEY,
    model: { id: model },
    local: { cwd: process.env.ATLAS_AGENT_CWD ?? workspace, settingSources: ["all"], autoReview: true },
  };
  agent = conversation ? await Agent.resume(conversation, options) : await Agent.create(options);
  conversation = agent.agentId;
  await writeSession({status:'idle',provider:'cursor',turns,updated_at:now()});
} catch (err) {
  await failStartup(err);
}

let lastRefresh=0;
console.log(`Watching ${requestPath}`);
try {
  for (;;) {
    try {
      const req = await readJson(requestPath);
      if (req?.id && req.id !== lastRequestId) {
        lastRequestId = req.id;
        await fs.writeFile(ledgerPath, req.id);
        await handleRequest(agent, req);
      } else if (Date.now()-lastRefresh>10000) {
        lastRefresh=Date.now();
        await agent.reload();
        const messages=await Agent.messages.list(conversation,{runtime:'local',cwd:process.env.ATLAS_AGENT_CWD ?? workspace,limit:10000});
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
  if (typeof agent[Symbol.asyncDispose] === "function") {
    await agent[Symbol.asyncDispose]();
  }
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
  // Delayed so libuv can close SDK handles. Immediate process.exit aborts on Windows.
  setTimeout(() => process.exit(1), 200);
}

  async function handleRequest(agent, req) {
    await agent.reload();
    if (turns.length === 0 && Array.isArray(req.history)) turns.push(...req.history);
  const prompt = [
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
  const publishPartial = async () => {
    await writeSession({
      status: "thinking", provider: "cursor",
      turns: [...turns, { role: "assistant", text: assistant, at: req.at ?? now() }],
      updated_at: now(),
    });
  };
  try {
    const run = await agent.send(prompt, {
      idempotencyKey: req.id,
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
    if (result.status === "error") {
      throw new Error(`Cursor run failed: ${result.id}`);
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
