use core_lib::Render;

pub struct Thing;

impl Render for Thing {
    fn draw(&self) {}
}

// Unresolvable intra-package path → Use edge targets the geo package node.
use crate::nonexistent::Missing;

pub mod util;
