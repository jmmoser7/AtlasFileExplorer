//! A source graph fixture; these comments are parsed metadata.
pub mod helper;
pub use helper::run as renamed;

pub trait Execute { fn execute(&self); }
pub struct Worker;
impl Execute for Worker { fn execute(&self) { self.finish(); } }
impl Worker {
    /// Start then finish.
    pub fn start(&self) { self.finish(); Self::finish(self); renamed(); }
    fn finish(&self) {}
}

pub fn local() {}
pub fn shadow(run: fn()) { run(); }
pub fn variable_shadow() { let renamed = || {}; renamed(); }

mod inner {
    pub fn nested() { super::renamed(); }
}

mod pieces {
    use super::*;
    impl Worker { fn from_piece(&self) { self.finish(); } }
}

pub fn outer() {
    fn nested_function() { crate::helper::run(); }
    nested_function();
}

#[cfg(feature = "a")]
fn duplicate() {}
#[cfg(feature = "b")]
fn duplicate() {}
#[cfg(feature = "c")]
fn duplicate() {}
fn ambiguous() { duplicate(); }

mod reexports {
    #[cfg(feature = "a")]
    pub use crate::helper::run as choice;
    #[cfg(feature = "b")]
    pub use crate::unknown::run as choice;
    pub fn collision() {}
    #[cfg(feature = "a")]
    pub use crate::helper::run as collision;
}
pub fn ambiguous_reexport() { crate::reexports::choice(); }
pub fn ambiguous_definition_reexport() { crate::reexports::collision(); }
pub fn generic_receiver<Worker>() { Worker::finish(); }
pub fn block_alias() { { use crate::helper::run as scoped; scoped(); } scoped(); }
pub fn block_glob() { { use crate::helper::*; } run(); }
mod duplicate_prefix {
    #[cfg(feature = "a")]
    mod branch { pub fn known() {} }
    #[cfg(feature = "b")]
    mod branch { pub fn other() {} }
}
pub fn ambiguous_module_prefix() { crate::duplicate_prefix::branch::known(); }
