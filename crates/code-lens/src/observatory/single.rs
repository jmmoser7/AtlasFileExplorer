//! Source-only analysis for immutable historical blobs. No filesystem or Git
//! access, cross-file resolver, build, macro expansion or checkout is required.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use super::{finish_weights, stable_id, symbols, Builder, Facts, SourceInfo};
use crate::extract::{rust_src, GraphBuilder};
use crate::model::{LensError, NodeKind};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleSymbol {
    pub id: String,
    pub parent: Option<String>,
    pub kind: &'static str,
    pub name: String,
    pub line: u32,
    pub end_line: u32,
    pub loc: u32,
    pub self_loc: u32,
}

pub struct SingleSource {
    pub available: bool,
    pub error: Option<String>,
    pub file_id: String,
    pub file_self_loc: u32,
    pub symbols: Vec<SingleSymbol>,
    /// Retained so a caller can run its canonical AST metrics without parsing
    /// again. The AST and source text are never part of the serialized output.
    pub syntax: Option<syn::File>,
}

/// Parse one source string using the same harvester and identities as the
/// workspace graph. A symbol whose parent is the physical file has parent=None.
pub fn analyze_source(path: &Path, text: &str) -> Result<SingleSource, LensError> {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut graph = GraphBuilder::new();
    let lines = text.lines().count() as u32;
    let file = graph.push_node(
        None,
        NodeKind::File,
        name.clone(),
        path.to_path_buf(),
        lines,
    );
    let mut info = SourceInfo::physical("file", path, &name);
    info.line = u32::from(lines > 0);
    info.end_line = lines;
    let file_id = stable_id(&info.identity);
    let mut builder = Builder {
        graph,
        source: vec![info],
        facts: Facts::default(),
        duplicates: HashMap::new(),
    };
    builder.facts.file_for.insert(file, file);
    let (syntax, error) = match rust_src::parse_file_with_error(text) {
        Ok(syntax) => {
            symbols::extract(&mut builder, file, path, &syntax);
            (Some(syntax), None)
        }
        Err(error) => (
            None,
            Some(format!(
                "Rust parse failed at line {}: {error}",
                error.span().start().line
            )),
        ),
    };
    let mut graph = builder.graph.finish_without_rollup(file, 0)?;
    finish_weights(&mut graph, &mut builder.source);
    let ids: Vec<_> = builder
        .source
        .iter()
        .map(|n| stable_id(&n.identity))
        .collect();
    let symbols = graph
        .nodes
        .iter()
        .filter(|n| n.id != file)
        .map(|node| {
            let info = &builder.source[node.id as usize];
            SingleSymbol {
                id: ids[node.id as usize].clone(),
                parent: node
                    .parent
                    .filter(|p| *p != file)
                    .map(|p| ids[p as usize].clone()),
                kind: info.kind,
                name: node.name.clone(),
                line: info.line,
                end_line: info.end_line,
                loc: node.loc,
                self_loc: info.self_loc,
            }
        })
        .collect();
    Ok(SingleSource {
        available: syntax.is_some(),
        error,
        file_id,
        file_self_loc: builder.source[file as usize].self_loc,
        symbols,
        syntax,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observatory::analyze_observatory;

    #[test]
    fn single_file_ids_and_spans_match_workspace_extraction() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/observatory-ws");
        let path = Path::new("core/src/lib.rs");
        let text = std::fs::read_to_string(root.join(path)).unwrap();
        let single = analyze_source(path, &text).unwrap();
        let full = analyze_observatory(&root, &["fixture-core".into()]).unwrap();
        let file = full
            .graph
            .nodes
            .iter()
            .find(|n| n.path == path && full.source[n.id as usize].kind == "file")
            .unwrap();
        assert_eq!(
            single.file_id,
            stable_id(&full.source[file.id as usize].identity)
        );
        let expected: Vec<_> = full
            .graph
            .nodes
            .iter()
            .filter(|n| n.path == path && n.id != file.id)
            .map(|n| {
                let info = &full.source[n.id as usize];
                (
                    stable_id(&info.identity),
                    info.line,
                    info.end_line,
                    n.loc,
                    info.self_loc,
                )
            })
            .collect();
        let actual: Vec<_> = single
            .symbols
            .iter()
            .map(|n| (n.id.clone(), n.line, n.end_line, n.loc, n.self_loc))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn malformed_source_retains_file_identity_without_fake_symbols() {
        let parsed = analyze_source(Path::new("src/broken.rs"), "fn broken(\n").unwrap();
        assert!(!parsed.available);
        assert!(parsed.syntax.is_none() && parsed.symbols.is_empty());
        assert_eq!(parsed.file_self_loc, 1);
        assert!(parsed.error.unwrap().contains("Rust parse failed"));
    }

    #[test]
    fn parent_links_are_local_and_line_shifts_preserve_identity() {
        let a = analyze_source(Path::new("src/lib.rs"), "fn outer() { fn inner() {} }\n").unwrap();
        let b = analyze_source(
            Path::new("src/lib.rs"),
            "\n\nfn outer() {\nfn inner() {}\n}\n",
        )
        .unwrap();
        assert_eq!(
            a.symbols
                .iter()
                .map(|s| (&s.id, &s.parent))
                .collect::<Vec<_>>(),
            b.symbols
                .iter()
                .map(|s| (&s.id, &s.parent))
                .collect::<Vec<_>>()
        );
        assert!(a.symbols[0].parent.is_none());
        assert_eq!(a.symbols[1].parent.as_ref(), Some(&a.symbols[0].id));
    }
}
