use std::collections::HashSet;
use std::path::Path;

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Item};

use crate::extract::rust_src::item_metadata;
use crate::model::{ItemKind, NodeId};

use super::{Builder, Call, Impl, Import, ModuleDecl, PathRef};

pub(super) fn extract(builder: &mut Builder, file: NodeId, path: &Path, ast: &syn::File) {
    builder.source[file as usize].doc = docs(&ast.attrs);
    builder.source[file as usize].conditional = conditional(&ast.attrs);
    Extractor {
        builder,
        file,
        path,
        current: file,
        inline: Vec::new(),
    }
    .visit_file(ast);
}

struct Extractor<'a> {
    builder: &'a mut Builder,
    file: NodeId,
    path: &'a Path,
    current: NodeId,
    inline: Vec<String>,
}

impl Extractor<'_> {
    fn with_node(
        &mut self,
        kind: &'static str,
        name: String,
        span: Span,
        attrs: &[Attribute],
        f: impl FnOnce(&mut Self),
    ) {
        let old = self.current;
        let conditional = self.builder.source[old as usize].conditional || conditional(attrs);
        self.current = self.builder.push(
            old,
            kind,
            name,
            self.path,
            span.start().line as u32,
            span.end().line as u32,
            docs(attrs),
            conditional,
        );
        f(self);
        self.current = old;
    }

    fn function_bindings(&mut self, signature: &syn::Signature, block: Option<&syn::Block>) {
        self.generic_bindings(&signature.generics);
        let mut bindings = Bindings::default();
        for input in &signature.inputs {
            if let syn::FnArg::Typed(arg) = input {
                bindings.visit_pat(&arg.pat);
            }
        }
        if let Some(block) = block {
            bindings.visit_block(block);
        }
        self.builder
            .facts
            .bindings
            .insert(self.current, bindings.names);
    }

    fn generic_bindings(&mut self, generics: &syn::Generics) {
        let names = self
            .builder
            .facts
            .type_bindings
            .entry(self.current)
            .or_default();
        for parameter in &generics.params {
            match parameter {
                syn::GenericParam::Type(p) => {
                    names.insert(p.ident.to_string());
                }
                syn::GenericParam::Const(p) => {
                    names.insert(p.ident.to_string());
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Extractor<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if let Item::Use(import) = item {
            let mut rows = Vec::new();
            use_paths(&import.tree, &mut Vec::new(), &mut rows);
            for (parts, alias, glob) in rows {
                if !["file", "module"].contains(&self.builder.source[self.current as usize].kind)
                    && !alias.is_empty()
                {
                    self.builder
                        .facts
                        .type_bindings
                        .entry(self.current)
                        .or_default()
                        .insert(alias.clone());
                }
                self.builder.facts.imports.push(Import {
                    scope: self.current,
                    path: PathRef {
                        parts,
                        absolute: import.leading_colon.is_some(),
                    },
                    alias,
                    glob,
                });
            }
            return;
        }
        if let Item::Mod(module) = item {
            let name = module.ident.to_string();
            self.with_node(
                "module",
                name.clone(),
                module.span(),
                &module.attrs,
                |this| {
                    this.builder.facts.modules.push(ModuleDecl {
                        node: this.current,
                        file: this.file,
                        inline: this.inline.clone(),
                        name: name.clone(),
                        path: path_attr(&module.attrs),
                        uncertain_path: module.attrs.iter().any(|a| {
                            a.path().is_ident("cfg_attr")
                                || (a.path().is_ident("path") && path_attr(&module.attrs).is_none())
                        }),
                        external: module.content.is_none(),
                    });
                    this.inline.push(name);
                    visit::visit_item_mod(this, module);
                    this.inline.pop();
                },
            );
            return;
        }
        let Some((kind, mut name, _)) = item_metadata(item) else {
            visit::visit_item(self, item);
            return;
        };
        if let Item::Macro(mac) = item {
            name = mac
                .ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| mac.mac.path.segments.last().unwrap().ident.to_string());
        }
        let kind = kind_name(kind);
        self.with_node(kind, name, item.span(), attributes(item), |this| {
            if let Item::Fn(function) = item {
                this.function_bindings(&function.sig, Some(&function.block));
            }
            if let Item::Impl(implementation) = item {
                this.generic_bindings(&implementation.generics);
                this.builder.facts.impls.push(Impl {
                    node: this.current,
                    scope: this.builder.facts.parents[&this.current],
                    self_type: implementation
                        .generics
                        .params
                        .is_empty()
                        .then(|| type_path(&implementation.self_ty))
                        .flatten(),
                    trait_type: implementation
                        .trait_
                        .as_ref()
                        .map(|(_, path, _)| path_ref(path)),
                });
            }
            if let Item::Trait(declaration) = item {
                this.generic_bindings(&declaration.generics);
            }
            visit::visit_item(this, item);
        });
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        self.with_node(
            "method",
            function.sig.ident.to_string(),
            function.span(),
            &function.attrs,
            |this| {
                this.function_bindings(&function.sig, Some(&function.block));
                visit::visit_impl_item_fn(this, function);
            },
        );
    }

    fn visit_trait_item_fn(&mut self, function: &'ast syn::TraitItemFn) {
        self.with_node(
            "method",
            function.sig.ident.to_string(),
            function.span(),
            &function.attrs,
            |this| {
                this.function_bindings(&function.sig, function.default.as_ref());
                visit::visit_trait_item_fn(this, function);
            },
        );
    }

    fn visit_foreign_item_fn(&mut self, function: &'ast syn::ForeignItemFn) {
        self.with_node(
            "function",
            function.sig.ident.to_string(),
            function.span(),
            &function.attrs,
            |this| {
                this.function_bindings(&function.sig, None);
                visit::visit_foreign_item_fn(this, function);
            },
        );
    }

    fn visit_impl_item_const(&mut self, item: &'ast syn::ImplItemConst) {
        self.with_node(
            "const",
            item.ident.to_string(),
            item.span(),
            &item.attrs,
            |this| visit::visit_impl_item_const(this, item),
        );
    }

    fn visit_impl_item_type(&mut self, item: &'ast syn::ImplItemType) {
        self.with_node(
            "type_alias",
            item.ident.to_string(),
            item.span(),
            &item.attrs,
            |this| visit::visit_impl_item_type(this, item),
        );
    }

    fn visit_trait_item_const(&mut self, item: &'ast syn::TraitItemConst) {
        self.with_node(
            "const",
            item.ident.to_string(),
            item.span(),
            &item.attrs,
            |this| visit::visit_trait_item_const(this, item),
        );
    }

    fn visit_trait_item_type(&mut self, item: &'ast syn::TraitItemType) {
        self.with_node(
            "type_alias",
            item.ident.to_string(),
            item.span(),
            &item.attrs,
            |this| visit::visit_trait_item_type(this, item),
        );
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        let path = match call.func.as_ref() {
            syn::Expr::Path(path) if path.qself.is_none() => Some(path_ref(&path.path)),
            _ => None,
        };
        self.builder.facts.calls.push(Call {
            caller: self.current,
            scope: self.current,
            path,
            self_method: None,
        });
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let is_self = matches!(call.receiver.as_ref(), syn::Expr::Path(path) if path.qself.is_none() && path.path.is_ident("self"));
        self.builder.facts.calls.push(Call {
            caller: self.current,
            scope: self.current,
            path: None,
            self_method: is_self.then(|| call.method.to_string()),
        });
        visit::visit_expr_method_call(self, call);
    }
}

#[derive(Default)]
struct Bindings {
    names: HashSet<String>,
}

impl<'ast> Visit<'ast> for Bindings {
    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        self.names.insert(pattern.ident.to_string());
        visit::visit_pat_ident(self, pattern);
    }
    fn visit_item(&mut self, _: &'ast Item) {}
}

pub(super) fn path_ref(path: &syn::Path) -> PathRef {
    PathRef {
        parts: path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect(),
        absolute: path.leading_colon.is_some(),
    }
}

fn type_path(ty: &syn::Type) -> Option<PathRef> {
    match ty {
        syn::Type::Path(path)
            if path.qself.is_none()
                && path
                    .path
                    .segments
                    .iter()
                    .all(|s| matches!(s.arguments, syn::PathArguments::None)) =>
        {
            Some(path_ref(&path.path))
        }
        _ => None,
    }
}

fn use_paths(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, String, bool)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_paths(&path.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let mut path = prefix.clone();
            if name.ident != "self" {
                path.push(name.ident.to_string());
            }
            if let Some(alias) = path.last().cloned() {
                out.push((path, alias, false));
            }
        }
        syn::UseTree::Rename(name) => {
            let mut path = prefix.clone();
            if name.ident != "self" {
                path.push(name.ident.to_string());
            }
            out.push((path, name.rename.to_string(), false));
        }
        syn::UseTree::Glob(_) => out.push((prefix.clone(), String::new(), true)),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                use_paths(item, prefix, out);
            }
        }
    }
}

fn docs(attrs: &[Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            match &attr.meta {
                syn::Meta::NameValue(value) => match &value.value {
                    syn::Expr::Lit(value) => match &value.lit {
                        syn::Lit::Str(value) => Some(value.value()),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .chars()
        .take(480)
        .collect()
}

fn conditional(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
}

fn path_attr(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        if let syn::Meta::NameValue(value) = &attr.meta {
            if let syn::Expr::Lit(value) = &value.value {
                if let syn::Lit::Str(value) = &value.lit {
                    return Some(value.value());
                }
            }
        }
        None
    })
}

fn attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(v) => &v.attrs,
        Item::Enum(v) => &v.attrs,
        Item::Fn(v) => &v.attrs,
        Item::Impl(v) => &v.attrs,
        Item::Macro(v) => &v.attrs,
        Item::Static(v) => &v.attrs,
        Item::Struct(v) => &v.attrs,
        Item::Trait(v) => &v.attrs,
        Item::Type(v) => &v.attrs,
        _ => &[],
    }
}

fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Struct => "struct",
        ItemKind::Enum => "enum",
        ItemKind::Trait => "trait",
        ItemKind::Function => "function",
        ItemKind::Impl => "impl",
        ItemKind::TypeAlias => "type_alias",
        ItemKind::Const => "const",
        ItemKind::Static => "static",
        ItemKind::Macro => "macro",
    }
}

pub(super) fn item_kind(kind: &str) -> ItemKind {
    match kind {
        "struct" => ItemKind::Struct,
        "enum" => ItemKind::Enum,
        "trait" => ItemKind::Trait,
        "function" | "method" => ItemKind::Function,
        "impl" => ItemKind::Impl,
        "type_alias" => ItemKind::TypeAlias,
        "const" => ItemKind::Const,
        "static" => ItemKind::Static,
        _ => ItemKind::Macro,
    }
}
