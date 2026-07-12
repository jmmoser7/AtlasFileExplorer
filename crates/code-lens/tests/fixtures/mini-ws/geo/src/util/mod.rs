use crate::util;
use super::Thing;
use self::local;

mod local;

pub fn helper() -> local::LocalThing {
    local::LocalThing
}
