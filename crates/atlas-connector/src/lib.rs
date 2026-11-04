//! Public, renderer-free Slate application connector protocol.
//!
//! This crate owns wire models and admission policy, never process launch, IPC
//! authentication, file I/O, or application editing. A transport must establish
//! the caller independently and supply it to [`Session::begin`].

mod capture_meta;
mod descriptor;
mod draft_authority;
mod framing;
mod mock;
mod model;
mod params;
mod schema;
mod session;
mod session_admission;
mod session_results;
mod validation;
mod view;
mod wire;

pub use capture_meta::*;
pub use descriptor::*;
pub use draft_authority::*;
pub use framing::*;
pub use mock::*;
pub use model::*;
pub use params::*;
pub use schema::*;
pub use session::*;
pub use validation::valid_id as valid_identifier;
pub use view::*;
pub use wire::*;
