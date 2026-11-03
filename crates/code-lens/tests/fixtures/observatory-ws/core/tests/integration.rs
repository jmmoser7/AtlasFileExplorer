fn local() {}
#[test]
fn integration_scope() { crate::local(); fixture_core::renamed(); }
