//! # circle-pack
//!
//! Pure 2D circle packing and Venn-diagram layout for the Slate presentation mode.
//! No UI dependencies — all coordinates are `f32` in arbitrary pixel space.
//!
//! ## Quick start
//!
//! ```
//! use circle_pack::{pack_in_circle, venn_layout, VennItem, VennSet};
//!
//! // Pack thumbnail circles inside a tag region
//! let packing = pack_in_circle(&[4.0, 4.0, 4.0, 4.0]);
//! assert_eq!(packing.circles.len(), 4);
//!
//! // Lay out overlapping tag circles and item thumbnails
//! let sets = vec![
//!     VennSet { id: 1, weight: 10.0 },
//!     VennSet { id: 2, weight: 8.0 },
//! ];
//! let items = vec![
//!     VennItem { id: 100, sets: vec![1, 2], r: 3.0 },
//!     VennItem { id: 101, sets: vec![1], r: 3.0 },
//! ];
//! let layout = venn_layout(&sets, &items);
//! assert_eq!(layout.set_circles.len(), 2);
//! ```

mod geometry;
mod pack;
mod venn;

pub use geometry::{lens_area, Circle};
pub use pack::{pack_in_circle, Packing};
pub use venn::{venn_layout, VennItem, VennLayout, VennSet};
