//! Resolution: many kit files in, one ordered tool registry out.
//!
//! Everything here is a *report*, never a refusal. A kit written against a
//! newer build, a tool whose grammar this build lacks, a bar naming a tool that
//! was removed — each degrades to a named finding and leaves the rest working.
//! The alternative, a file that fails to load with one message, is how an
//! extension surface becomes something users are afraid of.

use std::collections::BTreeMap;

use crate::grammar::Grammar;
use crate::kit::{BarDef, Kit};
use crate::tool::ToolDef;

/// Where a kit came from. Later scopes shadow earlier ones, which is what makes
/// "edit a built-in tool" a new file rather than a mutation of a shipped one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// Compiled into the build. Always present, never edited in place.
    Builtin,
    /// The user's kit folder, shared across every workbook they open.
    User,
    /// Carried by (or alongside) one workbook. Highest precedence so a
    /// document can guarantee the tools its content was authored with.
    Workbook,
}

impl Scope {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Scope::Builtin => "built-in",
            Scope::User => "user",
            Scope::Workbook => "workbook",
        }
    }
}

/// Why a tool is not usable, or that it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Ok,
    /// The grammar name is not one this build implements.
    UnsupportedGrammar(String),
    /// The grammar cannot produce what the recipe asks for.
    UnroutableRecipe,
    /// A higher-precedence kit defines the same id.
    Shadowed {
        by: Scope,
        kit: String,
    },
    /// Two tools in one kit claim the same id.
    DuplicateId,
}

impl Health {
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Health::Ok)
    }
}

/// How much a finding matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The tool still works; something was adjusted or is worth knowing.
    Note,
    /// This tool is not available.
    Error,
}

/// One thing worth telling the user about a kit they loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub kit: String,
    /// The tool or bar the finding concerns, when it concerns one.
    pub subject: Option<String>,
    pub message: String,
}

/// A tool after resolution, with its provenance and health attached.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTool {
    pub kit: String,
    pub scope: Scope,
    pub def: ToolDef,
    pub health: Health,
    /// The grammar, once resolved. `None` whenever `health` is not `Ok`.
    pub grammar: Option<Grammar>,
    /// The accelerator this tool actually gets. Dropped when it collided with a
    /// higher-precedence tool, so bindings stay unambiguous.
    pub key: Option<String>,
}

impl ResolvedTool {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.health.is_ok()
    }
}

/// A bar after resolution: only items that resolved to an active tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBar {
    pub kit: String,
    pub scope: Scope,
    pub id: String,
    pub name: String,
    pub items: Vec<String>,
}

/// Every tool available to the board, resolved across scopes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Registry {
    tools: Vec<ResolvedTool>,
    bars: Vec<ResolvedBar>,
    findings: Vec<Finding>,
}

impl Registry {
    /// Resolve kits into one registry. Input order does not matter; `Scope`
    /// decides precedence, and ties within a scope go to the first kit given.
    #[must_use]
    pub fn build(kits: &[(Scope, Kit)]) -> Registry {
        let mut reg = Registry::default();
        let mut ordered: Vec<&(Scope, Kit)> = kits.iter().collect();
        // Stable sort: highest precedence first, original order preserved
        // within a scope.
        ordered.sort_by_key(|(scope, _)| std::cmp::Reverse(*scope));

        // id -> (scope, kit) of the winner claiming it.
        let mut claimed: BTreeMap<String, (Scope, String)> = BTreeMap::new();
        // accelerator -> tool id holding it.
        let mut keys: BTreeMap<String, String> = BTreeMap::new();

        for (scope, kit) in ordered {
            let mut seen_in_kit: BTreeMap<&str, ()> = BTreeMap::new();
            for def in &kit.tools {
                let health = if seen_in_kit.insert(def.id.as_str(), ()).is_some() {
                    reg.findings.push(Finding {
                        severity: Severity::Error,
                        kit: kit.id.clone(),
                        subject: Some(def.id.clone()),
                        message: format!("`{}` is defined twice in this kit", def.id),
                    });
                    Health::DuplicateId
                } else if let Some((by, owner)) = claimed.get(&def.id) {
                    reg.findings.push(Finding {
                        severity: Severity::Note,
                        kit: kit.id.clone(),
                        subject: Some(def.id.clone()),
                        message: format!(
                            "`{}` is shadowed by the {} kit `{owner}`",
                            def.id,
                            by.label()
                        ),
                    });
                    Health::Shadowed {
                        by: *by,
                        kit: owner.clone(),
                    }
                } else {
                    match def.grammar() {
                        None => {
                            reg.findings.push(Finding {
                                severity: Severity::Error,
                                kit: kit.id.clone(),
                                subject: Some(def.id.clone()),
                                message: format!(
                                    "grammar `{}` is not one this build implements",
                                    def.grammar.as_str()
                                ),
                            });
                            Health::UnsupportedGrammar(def.grammar.as_str().to_string())
                        }
                        Some(g) if !def.recipe.accepts(g) => {
                            reg.findings.push(Finding {
                                severity: Severity::Error,
                                kit: kit.id.clone(),
                                subject: Some(def.id.clone()),
                                message: format!(
                                    "the `{}` grammar cannot produce what this recipe asks for",
                                    g.id()
                                ),
                            });
                            Health::UnroutableRecipe
                        }
                        Some(_) => Health::Ok,
                    }
                };

                if health.is_ok() {
                    claimed.insert(def.id.clone(), (*scope, kit.id.clone()));
                }

                // An accelerator only survives if it is free. The first claim
                // wins, so a higher-precedence kit keeps its binding.
                let key = match (&health, &def.key) {
                    (Health::Ok, Some(k)) => match keys.get(k) {
                        Some(owner) => {
                            reg.findings.push(Finding {
                                severity: Severity::Note,
                                kit: kit.id.clone(),
                                subject: Some(def.id.clone()),
                                message: format!(
                                    "`{k}` is already bound to `{owner}`; reach this tool by name instead"
                                ),
                            });
                            None
                        }
                        None => {
                            keys.insert(k.clone(), def.id.clone());
                            Some(k.clone())
                        }
                    },
                    _ => None,
                };

                reg.tools.push(ResolvedTool {
                    kit: kit.id.clone(),
                    scope: *scope,
                    grammar: if health.is_ok() { def.grammar() } else { None },
                    def: def.clone(),
                    health,
                    key,
                });
            }

            for bar in &kit.bars {
                reg.bars
                    .push(resolve_bar(bar, kit, *scope, &mut reg.findings));
            }
        }

        // Bars can name tools defined in another kit, so item pruning waits
        // until every tool is known.
        let active: Vec<String> = reg
            .tools
            .iter()
            .filter(|t| t.is_active())
            .map(|t| t.def.id.clone())
            .collect();
        for bar in &mut reg.bars {
            let before = std::mem::take(&mut bar.items);
            for item in before {
                if active.contains(&item) {
                    bar.items.push(item);
                } else {
                    reg.findings.push(Finding {
                        severity: Severity::Note,
                        kit: bar.kit.clone(),
                        subject: Some(bar.id.clone()),
                        message: format!("bar item `{item}` has no available tool; omitted"),
                    });
                }
            }
        }

        reg
    }

    /// Tools that can actually be armed, highest-precedence first.
    pub fn active(&self) -> impl Iterator<Item = &ResolvedTool> {
        self.tools.iter().filter(|t| t.is_active())
    }

    /// Every tool, including shadowed and unsupported ones, so an authoring
    /// interface can show the user why something is missing.
    #[must_use]
    pub fn all(&self) -> &[ResolvedTool] {
        &self.tools
    }

    #[must_use]
    pub fn bars(&self) -> &[ResolvedBar] {
        &self.bars
    }

    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    #[must_use]
    pub fn errors(&self) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect()
    }

    /// The active tool with this id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ResolvedTool> {
        self.active().find(|t| t.def.id == id)
    }

    /// Active tools whose name, id, or aliases contain `needle`, for
    /// type-to-command. Case-insensitive; prefix matches sort first.
    #[must_use]
    pub fn search(&self, needle: &str) -> Vec<&ResolvedTool> {
        let n = needle.trim().to_lowercase();
        if n.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u8, &ResolvedTool)> = self
            .active()
            .filter_map(|t| {
                t.def
                    .search_terms()
                    .iter()
                    .filter_map(|term| {
                        let lower = term.to_lowercase();
                        if lower == n {
                            Some(0u8)
                        } else if lower.starts_with(&n) {
                            Some(1)
                        } else if lower.contains(&n) {
                            Some(2)
                        } else {
                            None
                        }
                    })
                    .min()
                    .map(|rank| (rank, t))
            })
            .collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.def.name.cmp(&b.1.def.name)));
        hits.into_iter().map(|(_, t)| t).collect()
    }
}

fn resolve_bar(bar: &BarDef, kit: &Kit, scope: Scope, findings: &mut Vec<Finding>) -> ResolvedBar {
    if bar.items.is_empty() {
        findings.push(Finding {
            severity: Severity::Note,
            kit: kit.id.clone(),
            subject: Some(bar.id.clone()),
            message: format!("bar `{}` is empty", bar.id),
        });
    }
    ResolvedBar {
        kit: kit.id.clone(),
        scope,
        id: bar.id.clone(),
        name: bar.name.clone(),
        items: bar.items.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kit(id: &str, body: &str) -> Kit {
        Kit::from_toml(&format!(
            "format_version = 1\nid = \"{id}\"\nname = \"{id}\"\n{body}"
        ))
        .unwrap_or_else(|e| panic!("kit `{id}` failed to parse: {e}"))
    }

    fn rect_tool(id: &str, key: Option<&str>) -> String {
        let k = key.map_or(String::new(), |k| format!("key = \"{k}\"\n"));
        format!(
            "[[tool]]\nid = \"{id}\"\nname = \"{id}\"\ngrammar = \"drag_rect\"\n{k}recipe = {{ kind = \"shape\", node = \"rect\" }}\n"
        )
    }

    #[test]
    fn a_user_kit_shadows_a_builtin_tool_of_the_same_id() {
        let builtin = kit("core", &rect_tool("rect", None));
        let user = kit("mine", &rect_tool("rect", None));
        let reg = Registry::build(&[(Scope::Builtin, builtin), (Scope::User, user)]);

        assert_eq!(reg.active().count(), 1);
        let winner = reg.get("rect").unwrap();
        assert_eq!(winner.scope, Scope::User);
        assert_eq!(winner.kit, "mine");

        let loser = reg
            .all()
            .iter()
            .find(|t| t.kit == "core")
            .expect("built-in still listed");
        assert_eq!(
            loser.health,
            Health::Shadowed {
                by: Scope::User,
                kit: "mine".into()
            }
        );
        // Shadowing is expected, not a failure.
        assert!(reg.errors().is_empty());
    }

    #[test]
    fn a_workbook_kit_outranks_a_user_kit() {
        let reg = Registry::build(&[
            (Scope::User, kit("mine", &rect_tool("rect", None))),
            (Scope::Workbook, kit("doc", &rect_tool("rect", None))),
            (Scope::Builtin, kit("core", &rect_tool("rect", None))),
        ]);
        assert_eq!(reg.get("rect").unwrap().scope, Scope::Workbook);
        assert_eq!(reg.active().count(), 1);
    }

    #[test]
    fn input_order_does_not_change_the_outcome() {
        let a = Registry::build(&[
            (Scope::Builtin, kit("core", &rect_tool("rect", None))),
            (Scope::User, kit("mine", &rect_tool("rect", None))),
        ]);
        let b = Registry::build(&[
            (Scope::User, kit("mine", &rect_tool("rect", None))),
            (Scope::Builtin, kit("core", &rect_tool("rect", None))),
        ]);
        assert_eq!(a.get("rect").unwrap().kit, b.get("rect").unwrap().kit);
    }

    #[test]
    fn an_unsupported_grammar_costs_one_tool_and_not_the_kit() {
        let body = format!(
            "{}[[tool]]\nid = \"solver\"\nname = \"Solver\"\ngrammar = \"constraint_solve\"\nrecipe = {{ kind = \"shape\", node = \"rect\" }}\n",
            rect_tool("rect", None)
        );
        let reg = Registry::build(&[(Scope::User, kit("mine", &body))]);

        assert!(reg.get("rect").is_some(), "the healthy tool still loads");
        assert!(reg.get("solver").is_none());
        assert_eq!(
            reg.all()
                .iter()
                .find(|t| t.def.id == "solver")
                .map(|t| &t.health),
            Some(&Health::UnsupportedGrammar("constraint_solve".into()))
        );
        assert_eq!(reg.errors().len(), 1);
        assert!(reg.errors()[0].message.contains("constraint_solve"));
    }

    #[test]
    fn a_recipe_its_grammar_cannot_produce_is_refused_with_a_reason() {
        let body = "[[tool]]\nid = \"bad\"\nname = \"Bad\"\ngrammar = \"sample\"\nrecipe = { kind = \"shape\", node = \"rect\" }\n";
        let reg = Registry::build(&[(Scope::User, kit("mine", body))]);
        assert!(reg.get("bad").is_none());
        assert_eq!(
            reg.all()[0].health,
            Health::UnroutableRecipe,
            "sample creates no nodes"
        );
        assert!(reg.errors()[0].message.contains("sample"));
    }

    #[test]
    fn a_duplicate_id_inside_one_kit_keeps_the_first_and_reports_the_second() {
        let body = format!("{}{}", rect_tool("rect", None), rect_tool("rect", None));
        let reg = Registry::build(&[(Scope::User, kit("mine", &body))]);
        assert_eq!(reg.active().count(), 1);
        assert_eq!(reg.all().len(), 2);
        assert_eq!(reg.all()[1].health, Health::DuplicateId);
    }

    #[test]
    fn a_colliding_accelerator_is_dropped_and_the_tool_stays_reachable_by_name() {
        let reg = Registry::build(&[
            (Scope::Builtin, kit("core", &rect_tool("rect", Some("R")))),
            (Scope::User, kit("mine", &rect_tool("blob", Some("R")))),
        ]);
        // The user kit sorts first, so it keeps the binding.
        assert_eq!(reg.get("blob").unwrap().key.as_deref(), Some("R"));
        assert_eq!(reg.get("rect").unwrap().key, None);
        assert!(reg.get("rect").unwrap().is_active(), "still usable");
        assert!(!reg.search("rect").is_empty());
        assert!(reg
            .findings()
            .iter()
            .any(|f| f.message.contains("already bound")));
    }

    #[test]
    fn a_shadowed_tool_does_not_hold_an_accelerator_hostage() {
        let reg = Registry::build(&[
            (Scope::Builtin, kit("core", &rect_tool("rect", Some("R")))),
            (Scope::User, kit("mine", &rect_tool("rect", None))),
        ]);
        // The winning definition chose no key, and the shadowed one must not
        // keep `R` alive for a tool nobody can reach.
        assert_eq!(reg.get("rect").unwrap().key, None);
        assert!(reg.all().iter().all(|t| t.key.is_none()));
    }

    #[test]
    fn a_bar_drops_items_with_no_available_tool() {
        let body = format!(
            "{}[[bar]]\nid = \"draw\"\nname = \"Draw\"\nitems = [\"rect\", \"ghost\"]\n",
            rect_tool("rect", None)
        );
        let reg = Registry::build(&[(Scope::User, kit("mine", &body))]);
        assert_eq!(reg.bars()[0].items, vec!["rect"]);
        assert!(reg
            .findings()
            .iter()
            .any(|f| f.message.contains("bar item `ghost`")));
    }

    #[test]
    fn a_bar_may_name_a_tool_from_another_kit() {
        let core = kit("core", &rect_tool("rect", None));
        let bars = kit(
            "mine",
            "[[bar]]\nid = \"draw\"\nname = \"Draw\"\nitems = [\"rect\"]\n",
        );
        let reg = Registry::build(&[(Scope::Builtin, core), (Scope::User, bars)]);
        assert_eq!(reg.bars()[0].items, vec!["rect"]);
    }

    #[test]
    fn search_ranks_exact_then_prefix_then_substring() {
        let body = "[[tool]]\nid = \"a\"\nname = \"Line\"\ngrammar = \"two_point\"\nrecipe = { kind = \"shape\", node = \"path\" }\n\
                    [[tool]]\nid = \"b\"\nname = \"Linear dimension\"\ngrammar = \"two_point\"\nrecipe = { kind = \"shape\", node = \"path\" }\n\
                    [[tool]]\nid = \"c\"\nname = \"Centerline\"\ngrammar = \"two_point\"\nrecipe = { kind = \"shape\", node = \"path\" }\n";
        let reg = Registry::build(&[(Scope::User, kit("mine", body))]);
        let names: Vec<&str> = reg
            .search("line")
            .iter()
            .map(|t| t.def.name.as_str())
            .collect();
        assert_eq!(names, vec!["Line", "Linear dimension", "Centerline"]);
    }

    #[test]
    fn search_finds_aliases_and_ignores_case() {
        let body = "[[tool]]\nid = \"line-2pt\"\nname = \"Line\"\ngrammar = \"two_point\"\naliases = [\"polyline\"]\nrecipe = { kind = \"shape\", node = \"path\" }\n";
        let reg = Registry::build(&[(Scope::User, kit("mine", body))]);
        assert_eq!(reg.search("POLY").len(), 1);
        assert_eq!(reg.search("2pt").len(), 1);
        assert!(reg.search("").is_empty());
        assert!(reg.search("nothing here").is_empty());
    }

    #[test]
    fn search_never_offers_a_tool_that_cannot_be_armed() {
        let body = "[[tool]]\nid = \"solver\"\nname = \"Solver\"\ngrammar = \"constraint_solve\"\nrecipe = { kind = \"shape\", node = \"rect\" }\n";
        let reg = Registry::build(&[(Scope::User, kit("mine", body))]);
        assert!(reg.search("solver").is_empty());
    }

    #[test]
    fn an_empty_registry_is_harmless() {
        let reg = Registry::build(&[]);
        assert_eq!(reg.active().count(), 0);
        assert!(reg.findings().is_empty());
        assert!(reg.get("rect").is_none());
    }
}
