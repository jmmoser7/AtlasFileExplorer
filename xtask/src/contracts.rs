//! Reads `docs/keymap/contracts/` and checks that its three artifacts agree.
//!
//! The contract system has one rule that a human cannot enforce by reading:
//! *silence is not an answer*. Every contract must account for every dimension
//! the registry scopes to its family, and every row must exist in
//! `decisions.json` with the same verdict story the contract tells. Three hand-
//! authored files drifting apart is exactly the class of bug the registry was
//! invented to prevent, so the check is machine-run.
//!
//! Parsing is deliberately strict, on the ledger's principle: an unreadable
//! header fails the run rather than quietly checking nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::ledger::{columns, is_row, is_separator};
use crate::MetricsError;

const SOURCES: [&str; 5] = ["stated", "research", "pattern", "guess", "precedent"];
const VERDICTS: [&str; 3] = ["proposed", "approved", "rejected"];

/// Which contract families must answer a dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Tool,
    Portal,
    Any,
}

impl Scope {
    fn covers(self, family: Family) -> bool {
        matches!(
            (self, family),
            (Scope::Any, _) | (Scope::Tool, Family::Tool) | (Scope::Portal, Family::Portal)
        )
    }
}

/// What a contract describes: a canvas tool or a portal subtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Tool,
    Portal,
}

impl Family {
    fn parse(word: &str) -> Option<Family> {
        match word {
            "tool" => Some(Family::Tool),
            "portal" => Some(Family::Portal),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Family::Tool => "tool",
            Family::Portal => "portal",
        }
    }
}

/// How far through the workflow a contract is. Agreed and shipped carry
/// obligations the check enforces; draft carries none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Draft,
    Agreed,
    Shipped,
}

impl Status {
    fn parse(word: &str) -> Option<Status> {
        match word {
            "draft" => Some(Status::Draft),
            "agreed" => Some(Status::Agreed),
            "shipped" => Some(Status::Shipped),
            _ => None,
        }
    }

    fn settled(self) -> bool {
        matches!(self, Status::Agreed | Status::Shipped)
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Draft => "draft",
            Status::Agreed => "agreed",
            Status::Shipped => "shipped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dimension {
    pub id: String,
    pub name: String,
    pub scope: Scope,
}

/// `DIMENSIONS.md`, the permanent registry, in its canonical order.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    pub dims: Vec<Dimension>,
}

impl Registry {
    pub fn in_scope(&self, family: Family) -> Vec<&Dimension> {
        self.dims
            .iter()
            .filter(|d| d.scope.covers(family))
            .collect()
    }
}

/// One `contracts/<name>.md`, reduced to what the check needs.
#[derive(Debug, Clone)]
pub struct Contract {
    pub name: String,
    pub family: Family,
    pub status: Status,
    /// Matrix row IDs, in the order the contract lists them.
    pub rows: Vec<String>,
    /// True when the Open questions section's first line begins with "None".
    pub questions_closed: bool,
}

#[derive(Debug, Clone)]
pub struct Decision {
    pub source: String,
    pub confidence: i64,
    pub verdict: String,
    pub decided: Option<String>,
}

/// One contract's entry in `decisions.json`.
#[derive(Debug, Clone)]
pub struct DecisionEntry {
    pub family: Family,
    pub rows: Vec<(String, Decision)>,
}

/// A disagreement between the three artifacts. Findings are never fatal to
/// parsing — the run collects all of them so one command shows the whole gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub contract: String,
    pub message: String,
}

impl Finding {
    fn new(contract: &str, message: impl Into<String>) -> Finding {
        Finding {
            contract: contract.to_string(),
            message: message.into(),
        }
    }
}

/// Everything one `cargo xtask contracts` run learned.
#[derive(Debug)]
pub struct Audit {
    pub registry: Registry,
    pub contracts: Vec<Contract>,
    pub decisions: BTreeMap<String, DecisionEntry>,
    pub findings: Vec<Finding>,
}

pub fn contracts_dir(root: &Path) -> PathBuf {
    root.join("docs").join("keymap").join("contracts")
}

/// Reads the registry, every contract, and the decisions database, then checks
/// them against each other.
pub fn audit(root: &Path) -> Result<Audit, MetricsError> {
    let dir = contracts_dir(root);

    let registry_path = dir.join("DIMENSIONS.md");
    let registry = parse_registry(&read(&registry_path)?, &registry_path)?;

    let decisions_path = dir.join("decisions.json");
    let decisions = parse_decisions(&read(&decisions_path)?, &decisions_path)?;

    let mut contracts = Vec::new();
    for path in contract_paths(&dir)? {
        contracts.push(parse_contract(&read(&path)?, &path)?);
    }

    let findings = check(&registry, &contracts, &decisions);
    Ok(Audit {
        registry,
        contracts,
        decisions,
        findings,
    })
}

/// Contract files are the lowercase `.md` files in the directory: the
/// uppercase ones (`DIMENSIONS`, `PATTERNS`, `TEMPLATE`) are the framework.
fn contract_paths(dir: &Path) -> Result<Vec<PathBuf>, MetricsError> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| MetricsError::io(dir, e))? {
        let path = entry.map_err(|e| MetricsError::io(dir, e))?.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if stem.chars().any(|c| c.is_ascii_uppercase()) {
            continue;
        }
        paths.push(path);
    }
    paths.sort();
    Ok(paths)
}

fn read(path: &Path) -> Result<String, MetricsError> {
    std::fs::read_to_string(path).map_err(|e| MetricsError::io(path, e))
}

/// Parses `DIMENSIONS.md`'s registry table. IDs must run `D01`, `D02`, … with
/// no gaps: the append-only promise is only worth anything if it holds.
pub fn parse_registry(text: &str, path: &Path) -> Result<Registry, MetricsError> {
    let (header, body) = table_after(text, &["ID", "Dimension", "Scope"])
        .ok_or_else(|| MetricsError::contract(path, "no registry table with a `Scope` column"))?;
    let scope_col = column_index(&header, "Scope")
        .ok_or_else(|| MetricsError::contract(path, "registry table has no `Scope` column"))?;
    let name_col = column_index(&header, "Dimension")
        .ok_or_else(|| MetricsError::contract(path, "registry table has no `Dimension` column"))?;

    let mut dims: Vec<Dimension> = Vec::new();
    for cells in body {
        let id = cells.first().cloned().unwrap_or_default();
        let expected = format!("D{:02}", dims.len() + 1);
        if id != expected {
            return Err(MetricsError::contract(
                path,
                format!("registry row `{id}` breaks the sequence — expected `{expected}`"),
            ));
        }
        let raw_scope = cells.get(scope_col).cloned().unwrap_or_default();
        let scope = match raw_scope.as_str() {
            "tool" => Scope::Tool,
            "portal" => Scope::Portal,
            "any" => Scope::Any,
            other => {
                return Err(MetricsError::contract(
                    path,
                    format!("dimension `{id}` has unknown Scope `{other}`"),
                ))
            }
        };
        dims.push(Dimension {
            id,
            name: cells.get(name_col).cloned().unwrap_or_default(),
            scope,
        });
    }
    if dims.is_empty() {
        return Err(MetricsError::contract(path, "the registry table is empty"));
    }
    Ok(Registry { dims })
}

/// Parses one contract: its header fields and the IDs in its behavior matrix.
pub fn parse_contract(text: &str, path: &Path) -> Result<Contract, MetricsError> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();

    let status_word = header_field(text, "Status")
        .ok_or_else(|| MetricsError::contract(path, "no `Status:` line"))?;
    let status = Status::parse(&status_word)
        .ok_or_else(|| MetricsError::contract(path, format!("unknown Status `{status_word}`")))?;

    let family_word = header_field(text, "Family").ok_or_else(|| {
        MetricsError::contract(path, "no `Family:` line (tool | portal — see TEMPLATE.md)")
    })?;
    let family = Family::parse(&family_word)
        .ok_or_else(|| MetricsError::contract(path, format!("unknown Family `{family_word}`")))?;

    let (_, body) = table_after(text, &["ID", "Dimension"])
        .ok_or_else(|| MetricsError::contract(path, "no behavior matrix table"))?;
    let rows: Vec<String> = body
        .iter()
        .filter_map(|cells| cells.first().cloned())
        .filter(|id| is_dimension_id(id))
        .collect();
    if rows.is_empty() {
        return Err(MetricsError::contract(
            path,
            "the behavior matrix has no `D##` rows",
        ));
    }

    Ok(Contract {
        name,
        family,
        status,
        rows,
        questions_closed: open_questions_closed(text),
    })
}

/// Parses `decisions.json` into one entry per contract.
pub fn parse_decisions(
    text: &str,
    path: &Path,
) -> Result<BTreeMap<String, DecisionEntry>, MetricsError> {
    let root: Value = serde_json::from_str(text)
        .map_err(|e| MetricsError::contract(path, format!("invalid JSON: {e}")))?;
    let tools = root
        .get("tools")
        .and_then(Value::as_object)
        .ok_or_else(|| MetricsError::contract(path, "no `tools` object"))?;

    let mut out = BTreeMap::new();
    for (name, entry) in tools {
        let family_word = entry
            .get("family")
            .and_then(Value::as_str)
            .ok_or_else(|| MetricsError::contract(path, format!("`{name}` has no `family`")))?;
        let family = Family::parse(family_word).ok_or_else(|| {
            MetricsError::contract(path, format!("`{name}` has unknown family `{family_word}`"))
        })?;
        let decisions = entry
            .get("decisions")
            .and_then(Value::as_object)
            .ok_or_else(|| MetricsError::contract(path, format!("`{name}` has no `decisions`")))?;

        let mut rows = Vec::new();
        for (id, row) in decisions {
            let field = |key: &str| -> Result<&Value, MetricsError> {
                row.get(key).ok_or_else(|| {
                    MetricsError::contract(path, format!("`{name}.{id}` has no `{key}`"))
                })
            };
            let source = field("source")?
                .as_str()
                .ok_or_else(|| {
                    MetricsError::contract(path, format!("`{name}.{id}.source` is not a string"))
                })?
                .to_string();
            let confidence = field("confidence")?.as_i64().ok_or_else(|| {
                MetricsError::contract(path, format!("`{name}.{id}.confidence` is not a number"))
            })?;
            let verdict = field("verdict")?
                .as_str()
                .ok_or_else(|| {
                    MetricsError::contract(path, format!("`{name}.{id}.verdict` is not a string"))
                })?
                .to_string();
            let decided = match field("decided")? {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => {
                    return Err(MetricsError::contract(
                        path,
                        format!("`{name}.{id}.decided` is neither a date string nor null"),
                    ))
                }
            };
            if field("behavior")?
                .as_str()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(MetricsError::contract(
                    path,
                    format!("`{name}.{id}.behavior` is empty"),
                ));
            }
            rows.push((
                id.clone(),
                Decision {
                    source,
                    confidence,
                    verdict,
                    decided,
                },
            ));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        out.insert(name.clone(), DecisionEntry { family, rows });
    }
    Ok(out)
}

/// The rules, in one place, so a failure message can quote the rule it broke.
pub fn check(
    registry: &Registry,
    contracts: &[Contract],
    decisions: &BTreeMap<String, DecisionEntry>,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    for contract in contracts {
        let expected: Vec<String> = registry
            .in_scope(contract.family)
            .into_iter()
            .map(|d| d.id.clone())
            .collect();

        if contract.rows != expected {
            for id in &expected {
                if !contract.rows.contains(id) {
                    findings.push(Finding::new(
                        &contract.name,
                        format!(
                            "misses {id} — every dimension scoped to `{}` needs a row \
                             (an answer, a pattern reference, or `n/a`)",
                            contract.family.label()
                        ),
                    ));
                }
            }
            for id in &contract.rows {
                if !expected.contains(id) {
                    findings.push(Finding::new(
                        &contract.name,
                        format!("answers {id}, which the registry does not scope to it"),
                    ));
                }
            }
            let mut sorted = contract.rows.clone();
            sorted.sort();
            sorted.dedup();
            if sorted.len() != contract.rows.len() {
                findings.push(Finding::new(&contract.name, "repeats a dimension row"));
            } else if sorted == {
                let mut e = expected.clone();
                e.sort();
                e
            } {
                findings.push(Finding::new(
                    &contract.name,
                    "lists its rows out of registry order",
                ));
            }
        }

        let Some(entry) = decisions.get(&contract.name) else {
            findings.push(Finding::new(
                &contract.name,
                "has no entry in decisions.json — every matrix row is mirrored there",
            ));
            continue;
        };

        if entry.family != contract.family {
            findings.push(Finding::new(
                &contract.name,
                format!(
                    "is `{}` in the contract and `{}` in decisions.json",
                    contract.family.label(),
                    entry.family.label()
                ),
            ));
        }

        let recorded: Vec<String> = entry.rows.iter().map(|(id, _)| id.clone()).collect();
        for id in &contract.rows {
            if !recorded.contains(id) {
                findings.push(Finding::new(
                    &contract.name,
                    format!("{id} is in the matrix but not in decisions.json"),
                ));
            }
        }
        for id in &recorded {
            if !contract.rows.contains(id) {
                findings.push(Finding::new(
                    &contract.name,
                    format!("{id} is in decisions.json but not in the matrix"),
                ));
            }
        }

        for (id, row) in &entry.rows {
            if !SOURCES.contains(&row.source.as_str()) {
                findings.push(Finding::new(
                    &contract.name,
                    format!("{id} has unknown source `{}`", row.source),
                ));
            }
            if !VERDICTS.contains(&row.verdict.as_str()) {
                findings.push(Finding::new(
                    &contract.name,
                    format!("{id} has unknown verdict `{}`", row.verdict),
                ));
            }
            if !(0..=100).contains(&row.confidence) {
                findings.push(Finding::new(
                    &contract.name,
                    format!("{id} has confidence {} outside 0–100", row.confidence),
                ));
            }
            if row.verdict == "approved" && row.decided.is_none() {
                findings.push(Finding::new(
                    &contract.name,
                    format!("{id} is approved with no `decided` date"),
                ));
            }
        }

        if contract.status.settled() {
            for (id, row) in &entry.rows {
                if row.verdict != "approved" {
                    findings.push(Finding::new(
                        &contract.name,
                        format!(
                            "is `{}` while {id} is still `{}` — a contract settles only when \
                             every row is approved",
                            contract.status.label(),
                            row.verdict
                        ),
                    ));
                }
            }
            if !contract.questions_closed {
                findings.push(Finding::new(
                    &contract.name,
                    format!(
                        "is `{}` with open questions still listed",
                        contract.status.label()
                    ),
                ));
            }
        }
    }

    for name in decisions.keys() {
        if !contracts.iter().any(|c| &c.name == name) {
            findings.push(Finding::new(
                name,
                "is in decisions.json with no contract file",
            ));
        }
    }

    findings
}

/// Renders the audit the way the command prints it.
pub fn render(audit: &Audit) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} dimensions in the registry ({} tool-scoped, {} portal-scoped, {} shared)\n",
        audit.registry.dims.len(),
        audit
            .registry
            .dims
            .iter()
            .filter(|d| d.scope == Scope::Tool)
            .count(),
        audit
            .registry
            .dims
            .iter()
            .filter(|d| d.scope == Scope::Portal)
            .count(),
        audit
            .registry
            .dims
            .iter()
            .filter(|d| d.scope == Scope::Any)
            .count(),
    ));
    for contract in &audit.contracts {
        let counts = audit
            .decisions
            .get(&contract.name)
            .map(|entry| {
                let approved = entry
                    .rows
                    .iter()
                    .filter(|(_, r)| r.verdict == "approved")
                    .count();
                let proposed = entry
                    .rows
                    .iter()
                    .filter(|(_, r)| r.verdict == "proposed")
                    .count();
                let rejected = entry
                    .rows
                    .iter()
                    .filter(|(_, r)| r.verdict == "rejected")
                    .count();
                format!("{approved} approved, {proposed} proposed, {rejected} rejected")
            })
            .unwrap_or_else(|| "no decisions entry".to_string());
        out.push_str(&format!(
            "{:<26} {:<7} {:<8} {:>2} rows · {}\n",
            contract.name,
            contract.family.label(),
            contract.status.label(),
            contract.rows.len(),
            counts,
        ));
    }
    if audit.findings.is_empty() {
        out.push_str("contracts, registry, and decisions.json agree\n");
    } else {
        for finding in &audit.findings {
            out.push_str(&format!("  {}: {}\n", finding.contract, finding.message));
        }
    }
    out
}

fn is_dimension_id(cell: &str) -> bool {
    let bytes = cell.as_bytes();
    bytes.len() == 3 && bytes[0] == b'D' && bytes[1].is_ascii_digit() && bytes[2].is_ascii_digit()
}

/// Reads `Field: value` from the contract header, returning the first word of
/// the value with markdown emphasis stripped (`**shipped** (…)` → `shipped`).
fn header_field(text: &str, field: &str) -> Option<String> {
    let prefix = format!("{field}:");
    let line = text
        .lines()
        .take_while(|l| !l.starts_with("## "))
        .find(|l| l.starts_with(&prefix))?;
    let value = line[prefix.len()..].trim();
    let word: String = value
        .trim_start_matches('*')
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    (!word.is_empty()).then_some(word)
}

fn open_questions_closed(text: &str) -> bool {
    let mut lines = text.lines().skip_while(|l| {
        let l = l.trim_end();
        !(l.starts_with("## ") && l.to_ascii_lowercase().contains("open question"))
    });
    lines.next();
    lines
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim_start().starts_with("None"))
        .unwrap_or(true)
}

/// Finds the first markdown table whose header holds every named column, and
/// returns its header cells plus its data rows.
fn table_after(text: &str, required: &[&str]) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !is_row(line) {
            continue;
        }
        let header = columns(line);
        if !required.iter().all(|c| header.iter().any(|h| h == c)) {
            continue;
        }
        let mut body = Vec::new();
        for line in &lines[i + 1..] {
            if !is_row(line) {
                break;
            }
            let cells = columns(line);
            if cells.iter().all(|c| is_separator(c)) {
                continue;
            }
            body.push(cells);
        }
        return Some((header, body));
    }
    None
}

fn column_index(header: &[String], name: &str) -> Option<usize> {
    header.iter().position(|c| c == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &str = "\
| ID  | Dimension | Question | Scope | Introduced by |\n\
|-----|-----------|----------|-------|---------------|\n\
| D01 | Arming | how? | any | line |\n\
| D02 | Cancel | what? | tool | line |\n\
| D03 | Export | what? | portal | portal-lens-repository |\n";

    fn registry() -> Registry {
        parse_registry(REGISTRY, Path::new("DIMENSIONS.md")).expect("registry parses")
    }

    fn contract(family: &str, status: &str, rows: &[&str], questions: &str) -> Contract {
        let matrix: String = rows
            .iter()
            .map(|id| format!("| {id} | x | y | guess | 50 |\n"))
            .collect();
        let text = format!(
            "# X\n\nStatus: **{status}**\nFamily: {family}\n\n## Behavior matrix\n\n\
             | ID | Dimension | Agreed behavior | Source | Conf |\n|---|---|---|---|---|\n\
             {matrix}\n## Open questions\n\n{questions}\n"
        );
        parse_contract(&text, Path::new("x.md")).expect("contract parses")
    }

    fn decisions(family: &str, rows: &[(&str, &str)]) -> BTreeMap<String, DecisionEntry> {
        let body: Vec<String> = rows
            .iter()
            .map(|(id, verdict)| {
                let decided = if *verdict == "approved" {
                    "\"2026-07-30\""
                } else {
                    "null"
                };
                format!(
                    "\"{id}\": {{ \"behavior\": \"b\", \"source\": \"guess\", \
                     \"confidence\": 50, \"verdict\": \"{verdict}\", \"decided\": {decided} }}"
                )
            })
            .collect();
        let text = format!(
            "{{ \"tools\": {{ \"x\": {{ \"family\": \"{family}\", \"decisions\": {{ {} }} }} }} }}",
            body.join(", ")
        );
        parse_decisions(&text, Path::new("decisions.json")).expect("decisions parse")
    }

    #[test]
    fn scope_selects_the_rows_a_family_must_answer() {
        let registry = registry();
        let tool: Vec<&str> = registry
            .in_scope(Family::Tool)
            .iter()
            .map(|d| d.id.as_str())
            .collect();
        let portal: Vec<&str> = registry
            .in_scope(Family::Portal)
            .iter()
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(tool, ["D01", "D02"]);
        assert_eq!(portal, ["D01", "D03"]);
    }

    #[test]
    fn a_registry_gap_is_a_parse_error() {
        let broken = REGISTRY.replace("| D02 |", "| D04 |");
        let err = parse_registry(&broken, Path::new("DIMENSIONS.md")).expect_err("must fail");
        assert!(err.to_string().contains("breaks the sequence"), "{err}");
    }

    #[test]
    fn a_missing_family_line_is_a_parse_error() {
        let text = "# X\n\nStatus: draft\n\n## Behavior matrix\n\n\
                    | ID | Dimension |\n|---|---|\n| D01 | x |\n";
        let err = parse_contract(text, Path::new("x.md")).expect_err("must fail");
        assert!(err.to_string().contains("Family:"), "{err}");
    }

    #[test]
    fn a_complete_contract_is_silent() {
        let findings = check(
            &registry(),
            &[contract("portal", "draft", &["D01", "D03"], "1. one open")],
            &decisions("portal", &[("D01", "proposed"), ("D03", "proposed")]),
        );
        assert_eq!(findings, Vec::new());
    }

    #[test]
    fn silence_on_an_in_scope_dimension_is_reported() {
        let findings = check(
            &registry(),
            &[contract("portal", "draft", &["D01"], "None")],
            &decisions("portal", &[("D01", "proposed")]),
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("misses D03"), "{findings:?}");
    }

    #[test]
    fn an_out_of_scope_row_is_reported() {
        let findings = check(
            &registry(),
            &[contract("portal", "draft", &["D01", "D02", "D03"], "None")],
            &decisions(
                "portal",
                &[
                    ("D01", "proposed"),
                    ("D02", "proposed"),
                    ("D03", "proposed"),
                ],
            ),
        );
        assert!(
            findings.iter().any(|f| f
                .message
                .contains("answers D02, which the registry does not scope")),
            "{findings:?}"
        );
    }

    #[test]
    fn agreed_needs_every_row_approved_and_no_open_questions() {
        let findings = check(
            &registry(),
            &[contract(
                "portal",
                "agreed",
                &["D01", "D03"],
                "1. still open",
            )],
            &decisions("portal", &[("D01", "approved"), ("D03", "proposed")]),
        );
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("still `proposed`")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("open questions still listed")),
            "{findings:?}"
        );
    }

    #[test]
    fn the_matrix_and_the_database_must_hold_the_same_rows() {
        let findings = check(
            &registry(),
            &[contract("portal", "draft", &["D01", "D03"], "None")],
            &decisions("portal", &[("D01", "proposed")]),
        );
        assert!(
            findings.iter().any(|f| f
                .message
                .contains("D03 is in the matrix but not in decisions.json")),
            "{findings:?}"
        );
    }
}
