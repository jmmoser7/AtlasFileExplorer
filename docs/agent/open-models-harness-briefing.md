# How to harness open models without building an agent runtime

A briefing for Atlas and Slate. Researched 20 September 2026. Text only.

This is a strategy paper, not a task card. It does not authorize implementation. It is written against the constitution as it stands, the agent portal as it shipped, and the 2026 coding-agent landscape as it actually exists.

---

## 1. The question, restated

You already know how the closed side should work. Cursor, Codex, and Claude Code are complete products. The board places a host portal, the human points at material, the sidecar talks to the installed program, and Slate never embeds the runtime. That is the product you approved: icons, a name, a thin chat, wires for context, staged board edits. Point and shoot.

The open side is where the picture goes blurry. You want local models — Ollama-hosted GLM, DeepSeek, and their peers — to be useful as tool-using agents, not as chat boxes. You do not want to invent and maintain a custom agent harness: the loop, the tools, the permissions, the retries, the compaction, the session trees, the XML fallbacks, the model-specific prompts. You asked whether open-source platforms exist that you can integrate, and whether the sidecar strategy is even the right one given that confusion.

The short answer is yes, sidecar is the right strategy, and the confusion is a category error. You have been trying to decide how to integrate a *model*. That is the wrong object. A model is an inference backend. An agent is a *harness* wrapped around a model. Closed products feel easy because they sell you the harness and hide the model. Open weights feel hard because the industry hands you the model and pretends the harness is a weekend script. It is not.

The rest of this paper says what a harness actually is, why local models fail when you skip it, which platforms already maintain one, how those platforms fit this repository, and what you should own versus rent. The recommendation is conservative on purpose. Article III forbids building the general capability. Article I.2 forbids baking a vendor into the document. Article VII.8 already named the extension ladder: declarative assets first, out-of-process servers second, native in-process binaries never.

---

## 2. You already chose correctly

Three decisions in this repository have already answered most of the architecture.

First, the agent portal is a host-class portal. Its frame is authored board data. Its live contents come from a foreign local process. Export is a poster plus a pointer. Board mutations from the agent never apply directly; they become ordinary SceneCmd proposals that the human accepts or rejects as a unit. That is Article V.3 and Article VII.6, and it is already implemented in the file-link contract.

Second, Slate never embeds an agent runtime. It writes `context.json` and `request.json`. A sidecar writes `session.json` and optional staged proposals. Cursor is supervised as a Node process against the Cursor SDK. Codex is supervised as a leaf crate against the installed `codex app-server`. Provider IDs in the scene are names, not SDKs. Protocol adapters are leaf crates. That is Article I.2 as written.

Third, the roadmap already named the durable agent surface: an MCP server over the registered command surface, with the context beacon growing into a two-way channel, and agent work streaming back as journaled, attributed, interruptible actions. Phase 4 is “Speak.” The file contract is documented as the stable local surface that a future `atlas-mcp` wraps rather than replaces.

Those three decisions mean the product question is not “how do we become Claude Code.” It is “which foreign processes are allowed to sit behind the same portal, and what do we owe them.” Closed programs already sit there. Open models do not need a new product. They need a process that can sit in the same chair.

The existing portal-evolution plan sequences a later slice called “Add Ollama.” That slice, as written, discovers a local service, exposes text and vision and tools, and “manages conversational history in the adapter.” That sentence is the trap. Conversational history plus tools plus retries plus cancellation plus malformed-JSON recovery is the first third of a harness. If you build it, you will maintain it. The plan is right that Ollama must not download a model as a side effect of Send, and right that a local-only mode must refuse a silent cloud fallback. It is wrong if it treats Ollama itself as the agent.

Ollama is a model server. ComfyUI is a generation engine. Neither is an agent. Confusing those three categories is how projects accidentally grow a second Cursor.

---

## 3. What an agent harness actually is

A chat box with a function-calling API is not an agent. An agent execution system takes a task and drives a multi-step loop until the task is done, refused, or parked for a human. Every serious 2026 coding agent — Claude Code, Codex, Cursor, Goose, OpenCode, OpenHands, Cline — is this loop plus a pile of unglamorous machinery that the model cannot do for itself.

The loop is the obvious part. The model emits a thought or a plan, then a tool call. The harness executes the tool, or refuses it, and feeds the observation back. Repeat. Two loop shapes dominate. ReAct is light: think, act, observe. Anthropic-style loops spend extra tokens on an explicit reasoning block before acting. ReAct is cheaper per turn. The heavier loop plans better on ambiguous multi-file work. You do not need to pick one. You need to not implement either.

Around the loop sits the work you would actually be on the hook for.

Tool dispatch. Filesystem read and write, targeted edits, shell, grep, browser, LSP diagnostics, git. Some harnesses ship these as built-ins. Goose treats almost everything as an MCP server, including its own built-ins. OpenHands is MCP-friendly but keeps first-party tools. Aider is monolithic and git-shaped. The dispatch layer is where timeouts, hung servers, and schema mismatches live.

Permissions and sandbox. Autonomous agents delete files, run unexpected commands, and push to the wrong branch. The isolation choices are Docker per session, a lighter jail, a native child process with an approval policy, or nothing. Cursor and Codex sell you this as product. OpenHands defaults to Docker. Cline and Continue run inside the IDE with no separate sandbox and make the human approve each destructive step. Your Codex leaf already forces a read-only sandbox and rejects unsupported approval prompts. That is the right instinct for a board sidecar.

Context assembly and compaction. A coding agent burns context. Ollama’s own docs now tell people that Claude Code, OpenCode, and Codex want at least 64k tokens. Default Ollama context is far smaller. A harness that cannot compact, summarize, or drop stale tool output will silently truncate and then look stupid. Pi, a small open harness, keeps fixed prompt overhead under a thousand tokens and sometimes beats a heavier harness on the same model for that reason alone. Harness quality and model quality are not independent.

Recovery. Local models emit almost-JSON, XML tool calls, or prose that pretends to be a tool call. Goose has an XML fallback for Ollama. OpenCode tells you to raise `num_ctx` when tools stop working. vLLM needs explicit tool-call parsers. A harness that assumes every completion is a clean OpenAI tool-call object will stall on the first GLM glitch. This is not a model bug you can fix in Slate. It is why someone else’s parser exists.

Session, subagents, plan mode, undo of file edits, MCP client lifecycle, and model-specific system prompts. Each closed product has a different vocabulary for these. Rig exists specifically because Codex, Claude Code, Kimi Code, and Grok Build do not share tools, permissions, or session models. That unification is a full-time product. It is not a leaf crate.

The honest inventory: if you “just connect Ollama,” you are volunteering to own every paragraph above. That is the custom harness you said you do not want.

---

## 4. Why closed models feel easy and open models feel hard

Closed products collapse three layers into one icon. The human sees Cursor. Inside that icon are a harness, a hosted model, authentication, a permission UI, and a desktop or CLI the user already installed. Your job is process supervision and a file or RPC adapter. You already did that for Cursor and Codex. Claude Code is the same shape: an installed program, a stdio or HTTP control plane, a conversation you do not own.

Open weights explode the same three layers back into sight.

Layer one is inference. Ollama, LM Studio, llama.cpp, vLLM, SGLang. This layer loads weights, serves tokens, and optionally speaks an OpenAI-compatible or Anthropic-compatible HTTP API. It does not have a plan. It does not have a working tree. It does not ask before writing a file, because it cannot write a file.

Layer two is the harness. Something has to turn tokens into tool calls, enforce a cwd, keep a transcript, retry, compact, and stop. That something is Claude Code, Codex, Goose, OpenCode, OpenHands, or a thing you write.

Layer three is the host. That is Slate or File Atlas: canvas as prompt, journal, staging, command parity, the portal chrome.

When you use Codex with ChatGPT, layers one and two are the same vendor. When you use GLM through Ollama, layer one is present and layer two is missing. The missing layer is what people mean by “open models are worse at tools.” Sometimes the model really is worse. Often the model is being asked to be a harness.

Two further facts make the open side feel worse than it is.

First, tool-calling quality is uneven and server-dependent. A model that tools cleanly on vLLM will stall on Ollama, or stall only after five thousand tokens, or emit mismatched XML tags under quantization. GLM-4.7-Flash is a current example: strong agentic scores on paper, and a trail of “generation stops after the first tool call” reports when the serving path is wrong. DeepSeek’s large MoE models are excellent through an API and impractical as a single-workstation Ollama pull. Older coder models can write functions and still fail a multi-step tool loop. Capability discovery — does this installed model actually accept `tools`, and does it still do so at 32k context — is real work. It belongs in the harness or the inference server, not in `board_agent.rs`.

Second, latency changes the product. A cloud turn that takes three seconds can tolerate fifteen tool calls. A local Q4 model on a 4090 can take fifteen to thirty seconds per step. The human must not be blocked. Article VIII already requires this. A local agent that is useful is an asynchronous sidecar with a Stop button and a transcript, not a modal “thinking” overlay. You already have that shape.

The implication is the reverse of the usual instinct. Weaker models need a *better* harness, not a thinner one. A thin “call Ollama, show the string” adapter maximizes the failure mode you are trying to avoid: the model chats instead of acting, or acts once and dies. A mature harness with plan mode, tool repair, and a 64k window is how you get utility out of GLM and Qwen. You will not out-write OpenCode’s or Goose’s Ollama path this year.

---

## 5. Three strategies that do not require a custom harness

There are three honest ways to get local-model agency without becoming the maintainer of an agent runtime. They compose. They are not mutually exclusive.

### Strategy A. Point the harnesses you already integrate at local inference

This is the surprise of 2026, and it is the cheapest path that is still real.

Ollama now speaks enough of the Anthropic Messages API that Claude Code can be launched against a local model (`ANTHROPIC_BASE_URL` at the Ollama port, or `ollama launch claude`). Ollama also launches Codex against local or cloud-local models (`codex --oss`, `ollama launch codex`), and documents a 64k context floor. The same launcher will start OpenCode. The installed program remains the harness. The model becomes a setting.

You already have a Codex leaf. It starts `codex app-server`, owns a portal conversation, forces a read-only sandbox, and refuses to attach to the desktop’s open task. If the installed Codex can be started with an OSS/Ollama profile the way the CLI already can, then “local GLM” is not a new portal kind. It is a connection setting on a program you already discovered. Claude Code is the same adapter-shaped object you have not built yet: an installed CLI, a conversation, a cwd, a permission policy.

The constitutional fit is exact. The document still stores a provider name. The leaf still translates a foreign protocol. The user still installs the program and the model. Slate still does not embed a runtime. You do not maintain tool parsers for GLM.

The limits are real and should be stated. You inherit each vendor’s local-model support, which is newer than their hosted support and will break in vendor-specific ways. Codex-on-Ollama is not the same product as Codex-on-ChatGPT; sandbox and approval knobs may differ; some tools may assume a hosted model. Claude Code pointed at Ollama is unofficial from Anthropic’s point of view even when Ollama documents it. You must not advertise “Claude” when the weights are GLM. The portal should say the program and the model separately: Codex · glm-4.7-flash, or Claude Code · qwen3-coder. Honesty is Article IV applied to a host poster.

This strategy does not help a user who refuses to install Codex or Claude Code. It does help the user you already designed for: someone who has the closed program and also wants it to run locally sometimes.

### Strategy B. Treat one open-source coding agent as another installed program

If the user wants a local-first agent with no Closed-vendor CLI at all, you still should not write the loop. You add another icon to the program grid, the way Codex was added. The candidate must be an installed, out-of-process program with a control plane you can supervise, MCP or equivalent tool use, an Ollama or OpenAI-compatible provider, cancellation, and a license that does not force an account.

The fit test is the same test you used for Codex. Is there a documented app-server, ACP mode, or headless HTTP API. Can you start a portal-owned session that does not hijack the user’s desktop conversation. Can you stream a transcript into `session.json`. Can you keep the working tree read-only or permissioned. Can discovery run off the UI thread and stay quiet when the binary is absent.

Goose, from Block, is the best match on paper. It is a general-purpose on-machine agent with a desktop app, a CLI, and a server. It is written in Rust. It is MCP-first. It has a first-class Ollama provider, including native tool calling and an XML fallback. It speaks ACP for IDE-shaped hosts and a REST API for simpler clients. It supports thirty-odd providers. Apache-licensed. No account of its own. Custom distributions can ship a default of `GOOSE_PROVIDER=ollama`. For a host portal that already believes in out-of-process servers, this is the native-shaped cousin of `atlas-codex`.

OpenCode is the best match if you want the Claude Code experience without Anthropic. It is model-agnostic, has plan mode, permissions, LSP diagnostics, MCP, undo of its own edits, subagents, and `opencode serve` as a headless OpenAPI server. Ollama documents a first-party launch path and warns that tool calls fail when context is short. Adoption is high. The trade is weight: it is a large TypeScript product, not a small Rust crate, and its philosophy is “the harness is a real product.” That is good for users and expensive to wrap if you try to re-skin it. You should not re-skin it. You should start it, talk to its server, and keep your portal small.

OpenHands is the most complete self-hosted software-engineering platform: SDK, server, web UI, Docker sandbox, memory providers. It is the wrong 10% for a board sidecar. It wants to be the environment. You already are the environment.

Aider is a git-aware pair-programming CLI. Excellent at the thing it does. Wrong category. It is not a general tool loop and it is not MCP-first.

Cline and Continue live inside VS Code. They are not programs you host on a canvas. Continue is closer to autocomplete plus chat. Cline is an autonomous in-IDE agent with per-action approval. Both talk to Ollama. Neither should be a Slate portal.

Hermes Agent and OpenClaw are adjacent and easy to over-weight. Hermes, from Nous, is a self-improving agent with skill creation, cross-session memory, and messaging gateways. OpenClaw is a personal-automation runtime that lives in Telegram and cron. Ollama will launch both. They are interesting as user-installed programs and dangerous as defaults. Hermes writes learned skills; Article VII.5 forbids agent-written control flow, and VII.4 still forbids workbook scripts. OpenClaw’s blast radius is a household, not a workbook. If a user already runs them, a later adapter can exist. They should not be the Atlas answer to “local GLM.”

Pi and Rig are harnesses for people who want to own a harness. Pi is small and token-thrifty. Rig unifies several vendor toolsets on top of Pi. Integrating them means you become a harness customer who still thinks like a harness author. Skip them unless you later decide to maintain an agent product.

LangGraph, AutoGen, CrewAI, PydanticAI, and smolagents are libraries. A library is how you *start* a custom harness. They are the opposite of the constraint you stated.

### Strategy C. Stop adding programs and become the command surface

This is the Phase 4 strategy, and it is the one that ages.

MCP is the agent-to-tools protocol. An MCP server exposes tools. Any MCP client — Cursor, Claude Code, Codex, Goose, OpenCode, Cline — can call them. Your constitution already says the MCP surface exposes the same commands as the human interface. That is command parity. When `atlas-mcp` exists, a local Goose session with an Ollama model can move a frame, stage a proposal, or read the current selection without a Slate-specific Goose adapter. The adapter work you do today is a courtesy. The MCP work is the hedge.

ACP, the Agent Client Protocol, is the other half, and it is newer in this repository’s thinking. ACP is editor-to-agent. Zed and JetBrains speak it as hosts. Goose, OpenCode, Claude Code, Codex, and Gemini CLI speak it as agents. It is JSON-RPC over stdio. It has session new/prompt/cancel, streaming tool updates, and interactive permission requests. Goose’s own docs say use ACP when the host is a desktop application that needs rich tool feedback and approvals; use REST when the client is simple.

Slate is much closer to Zed than it is to a website. A future Atlas ACP client would let the portal be a generic host for any installed agent that speaks the standard, the same way the program grid is a generic host for any file-link sidecar. That is the provider hedge applied to control planes. You should not implement ACP tomorrow. You should not invent a fourth control plane that looks like ACP either.

The file-link contract remains useful. It is pollable, durable across process death, and already understood by the Cursor sidecar. The contract itself says a future MCP server wraps the same operations. Keep it. Do not grow it into a tool loop.

Strategy C is how you avoid a linear tax: one leaf crate per program forever. Leaves are allowed. They should stay thin. The durable surface is commands, context, and stage.

---

## 6. What the open platforms are actually good for

A survey is only useful if it is filtered through this product. The product is a Windows-first local canvas. Agents are host portals. The human is never blocked. Mutations stage. No account may be required. Adapters are out of process. The 10% is point-and-shoot over a real program, not an embedded IDE.

Goose. Best open local-first general agent to treat as an installed program. MCP-first means your future command server plugs in as an extension rather than a fork. Rust is culturally aligned with this workspace, but that is not a reason to vendor Goose or to call its crates from in-process code. Article VII.8 forbids native in-process binary extensions. Supervising `goosed` or `goose acp` is the legal shape. Weakness: you will be an ACP or REST client, and permission prompts have to land in the portal or be pre-answered by a read-only policy, the way Codex is already pinned.

OpenCode. Best open Claude-shaped harness, with a real headless server and first-party Ollama docs. Weakness: large surface, TypeScript, easy to accidentally clone its UI. If you can see yourself rebuilding plan mode inside the portal, do not pick OpenCode. If you can treat `opencode serve` the way you treat `codex app-server`, it is a strong second icon.

OpenHands. Best if you wanted a self-hosted software-engineering department. Docker, browsers, memory backends, SWE-bench recipes. Wrong host. A board portal that launched OpenHands would be launching a second universe, which Article V forbids in spirit even if the process is foreign.

Continue and Cline. Best if the user already lives in VS Code. They will. That is not your job. Do not host them. Do not compete with them. Let those users open the same workspace folder you already write into `.atlas-ai`.

Aider. Best surgical git editor. If a user wants “apply this diff and commit,” they can run it. It should not be a default Atlas program.

Claude Code and Codex, reused. Best path for users who already have them. Best path for you, because the leaves are known. Local models become a profile, not a product.

Cursor. Already done. Cursor can also be pointed at other models inside its own product. You should not re-implement that. Open in program remains the escape hatch.

The platforms you should not integrate as strategy: anything whose value is a multi-agent orchestra, a skill marketplace that writes code, a messaging gateway, or a memory system that the human cannot read and delete. Article VII.5 requires pinned versus learned memory, all of it human-readable and deletable. Article VII.3 says agents extend the workspace with data, not code. A sidecar that writes skills into a hidden folder is a constitutional problem even if it is useful.

One more non-platform belongs here because people reach for it: “we will speak raw Ollama `/api/chat` with tools.” That is Strategy None. It looks like four hundred lines and becomes four thousand. Every XML fallback you skip becomes a support thread. Every context-window default you forget becomes a user who says local models are dumb. Leave it.

---

## 7. Local models: what is actually useful in 2026

Once the harness is someone else’s, the model question gets smaller, which is the point.

Inference servers. Ollama is the right default for a single Windows workstation. It is what your users will have. LM Studio is the nicer desktop for people who want a GUI and a headless daemon. llama.cpp is the raw server. vLLM and SGLang are what you use when a model’s tool parser is broken in Ollama and you have a real GPU box. The portal should not pick among these. The harness should. Your discovery code should ask “is a known program installed” and “does the user’s chosen program report a reachable model,” not “what is on port 11434.”

Context. This is the silent killer. Agentic loops consume tens of thousands of tokens. Ollama’s defaults will truncate. Every serious local-agent writeup in 2026 repeats this, because every integrator ships the bug once. If you do anything Ollama-specific at all, the only honest product rule is: refuse to start an agentic session below a named context floor, and say so. Do not clamp. Do not silently continue. Article IV: honest models.

Which weights. The useful local coding agents in the current window are not “whatever GGUF is popular.” They are models trained or post-trained for tool use, in the 14B to 35B class for a single serious GPU, or larger MoE with few active parameters. Qwen’s coder and 3.6 lines keep showing up as the reliable local tool-users. GLM-4.7-Flash is the interesting 30B-class MoE: strong SWE-bench and tau-bench numbers, officially launched into Claude Code and OpenCode via Ollama, and still sensitive to serving path and quantization. gpt-oss 20B is the model Ollama itself demonstrates with Claude Code. Devstral-class models exist specifically for OpenHands-like loops.

DeepSeek needs a sober paragraph. The API-scale DeepSeek V3 family is a frontier-class coder. It is not a casual Ollama model. Self-hosting the full MoE is a multi-GPU affair. The older dense `deepseek-coder` 33B-class weights can generate code well and still collapse on multi-step tool loops; at least one 2026 local eval had it win code generation and fail agent accuracy. If a user says they want “DeepSeek locally,” ask which: the hosted API, a distilled coder, or the full V3. Treat those as different programs. Do not promise V3 behavior from a 7B pull.

Vision is a separate capability bit. An Ollama tag that chats cannot see a Rhino view capture. The evolution plan already says this. Keep it. A harness that supports image inputs will either accept the wired image or refuse. Slate must not flatten an image into a caption and pretend the model saw it.

What local models are good for on a board, given a real harness. Narrow, grounded tasks over material you already selected: arrange these screenshots, draft alt text, propose a frame title, summarize this folder, extract a list of drawing numbers from a PDF excerpt, write a kit recipe as data, classify files into existing tags. They are worse at long autonomous refactors, worse at inventing a graphic system, and worse at anything that needs a secret the model should not see. That last point is already law: write-back to a source is never available to an agent, and cloud placeholders must not be hydrated to give an agent bytes.

What they are not good for. Replacing Cursor on a hard multi-file software task. Running unattended overnight against a live workbook. Holding months of memory inside the weights. Memory belongs in the document, pinned versus learned, as the earlier audit said and as Article VII.5 now requires.

---

## 8. The constitutional reading, with pushback

Article I.2: no document model, command, or memory record may depend on a provider, vendor application, or protocol revision. Protocol adapters are leaf crates. Consequence: `AgentPortalRef` keeps storing a name. `atlas-codex` is the pattern. An `atlas-ollama` crate that owns a ReAct loop would be a violation of the spirit even if it compiled. An `atlas-goose` crate that only starts and translates would be conformant.

Article I.4: no capability may require an account with anyone, including this project. Local models are how you keep that promise for users who will not sign into ChatGPT or Cursor. That is a reason to support them. It is not a reason to build a harness. Goose and OpenCode satisfy the clause. A Slate-mediated OpenAI key does not.

Article III: implement the fraction the user reaches for. The named use is “point this portal at a real program and send the canvas.” The user does not reach for “Slate’s own tool-calling runtime.” If they did, this paper would be a different paper and would require an amendment conversation.

Article V.3: host portals own no mutations and export as a poster plus a pointer. A local agent that writes SceneCmds itself is not a host; it is a bug. Staging remains the seam.

Article VI: every mutation is a named, invertible, authored command. The author of an accepted proposal is the named agent, not “local.” If the harness is Goose running GLM, the author string should say so. Undo must still restore the pre-accept state. You already have this for Cursor.

Article VII.1: MCP exposes the same commands. This is the strategy-C obligation. Until it exists, every new sidecar re-teaches the agent what Slate can do through prompt text. Prompt text is not a command surface.

Article VII.3 and VII.7: data, not code; skills are recipes of registered commands with no user-authored control flow. A sidecar may write a dashboard scene or a `.slatekit` recipe. It may not install a Python tool into the Slate process. It may not require the script amendment.

Article VII.8: out-of-process servers, OS-sandboxed, user-permitted. This is the entire recommendation in one clause. Goose, OpenCode, Claude Code, Codex, and a future ComfyUI server are this clause. An in-process `llama-cpp-rs` bound to the frame loop would violate Articles I, II, and VII at once.

Article VIII: the canvas is the prompt; the human is never blocked. Local models make this sharper. A thirty-second tool step is fine if the board still pans at 60 fps. It is not fine if `agent_pump` reads a huge session file on the UI thread. You already moved I/O to a worker. Keep that discipline as transcripts grow. A 16 MB session cap already exists for a reason.

Article IX.5: write-back is never available to an agent. A local harness with a shell tool can violate this in the filesystem even if it cannot violate it in the journal. The Codex leaf’s read-only sandbox is the pattern to copy. “The model is local, so it is safe” is false. Local plus shell is a full user-level attacker on the working tree.

Article XI: if a request conflicts, name the article. The request “maximize open-model tool use by building our own harness” conflicts with III, I.2, and VII.8. The conforming alternative is: rent a harness, expose commands, keep staging. The amendment path would be: Slate becomes an agent runtime. That would be a different program. You have not asked for that amendment, and this paper does not propose one.

Article XII: one owner. The owner of chrome is atlas-shell. The owner of the file-link types is atlas-agent. The owner of Codex protocol is atlas-codex. The owner of folder maps is folder_map. There must not be a `board_ollama.rs`. There must not be a second chat painter. There must not be a Slate-specific copy of Goose’s tool loop.

---

## 9. Recommended approach

Own the host. Rent the loop. Let the user own inference.

Concretely, Atlas and Slate should treat “agent” as one product surface with three interchangeable backends: a closed installed program, an open installed program, and, later, any program that speaks ACP while Slate speaks MCP. The portal UI does not change. The chooser gains icons only when a control plane exists and can be discovered. Unavailable programs stay quiet. Advanced model lists stay in the real program or in a portal-local inspector, never in a board-wide settings panel. That last sentence is already D35.

What you own, and should keep investing in.

The command registry and its parity with any future MCP server. This is the only way open models become first-class on the *board*, as opposed to first-class on a folder of files. A Goose-plus-Ollama agent that cannot stage a SceneCmd is just a coding agent sitting next to Slate. A Goose-plus-Ollama agent that can call `board.selection.nudge` through MCP is your product.

The context beacon and the canvas-as-prompt extraction. Selection, viewport, frame membership, wired text, wired images, intent ink. This is the work only you can do. No open harness knows what a Slate frame is. Every hour spent making context honest returns more local-model utility than an hour spent parsing tool JSON.

The staging layer, authorship, and the file-link contract. These are how you stay reversible and how you survive vendor churn.

The leaf-adapter pattern. Thin, renderer-free, off-thread, capability-driven. Codex is the template. Claude Code would be a sibling. Goose or OpenCode would be a sibling. Ollama is not a sibling.

What you rent.

The loop, tools, permissions, compaction, model catalogs, and Ollama quirks. Prefer harnesses that already advertise Ollama or OpenAI-compatible endpoints. Prefer ones with a headless control plane. Prefer MCP-first, so your command server is an extension rather than a patch.

What the user owns.

Weights, GPUs, `ollama pull`, LM Studio libraries, whether the machine is allowed to talk to the network. You may discover and refuse. You may not download. You may not silently route a “local” portal to a hosted API. A local-only badge that lies is an Article IV defect.

Sequence, in the order that spends the least of your attention for the most utility.

First, prove Strategy A on the Codex leaf you already have. Can a portal-owned `codex app-server` conversation run against an Ollama profile without loosening the read-only sandbox. If yes, local open models are a settings path for a program that is already in the grid. That is the highest-leverage experiment in this paper. It might fail for protocol reasons. Failure is cheap and informative. Success means GLM users who already installed Codex never need a fourth icon.

Second, decide whether Claude Code is worth a sibling leaf. The control-plane work is similar to Codex. The user already named it. Ollama can feed it. Do not do this in the same slice as a Goose integration. One leaf at a time.

Third, add at most one open harness as an installed program, and only if Strategy A leaves a real gap: users with no Closed CLI who still want tools. The default recommendation is Goose, because of MCP-first design, ACP, Ollama, Rust, and the absence of an account. OpenCode is the alternative if you want plan mode and a Claude-shaped permission model more than you want MCP-as-the-spine. Do not add both in the same phase. Two open harnesses is not 10%. It is a catalog.

Fourth, build `atlas-mcp` as Roadmap Phase 4 already describes. This is the strategy that makes the previous icons less important. It is also how File Atlas and Slate stay one surface for agents. A local model tagging a folder and a local model arranging a board should call different tools on the same server, not two prompt dialects.

Fifth, keep ComfyUI on the image-engine path, not the agent path. Generation is a workflow server. It is not a tool loop. The image sidecar contract you already wrote is the right size.

Sixth, only after MCP exists, consider speaking ACP as a host. That is how the program grid becomes a protocol grid. It is also how you stop writing leaves. Until MCP exists, ACP would let a foreign agent act on files but not honestly on the board.

What the “Add Ollama” slice should be rewritten to mean.

It should not mean “Slate chats with Ollama.” It may mean “discover whether the selected harness has a reachable local model, show that capability, and refuse image inputs the model cannot see.” It may mean a *non-agentic* text endpoint for tiny jobs that do not need tools — rewrite this sticky, translate this label — if you ever have evidence that users reach for that weekly. That is a different, smaller 10%, and it still belongs in a leaf, not in the scene. It should not be sold as the way to harness GLM.

---

## 10. Risks, anti-patterns, and the decision

The failure mode you are trying to avoid has a name. It is called becoming the second interpreter of a moving protocol. You already refused to do that with renderers and with SVG. Do it again with agents.

Anti-pattern one: embed the model. Linking llama.cpp or an Ollama crate into the app couples the frame loop to inference, destroys the provider hedge, and makes every weight upgrade your release. Forbidden in substance by I, II, and VII.8.

Anti-pattern two: embed the harness. Copying Goose’s loop, or “just the tools we need,” produces a permanent fork. You will lag on GLM parser bugs, on MCP revisions, and on permission models. Article XII: do not emulate a named owner. The named owner of an agent loop is an agent product.

Anti-pattern three: one leaf per model. `atlas-glm`, `atlas-deepseek`, `atlas-qwen` are not adapters. They are a catalog of weights. Adapters attach to programs and protocols.

Anti-pattern four: prompt-shaped command parity. A system prompt that lists board operations is not MCP. Local models follow instructions less reliably than Claude. The worse the model, the more you need a real tool schema, which is another way of saying the worse the model, the more you need Phase 4.

Anti-pattern five: memory in the provider. Ollama has no durable memory you should trust. Goose and Hermes will happily grow their own. Your law says memory is pinned or learned, journaled or at least human-readable and deletable, and provider-agnostic. If a sidecar keeps memory, the portal must be able to show and clear it, or you must not send it.

Anti-pattern six: autonomous write on a local model because “it is only my machine.” Local plus shell can wreck a working tree as thoroughly as a hosted agent. Staging, read-only sandboxes, and IX.5 do not relax for open weights.

Anti-pattern seven: cloning chrome. A plan-mode timeline, a permission modal, a token meter, a model library — those are the foreign program’s UI. Your portal is a sidecar. Open in program exists. Article X and the host-portal contract both say this.

Risks that remain even if you do it right.

Vendor control planes move. Codex app-server, Claude’s CLI flags, Goose ACP, OpenCode’s OpenAPI, MCP itself — all revisioned. Leaves will break. That is why they are leaves. Budget for it. Do not spread protocol types into `slate-doc`.

Local quality is hardware-honest. A user on a 16 GB laptop will not get Codex-like agency from a 7B model no matter which harness you rent. The honest UI is a capability badge and a refusal, not a spinner that produces a paragraph.

Legal and account residue. Pointing Claude Code at Ollama may still require a Claude install that expects a login. Codex OSS still wants the Codex CLI. Goose does not. If “no account” is the actual requirement for a given user, Goose or OpenCode is the path, not Strategy A.

Split brain. The user already has Cursor open on the same folder the portal is using. You already decided not to attach to the desktop task. Keep that. Two harnesses on one tree need read-only defaults or they will fight.

What I would decide if this were my product.

Sidecar is the strategy. It is already the law and already the UI. Do not build a custom harness. Do not make Ollama an agent. Use the next implementation energy on, in order: Codex-against-Ollama as an experiment on the existing leaf; the MCP command server that makes every harness smarter on the board; one open program icon only if the experiment leaves a user who cannot install Codex or Claude Code. Keep ComfyUI in the image bucket. Keep memory in the document. Keep authors on the journal. Keep the portal boring.

That is how you maximize the utility of GLM and DeepSeek without becoming the maintainer of their tool loop. The models will change again this winter. A good host will not have to.

---

## Sources and scope notes

This briefing is grounded in the repository as of 20 September 2026: `CONSTITUTION.md` Articles I, III, V–IX, XI, XII; `ROADMAP.md` Phase 4; `docs/agent-link-contract.md`; `docs/keymap/contracts/portal-agent-link.md`; `docs/agent/portal-evolution-plan.md`; `crates/atlas-ai` supervision; `crates/atlas-codex` app-server leaf.

External landscape, same date: Ollama’s Claude Code, Codex, OpenCode, and Hermes launch docs; Ollama Anthropic-compatibility notes and 64k context guidance; OpenCode server and provider docs; Goose architecture, Ollama provider, ACP, and custom-distribution docs; Agent Client Protocol v1 (Zed / JetBrains / `agentclientprotocol`); OpenHands local-LLM docs; comparative notes on OpenCode vs Pi, Cline vs Continue vs Aider, and local tool-calling evals for Qwen, GLM-4.7-Flash, and DeepSeek-class weights. Specific benchmark numbers will move. The layering — host, harness, inference — will not.

If this paper conflicts with a later ratified amendment, the constitution wins. If a future task card implements an Ollama chat leaf, it should cite a weekly user reach and a non-agentic scope, or it should be rewritten as a harness adapter.
