//! Shared model choice, key editing, and seed helpers for flow editors.
use super::AgentDraft;

/// Where `choice` sits in a model list: the exact entry, else that provider's
/// default, else the first (local) entry.
pub(super) fn model_index(
    models: &[(String, String, String)],
    choice: Option<&(String, String)>,
) -> usize {
    let Some((provider, model)) = choice else {
        return 0;
    };
    models
        .iter()
        .position(|m| &m.0 == provider && &m.1 == model)
        .or_else(|| {
            models
                .iter()
                .position(|m| &m.0 == provider && m.1.is_empty())
        })
        .or_else(|| models.iter().position(|m| &m.0 == provider))
        .unwrap_or(0)
}

/// The key field shows one bullet per key character: the bullets left are
/// the characters kept, anything else was typed or pasted.
pub(super) fn absorb_key(draft: &mut AgentDraft, shown: &str) {
    let kept = shown.chars().filter(|c| *c == '•').count();
    let typed: String = shown.chars().filter(|c| *c != '•').collect();
    let key = draft.key.get_or_insert_with(String::new);
    *key = key.chars().take(kept).chain(typed.chars()).collect();
}

/// A seed to lock, from a fresh request id.
pub(super) fn fresh_seed() -> u64 {
    let id = atlas_ai::agent::request_id();
    id.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    }) >> 1
}
