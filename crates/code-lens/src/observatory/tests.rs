use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::*;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/observatory-ws")
}

fn analyze() -> ObservatoryGraph {
    analyze_observatory(&fixture(), &["fixture-core".into()]).unwrap()
}

fn named(result: &ObservatoryGraph, name: &str, kind: &str) -> NodeId {
    result
        .graph
        .nodes
        .iter()
        .find(|node| node.name == name && result.source[node.id as usize].kind == kind)
        .unwrap()
        .id
}

#[test]
fn complete_symbols_have_real_spans_and_container_weights_do_not_double_count() {
    let result = analyze();
    let start = named(&result, "start", "method");
    let info = &result.source[start as usize];
    assert!(info.line > 0 && info.end_line >= info.line);
    assert_eq!(info.doc, "Start then finish.");
    let nested = named(&result, "nested_function", "function");
    assert_eq!(
        result
            .graph
            .node(result.graph.node(nested).parent.unwrap())
            .name,
        "outer"
    );
    assert_eq!(result.counts.files, 3);
    assert_eq!(result.counts.parsed_files, 3);
    assert!(result.failures.is_empty());
    let files: u32 = result
        .graph
        .nodes
        .iter()
        .filter(|n| result.source[n.id as usize].kind == "file")
        .map(|n| n.loc)
        .sum();
    assert_eq!(result.graph.node(result.graph.root).loc, files);
    let mut identities = HashSet::new();
    for info in &result.source {
        assert!(identities.insert(stable_id(&info.identity)));
    }
}

#[test]
fn strict_links_resolve_aliases_self_and_test_targets_but_not_shadowing_or_ambiguity() {
    let result = analyze();
    let start = named(&result, "start", "method");
    let finish = named(&result, "finish", "method");
    let run = named(&result, "run", "function");
    assert!(result
        .relations
        .iter()
        .any(|r| r.from == start && r.to == finish && r.kind == "call" && r.weight == 2));
    assert!(result
        .relations
        .iter()
        .any(|r| r.from == start && r.to == run && r.kind == "call"));
    let piece = named(&result, "from_piece", "method");
    assert!(result
        .relations
        .iter()
        .any(|r| r.from == piece && r.to == finish && r.kind == "call"));
    for name in ["shadow", "variable_shadow", "ambiguous"] {
        let caller = named(&result, name, "function");
        assert!(!result
            .relations
            .iter()
            .any(|r| r.from == caller && r.kind == "call"));
    }
    let integration = named(&result, "integration_scope", "function");
    let local = result
        .relations
        .iter()
        .find(|r| r.from == integration && result.graph.node(r.to).name == "local")
        .unwrap();
    assert!(result
        .graph
        .node(local.to)
        .path
        .ends_with("tests/integration.rs"));
    assert!(result.relations.iter().any(|r| r.kind == "implements"));
    assert!(result.counts.unresolved_calls >= 3);
}

#[test]
fn source_identity_does_not_depend_on_line_offsets() {
    fn ids(text: &str) -> Vec<(String, String)> {
        let mut graph = GraphBuilder::new();
        let file = graph.push_node(None, NodeKind::File, "test.rs".into(), "test.rs".into(), 0);
        let mut builder = Builder {
            graph,
            source: vec![SourceInfo {
                kind: "file",
                line: 0,
                end_line: 0,
                self_loc: 0,
                doc: String::new(),
                conditional: false,
                identity: "file:test.rs".into(),
            }],
            facts: Facts::default(),
            duplicates: HashMap::new(),
        };
        builder.facts.file_for.insert(file, file);
        let ast = rust_src::parse_file_with_error(text).unwrap();
        symbols::extract(&mut builder, file, Path::new("test.rs"), &ast);
        builder
            .graph
            .nodes()
            .iter()
            .map(|n| {
                (
                    n.name.clone(),
                    stable_id(&builder.source[n.id as usize].identity),
                )
            })
            .collect()
    }
    assert_eq!(
        ids("fn one() {} fn two() {}"),
        ids("\n\nfn one() {}\nfn two() {}\n")
    );
}

#[test]
fn partial_aliases_generics_and_block_scopes_do_not_create_false_call_edges() {
    let result = analyze();
    for name in [
        "ambiguous_reexport",
        "ambiguous_definition_reexport",
        "generic_receiver",
        "block_alias",
        "block_glob",
        "ambiguous_module_prefix",
    ] {
        let caller = named(&result, name, "function");
        assert!(
            !result
                .relations
                .iter()
                .any(|edge| edge.from == caller && edge.kind == "call"),
            "Unexpected resolved call from {name}"
        );
    }
}
