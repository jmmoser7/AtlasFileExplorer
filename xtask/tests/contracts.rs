//! The guarantee the contract audit has to keep: the three hand-authored
//! artifacts in `docs/keymap/contracts/` agree with each other, in the tree as
//! it is committed. A contract that stops answering a dimension, or a decision
//! row that loses its contract, fails here rather than in six months when
//! somebody builds from a matrix with a hole in it.

use std::path::{Path, PathBuf};

use xtask::contracts::{Family, Status};
use xtask::{audit_contracts, render_contract_audit};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sits directly under the workspace root")
        .to_path_buf()
}

#[test]
fn committed_contracts_agree_with_registry_and_decisions() {
    let audit = audit_contracts(&workspace_root()).expect("the contract artifacts parse");
    assert!(
        audit.findings.is_empty(),
        "contract audit findings:\n{}",
        render_contract_audit(&audit)
    );
}

#[test]
fn every_contract_answers_every_dimension_in_its_scope() {
    let audit = audit_contracts(&workspace_root()).expect("the contract artifacts parse");
    assert!(
        !audit.contracts.is_empty(),
        "no contracts found — the audit would pass vacuously"
    );
    for contract in &audit.contracts {
        let expected = audit.registry.in_scope(contract.family).len();
        assert_eq!(
            contract.rows.len(),
            expected,
            "`{}` answers {} of {expected} dimensions scoped to {}",
            contract.name,
            contract.rows.len(),
            contract.family.label(),
        );
    }
}

#[test]
fn a_draft_contract_carries_only_proposed_or_decided_rows() {
    let audit = audit_contracts(&workspace_root()).expect("the contract artifacts parse");
    for contract in &audit.contracts {
        let entry = audit
            .decisions
            .get(&contract.name)
            .unwrap_or_else(|| panic!("`{}` has a decisions entry", contract.name));
        if contract.status == Status::Draft {
            assert!(
                entry.rows.iter().any(|(_, r)| r.verdict == "proposed"),
                "`{}` is a draft with nothing left to decide — it should be agreed",
                contract.name
            );
        }
    }
}

#[test]
fn the_first_portal_contract_is_present_and_generated_class() {
    let audit = audit_contracts(&workspace_root()).expect("the contract artifacts parse");
    let portal = audit
        .contracts
        .iter()
        .find(|c| c.name == "portal-lens-repository")
        .expect("the repository Lens portal contract exists");
    assert_eq!(portal.family, Family::Portal);
    assert_eq!(
        portal.rows.len(),
        audit.registry.in_scope(Family::Portal).len()
    );
}
