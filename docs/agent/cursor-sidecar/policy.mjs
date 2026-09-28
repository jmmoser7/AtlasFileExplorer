// Read-only is provider policy: the tool allowlist, the sandbox, and plan
// mode. It is never a persona added to the prompt.
// TWIN: crates/atlas-ai/src/sidecar.rs access_env sets ATLAS_AGENT_READ_ONLY.

// No edit, shell, delete, task (subagents), mcp, applyAgentDiff, or generateImage.
export const READ_ONLY_TOOLS = Object.freeze(['read', 'grep', 'glob', 'ls', 'semSearch', 'readLints']);

export const REFUSAL = 'This Cursor sidecar was started with write tools; Slate restarts it read-only. Send again.';

function readOnly(env) {
  return env?.ATLAS_AGENT_READ_ONLY === '1';
}

// Extra Agent.create / Agent.resume options. The SDK does not persist `tools`,
// so they are passed on every resume.
export function agentPolicyOptions(env) {
  if (!readOnly(env)) return {};
  return { tools: [...READ_ONLY_TOOLS], local: { sandboxOptions: { enabled: true }, autoReview: true } };
}

// `options` with the policy applied; policy wins, and `local` merges key by key.
export function withPolicy(options, env) {
  const extra = agentPolicyOptions(env);
  const merged = { ...options, ...extra };
  if (options?.local || extra.local) merged.local = { ...options?.local, ...extra.local };
  return merged;
}

export function sendMode(req, env) {
  return readOnly(env) ? 'plan' : undefined;
}

// Fails closed: a read-only turn never runs on a sidecar that holds write tools.
export function refusal(req, env) {
  return req?.policy === 'read_only' && !readOnly(env) ? REFUSAL : null;
}
