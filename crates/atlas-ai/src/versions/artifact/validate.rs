//! The first format is one offline HTML entry with embedded data/assets.
use super::paths;
use atlas_agent::ArtifactProvenance;
use std::{io, path::Path};

pub(super) fn provenance(p: &ArtifactProvenance) -> io::Result<()> {
    let checkpoint = &p.checkpoint;
    if p.provider.trim().is_empty()
        || p.model.trim().is_empty()
        || checkpoint.conversation.trim().is_empty()
        || checkpoint.through != checkpoint.turns.len()
        || checkpoint.through == 0
        || checkpoint.through > 4096
        || serde_json::to_vec(p)?.len() > 2 * 1024 * 1024
    {
        return Err(paths::bad(
            "Artifact provenance has an incomplete or oversized exclusive checkpoint",
        ));
    }
    for input in &p.inputs {
        if input.source.is_empty()
            || input.source_hash.len() != 64
            || !input.source_hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(paths::bad(
                "Artifact input provenance is missing its exact source hash",
            ));
        }
    }
    Ok(())
}

pub(super) fn output_directory(dir: &Path) -> io::Result<()> {
    let mut entries = std::fs::read_dir(dir)?;
    let entry = entries
        .next()
        .transpose()?
        .ok_or_else(|| paths::bad("Artifact output is empty"))?;
    if entry.file_name() != super::ARTIFACT_ENTRY || entries.next().is_some() {
        return Err(paths::bad(
            "Artifact output must contain only self-contained index.html",
        ));
    }
    paths::ordinary(&entry.path())
}

/// This is a narrow format validator, not a JavaScript security sandbox. The
/// browser owner supplies that boundary; HTML is never executed during validation.
pub(super) fn html(source: &[u8]) -> io::Result<()> {
    let text = std::str::from_utf8(source).map_err(|_| paths::bad("Artifact HTML is not UTF-8"))?;
    let lower = text.to_ascii_lowercase();
    if source.is_empty()
        || text.contains('\0')
        || !lower.contains("<html")
        || !lower.contains("</html>")
    {
        return Err(paths::bad(
            "Artifact output is not a complete HTML document",
        ));
    }
    // Dependencies must be embedded. Reject ambient filesystem/network resource
    // locators and network loaders; page scripts still run only in WebHost.
    for forbidden in [
        "<base",
        "<iframe",
        "<object",
        "<embed",
        "@import",
        "fetch(",
        "fetch (",
        "xmlhttprequest",
        "websocket",
        "import(",
        "import (",
    ] {
        if lower.contains(forbidden) {
            return Err(paths::bad(
                "Artifact output is not self-contained offline HTML",
            ));
        }
    }
    attributes(&lower)?;
    for (start, _) in lower.match_indices("url(") {
        // CSS's function token starts at an identifier boundary. JavaScript
        // createObjectURL/revokeObjectURL contain this suffix but create and
        // release local Blob URLs; they are not CSS resource dependencies.
        if start > 0 {
            let previous = lower.as_bytes()[start - 1];
            if previous.is_ascii_alphanumeric()
                || matches!(previous, b'_' | b'-')
                || !previous.is_ascii()
            {
                continue;
            }
        }
        let value = lower[start + 4..].trim_start();
        let value = value.trim_start_matches(['\'', '"']);
        if !value.starts_with("data:") && !value.starts_with('#') {
            return Err(paths::bad("Artifact CSS depends on an external asset"));
        }
    }
    Ok(())
}

fn attributes(text: &str) -> io::Result<()> {
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find('<') {
        let start = cursor + relative;
        let Some(end) = text[start..].find('>') else {
            return Err(paths::bad("Artifact HTML has an incomplete tag"));
        };
        let tag = &text[start..start + end];
        for attribute in ["src", "href", "poster", "action", "srcset"] {
            for (offset, _) in tag.match_indices(attribute) {
                if offset > 0 && !tag.as_bytes()[offset - 1].is_ascii_whitespace() {
                    continue;
                }
                let value = tag[offset + attribute.len()..].trim_start();
                let Some(value) = value.strip_prefix('=') else {
                    continue;
                };
                let value = value.trim_start();
                let value = match value.chars().next() {
                    Some(quote @ ('\'' | '"')) => {
                        value[1..].split(quote).next().unwrap_or_default()
                    }
                    _ => value.split_ascii_whitespace().next().unwrap_or_default(),
                };
                if (attribute == "srcset" && !value.is_empty())
                    || (!value.starts_with("data:") && !value.starts_with('#') && !value.is_empty())
                {
                    return Err(paths::bad(
                        "Artifact resource attributes must be embedded data or local fragments",
                    ));
                }
            }
        }
        cursor = start + end + 1;
    }
    Ok(())
}
