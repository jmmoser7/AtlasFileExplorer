//! Explicit UTF-8 byte pages of authored words. Never opens a linked source.
use super::*;

pub(super) struct TextPage {
    node: slate_doc::NodeId,
    cursor: usize,
    limit: usize,
    generation: u64,
}
impl TextPage {
    pub(super) fn parse(call: &BoardCall, generation: u64) -> Result<Option<Self>, String> {
        let BoardOperation::Invoke {
            command,
            targets,
            arguments,
        } = &call.operation
        else {
            return Ok(None);
        };
        if command != "presence.read_text" {
            return Ok(None);
        }
        let object = arguments
            .as_object()
            .ok_or("Text page arguments must be an object")?;
        if targets.len() != 1
            || object
                .keys()
                .any(|key| !matches!(key.as_str(), "cursor" | "limit" | "generation"))
        {
            return Err("Use one explicit node and only cursor, limit and generation".into());
        }
        let integer = |key: &str, default: u64| -> Result<u64, String> {
            arguments.get(key).map_or(Ok(default), |value| {
                value
                    .as_u64()
                    .ok_or_else(|| format!("{key} must be an unsigned integer"))
            })
        };
        let cursor =
            usize::try_from(integer("cursor", 0)?).map_err(|_| "Cursor is out of range")?;
        let limit =
            usize::try_from(integer("limit", 4096)?).map_err(|_| "Limit is out of range")?;
        if !(4..=4096).contains(&limit) {
            return Err("Text page limit must be 4–4096 bytes".into());
        }
        if (cursor > 0 && !object.contains_key("generation"))
            || integer("generation", generation)? != generation
        {
            return Err(
                "Board changed or continuation generation is missing; restart at cursor zero"
                    .into(),
            );
        }
        let node = targets[0]
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .map(slate_doc::NodeId)
            .ok_or("Use one positive stable node ID")?;
        Ok(Some(Self {
            node,
            cursor,
            limit,
            generation,
        }))
    }
    fn body<'a>(&self, app: &'a SlateApp) -> Result<&'a str, String> {
        if app.doc().scene.scene_gen() != self.generation {
            return Err("Board changed during this text page; restart at cursor zero".into());
        }
        let node = app
            .doc()
            .scene
            .node(self.node)
            .ok_or("Text node no longer exists")?;
        let text = slate_doc::authored_text::text(node)
            .ok_or("This node has no authored text; linked source text is outside this reader")?;
        if self.cursor > text.len() || !text.is_char_boundary(self.cursor) {
            return Err("Text cursor must be an in-range UTF-8 byte boundary".into());
        }
        Ok(text)
    }
    pub(super) fn validate(&self, app: &SlateApp) -> Result<(), String> {
        self.body(app).map(|_| ())
    }
    pub(super) fn value(&self, app: &SlateApp) -> Result<Value, String> {
        let text = self.body(app)?;
        let mut end = self.cursor.saturating_add(self.limit).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(
            json!({"node_id":self.node.0.to_string(), "text":&text[self.cursor..end],
            "cursor":self.cursor, "next_cursor":(end < text.len()).then_some(end),
            "total_bytes":text.len(), "generation":self.generation, "authored_only":true}),
        )
    }
}
