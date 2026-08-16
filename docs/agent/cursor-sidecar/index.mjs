import { Agent, CursorAgentError } from "@cursor/sdk";
import fs from "node:fs/promises";
import path from "node:path";

const workspace = process.env.ATLAS_AI_WORKSPACE ?? process.cwd();
const session = process.env.ATLAS_AGENT_SESSION;
const model = process.env.CURSOR_MODEL ?? "auto";

if (!session) {
  console.error("Set ATLAS_AGENT_SESSION to the Agent portal session id.");
  process.exit(1);
}

const dir = path.join(workspace, ".atlas-ai", "agent", session);
const requestPath = path.join(dir, "request.json");
const contextPath = path.join(dir, "context.json");
const sessionPath = path.join(dir, "session.json");

let lastRequestId = "";
let turns = [];

await fs.mkdir(dir, { recursive: true });
await writeSession({ status: "idle", provider: "cursor", turns, updated_at: now() });

await using agent = await Agent.create({
  apiKey: process.env.CURSOR_API_KEY,
  model: { id: model },
  local: { cwd: workspace },
});

console.log(`Watching ${requestPath}`);
for (;;) {
  try {
    const req = await readJson(requestPath);
    if (req?.id && req.id !== lastRequestId) {
      lastRequestId = req.id;
      await handleRequest(agent, req);
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

async function handleRequest(agent, req) {
  const context = await readJson(contextPath).catch(() => null);
  const prompt = [
    "You are linked to a Slate Agent portal.",
    "Use the context JSON below as canvas context.",
    "If you propose board edits, write a proposal JSON under .atlas-ai/stage/; do not edit the .slate file directly.",
    "",
    "Context:",
    JSON.stringify(context, null, 2),
    "",
    "User prompt:",
    req.prompt,
  ].join("\n");

  turns.push({ role: "user", text: req.prompt, at: req.at ?? now() });
  await writeSession({ status: "thinking", provider: "cursor", turns, updated_at: now() });

  try {
    const run = await agent.send(prompt);
    let assistant = "";
    for await (const event of run.stream()) {
      if (event.type !== "assistant") continue;
      for (const block of event.message.content ?? []) {
        if (block.type === "text") {
          assistant += block.text;
          await writeSession({
            status: "thinking",
            provider: "cursor",
            turns: [...turns, { role: "assistant", text: assistant, at: now() }],
            updated_at: now(),
          });
        }
      }
    }
    const result = await run.wait();
    if (result.status === "error") {
      throw new Error(`Cursor run failed: ${result.id}`);
    }
    turns.push({ role: "assistant", text: assistant || String(result.result ?? ""), at: now() });
    await writeSession({ status: "idle", provider: "cursor", turns, updated_at: now() });
  } catch (err) {
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
  await fs.writeFile(tmp, JSON.stringify(value, null, 2));
  await fs.rename(tmp, sessionPath);
}

function now() {
  return Math.floor(Date.now() / 1000);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
