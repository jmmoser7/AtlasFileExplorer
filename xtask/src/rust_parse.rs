//! Every Rust-source metric goes through `syn`.
//!
//! A regex over `#[test]` or `pub const CURRENT` reads comments, doc examples,
//! and string literals as code, and the whole point of the snapshot is that two
//! runs a month apart are comparable. Parsing is the only way to be literal.

use std::path::Path;

use syn::visit::{self, Visit};

use crate::MetricsError;

/// Per-file counts taken from the parsed syntax tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileCounts {
    pub tests: u32,
    pub unsafe_blocks: u32,
}

pub fn parse_file(path: &Path) -> Result<syn::File, MetricsError> {
    let text = std::fs::read_to_string(path).map_err(|e| MetricsError::io(path, e))?;
    syn::parse_file(&text).map_err(|e| MetricsError::syntax(path, e.to_string()))
}

/// `#[test]` attributes and `unsafe { .. }` blocks — the two drift canaries the
/// card asks for, in one walk.
pub fn file_counts(file: &syn::File) -> FileCounts {
    let mut visitor = CountVisitor::default();
    visitor.visit_file(file);
    visitor.counts
}

/// The integer literal of a `const NAME: u32 = N;`, free-standing or in an
/// `impl` block. This is how `format_version` is read out of `doc.rs`.
pub fn const_u32(file: &syn::File, name: &str) -> Option<u32> {
    let mut visitor = ConstVisitor { name, value: None };
    visitor.visit_file(file);
    visitor.value
}

/// Variant names of `enum NAME`, in declaration order, converted to the
/// `snake_case` the model serializes them as.
pub fn enum_variants(file: &syn::File, name: &str) -> Option<Vec<String>> {
    let mut visitor = EnumVisitor {
        name,
        variants: None,
    };
    visitor.visit_file(file);
    visitor.variants
}

/// Element count of a `static`/`const` slice literal such as
/// `pub static SPECS: &[CommandSpec] = &[ .. ];`.
pub fn slice_literal_len(file: &syn::File, name: &str) -> Option<usize> {
    let mut visitor = SliceVisitor { name, len: None };
    visitor.visit_file(file);
    visitor.len
}

fn snake_case(ident: &str) -> String {
    let mut out = String::with_capacity(ident.len() + 4);
    for (i, ch) in ident.char_indices() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn array_len(expr: &syn::Expr) -> Option<usize> {
    match expr {
        syn::Expr::Reference(r) => array_len(&r.expr),
        syn::Expr::Group(g) => array_len(&g.expr),
        syn::Expr::Paren(p) => array_len(&p.expr),
        syn::Expr::Array(a) => Some(a.elems.len()),
        _ => None,
    }
}

fn int_value(expr: &syn::Expr) -> Option<u32> {
    match expr {
        syn::Expr::Group(g) => int_value(&g.expr),
        syn::Expr::Paren(p) => int_value(&p.expr),
        syn::Expr::Lit(lit) => match &lit.lit {
            syn::Lit::Int(int) => int.base10_parse().ok(),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Default)]
struct CountVisitor {
    counts: FileCounts,
}

impl<'ast> Visit<'ast> for CountVisitor {
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        if node.path().is_ident("test") {
            self.counts.tests += 1;
        }
        visit::visit_attribute(self, node);
    }

    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.counts.unsafe_blocks += 1;
        visit::visit_expr_unsafe(self, node);
    }
}

struct ConstVisitor<'a> {
    name: &'a str,
    value: Option<u32>,
}

impl<'ast> Visit<'ast> for ConstVisitor<'_> {
    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        if self.value.is_none() && node.ident == self.name {
            self.value = int_value(&node.expr);
        }
        visit::visit_item_const(self, node);
    }

    fn visit_impl_item_const(&mut self, node: &'ast syn::ImplItemConst) {
        if self.value.is_none() && node.ident == self.name {
            self.value = int_value(&node.expr);
        }
        visit::visit_impl_item_const(self, node);
    }
}

struct EnumVisitor<'a> {
    name: &'a str,
    variants: Option<Vec<String>>,
}

impl<'ast> Visit<'ast> for EnumVisitor<'_> {
    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        if self.variants.is_none() && node.ident == self.name {
            self.variants = Some(
                node.variants
                    .iter()
                    .map(|v| snake_case(&v.ident.to_string()))
                    .collect(),
            );
        }
        visit::visit_item_enum(self, node);
    }
}

struct SliceVisitor<'a> {
    name: &'a str,
    len: Option<usize>,
}

impl<'ast> Visit<'ast> for SliceVisitor<'_> {
    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        if self.len.is_none() && node.ident == self.name {
            self.len = array_len(&node.expr);
        }
        visit::visit_item_static(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        if self.len.is_none() && node.ident == self.name {
            self.len = array_len(&node.expr);
        }
        visit::visit_item_const(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_tests_and_unsafe_blocks_not_their_mentions() {
        let file = syn::parse_file(
            r##"
            // #[test] in a comment is not a test.
            const NOTE: &str = "#[test] in a string is not a test either";

            #[test]
            fn a() {
                let _ = unsafe { 1 };
            }

            #[cfg(test)]
            mod inner {
                #[test]
                fn b() {                }
            }
            "##,
        )
        .expect("fixture parses");
        assert_eq!(
            file_counts(&file),
            FileCounts {
                tests: 2,
                unsafe_blocks: 1
            }
        );
    }

    #[test]
    fn reads_associated_const_and_enum_and_slice() {
        let file = syn::parse_file(
            r#"
            struct Doc;
            impl Doc {
                pub const CURRENT: u32 = 7;
            }
            pub enum NodeKind { Frame, ImageNode, Text }
            pub static SPECS: &[u8] = &[1, 2, 3, 4];
            "#,
        )
        .expect("fixture parses");
        assert_eq!(const_u32(&file, "CURRENT"), Some(7));
        assert_eq!(
            enum_variants(&file, "NodeKind"),
            Some(vec![
                "frame".to_string(),
                "image_node".to_string(),
                "text".to_string()
            ])
        );
        assert_eq!(enum_variants(&file, "EdgeRole"), None);
        assert_eq!(slice_literal_len(&file, "SPECS"), Some(4));
    }
}
