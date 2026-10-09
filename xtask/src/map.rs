//! Generated code map and symbol index for agent navigation (`cargo xtask map`).

use serde::{Deserialize, Serialize};
use std::path::Path;
use syn::{ImplItem, Item, Visibility};

use crate::map_cache;
use crate::size::{count_physical_lines, scoped_files};
use crate::MetricsError;

const JSONL: &str = "docs/metrics/code-map.jsonl";
const SYMBOLS: &str = "docs/metrics/symbols.tsv";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapItem {
    pub name: String,
    pub kind: String,
    pub line: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub doc: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileMap {
    pub path: String,
    pub lines: usize,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub purpose: String,
    pub items: Vec<MapItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SymbolRow {
    name: String,
    kind: String,
    loc: String,
}

pub(crate) fn collect(root: &Path) -> Result<(Vec<FileMap>, Vec<SymbolRow>), MetricsError> {
    let mut cache = map_cache::load(root);
    map_cache::retain_existing(root, &mut cache);
    let mut next_cache = map_cache::empty_store();
    let mut files = Vec::new();
    let mut symbols = Vec::new();

    for path in scoped_files(root) {
        let rel = normalize_rel(&path, root);
        let fm = if let Some(hit) = map_cache::try_hit(&cache, &rel, &path) {
            hit
        } else {
            parse_file_map(&path, &rel)?
        };
        map_cache::insert(&mut next_cache, rel.clone(), &fm, &path);

        for item in &fm.items {
            symbols.push(SymbolRow {
                name: item.name.clone(),
                kind: item.kind.clone(),
                loc: format!("{}:{}", rel, item.line),
            });
        }
        files.push(fm);
    }

    map_cache::save(root, &next_cache)?;

    files.sort_by(|a, b| a.path.cmp(&b.path));
    symbols.sort();
    Ok((files, symbols))
}

fn parse_file_map(path: &Path, rel: &str) -> Result<FileMap, MetricsError> {
    let text = std::fs::read_to_string(path).map_err(|e| MetricsError::io(path, e))?;
    let lines = count_physical_lines(&text);
    let purpose = file_purpose(&text);
    let file = syn::parse_file(&text).map_err(|e| MetricsError::syntax(path, e.to_string()))?;
    let mut items = extract_items(&file, &text);
    items.sort_by(|a, b| (a.line, &a.name, &a.kind).cmp(&(b.line, &b.name, &b.kind)));
    Ok(FileMap {
        path: rel.to_string(),
        lines,
        purpose,
        items,
    })
}

pub(crate) fn render(files: &[FileMap], symbols: &[SymbolRow]) -> (String, String) {
    let mut jsonl = String::new();
    for file in files {
        let line = serde_json::to_string(file).expect("FileMap serializes");
        jsonl.push_str(&line);
        jsonl.push('\n');
    }

    let mut tsv = String::new();
    for row in symbols {
        tsv.push_str(&row.name);
        tsv.push('\t');
        tsv.push_str(&row.kind);
        tsv.push('\t');
        tsv.push_str(&row.loc);
        tsv.push('\n');
    }
    (jsonl, tsv)
}

pub fn write(root: &Path) -> Result<(), MetricsError> {
    let (files, symbols) = collect(root)?;
    let (jsonl, tsv) = render(&files, &symbols);
    let dir = root.join("docs").join("metrics");
    std::fs::create_dir_all(&dir).map_err(|e| MetricsError::io(&dir, e))?;
    let jsonl_path = root.join(JSONL);
    let tsv_path = root.join(SYMBOLS);
    std::fs::write(&jsonl_path, jsonl).map_err(|e| MetricsError::io(&jsonl_path, e))?;
    std::fs::write(&tsv_path, tsv).map_err(|e| MetricsError::io(&tsv_path, e))?;
    Ok(())
}

pub fn check(root: &Path) -> Result<(), MetricsError> {
    let (files, symbols) = collect(root)?;
    let (jsonl, tsv) = render(&files, &symbols);
    let jsonl_path = root.join(JSONL);
    let tsv_path = root.join(SYMBOLS);
    let on_disk_jsonl =
        std::fs::read_to_string(&jsonl_path).map_err(|e| MetricsError::io(&jsonl_path, e))?;
    let on_disk_tsv =
        std::fs::read_to_string(&tsv_path).map_err(|e| MetricsError::io(&tsv_path, e))?;
    if on_disk_jsonl != jsonl {
        return Err(MetricsError::contract(
            &jsonl_path,
            "code-map.jsonl is stale — run `cargo xtask map`",
        ));
    }
    if on_disk_tsv != tsv {
        return Err(MetricsError::contract(
            &tsv_path,
            "symbols.tsv is stale — run `cargo xtask map`",
        ));
    }
    Ok(())
}

fn span_line(_source: &str, span: proc_macro2::Span) -> u32 {
    span.start().line as u32
}

fn normalize_rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn file_purpose(text: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") && !trimmed.starts_with("//!") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("//!") {
            let first = rest.trim();
            if !first.is_empty() {
                return first.to_string();
            }
            continue;
        }
        break;
    }
    String::new()
}

fn extract_items(file: &syn::File, source: &str) -> Vec<MapItem> {
    let mut out = Vec::new();
    for item in &file.items {
        walk_item(item, source, &mut out);
    }
    out
}

fn walk_item(item: &Item, source: &str, out: &mut Vec<MapItem>) {
    match item {
        Item::Fn(i) => maybe_push(
            out,
            "fn",
            &i.sig.ident.to_string(),
            &i.vis,
            &i.attrs,
            span_line(source, i.sig.ident.span()),
        ),
        Item::Struct(i) => maybe_push(
            out,
            "struct",
            &i.ident.to_string(),
            &i.vis,
            &i.attrs,
            span_line(source, i.ident.span()),
        ),
        Item::Enum(i) => maybe_push(
            out,
            "enum",
            &i.ident.to_string(),
            &i.vis,
            &i.attrs,
            span_line(source, i.ident.span()),
        ),
        Item::Trait(i) => maybe_push(
            out,
            "trait",
            &i.ident.to_string(),
            &i.vis,
            &i.attrs,
            span_line(source, i.ident.span()),
        ),
        Item::Const(i) => maybe_push(
            out,
            "const",
            &i.ident.to_string(),
            &i.vis,
            &i.attrs,
            span_line(source, i.ident.span()),
        ),
        Item::Mod(i) => {
            maybe_push(
                out,
                "mod",
                &i.ident.to_string(),
                &i.vis,
                &i.attrs,
                span_line(source, i.ident.span()),
            );
            if let Some((_, items)) = &i.content {
                for child in items {
                    walk_item(child, source, out);
                }
            }
        }
        Item::Impl(i) => {
            for impl_item in &i.items {
                if let ImplItem::Fn(f) = impl_item {
                    maybe_push(
                        out,
                        "fn",
                        &f.sig.ident.to_string(),
                        &f.vis,
                        &f.attrs,
                        span_line(source, f.sig.ident.span()),
                    );
                }
            }
        }
        _ => {}
    }
}

fn maybe_push(
    out: &mut Vec<MapItem>,
    kind: &str,
    name: &str,
    vis: &Visibility,
    attrs: &[syn::Attribute],
    line: u32,
) {
    if !is_pub_or_crate(vis) {
        return;
    }
    out.push(MapItem {
        name: name.to_string(),
        kind: kind.to_string(),
        line,
        doc: first_doc_line(attrs),
    });
}

fn is_pub_or_crate(vis: &Visibility) -> bool {
    match vis {
        Visibility::Public(_) => true,
        Visibility::Restricted(r) => r.path.is_ident("crate"),
        _ => false,
    }
}

fn first_doc_line(attrs: &[syn::Attribute]) -> String {
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        let syn::Meta::NameValue(nv) = &attr.meta else {
            continue;
        };
        let syn::Expr::Lit(lit_expr) = &nv.value else {
            continue;
        };
        let syn::Lit::Str(s) = &lit_expr.lit else {
            continue;
        };
        return s
            .value()
            .lines()
            .find_map(|l| {
                let t = l.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            })
            .unwrap_or_default();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_reads_first_inner_doc_line() {
        let text = "// comment\n\n//! First purpose line\n//! second\n\nfn x() {}\n";
        assert_eq!(file_purpose(text), "First purpose line");
    }

    #[test]
    fn extract_items_finds_pub_items_in_fixture() {
        let text = "//! Widget helpers.\n\npub fn visible() {}\npub(crate) struct Inner;\nstruct Hidden;\n";
        let file = syn::parse_file(text).expect("fixture parses");
        let items = extract_items(&file, text);
        assert!(items.iter().any(|i| i.name == "visible" && i.kind == "fn"));
        assert!(items
            .iter()
            .any(|i| i.name == "Inner" && i.kind == "struct"));
        assert!(!items.iter().any(|i| i.name == "Hidden"));
    }
}
