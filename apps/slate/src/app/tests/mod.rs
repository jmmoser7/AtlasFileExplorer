//! Headless Slate stability tests: drive the real frame loop through a plain
//! `egui::Context` (no eframe window) with the real thumbnail pool, exercising
//! the tag model, both presentations, tabs, and workbook save/load.

use super::{board_align, board_handles, board_place, board_wire, *};
use eframe::egui::{Pos2, Rect as ERect, Vec2 as EVec2};
use slate_doc::scene::{FrameNode, NodeKind, Rgba, WorldRect};
use slate_doc::{NodeId, ViewKind};

mod support;

pub(super) use support::*;

mod agent;
mod align;
mod app_smoke;
mod arming;
mod bbox;
mod bezier;
mod bezier_close;
mod board_canvas;
mod brush;
mod clipboard;
mod closed_form;
mod crop;
mod curve_grips;
mod eraser;
mod escape_drag;
mod fillet;
mod hit_order;
mod image_paint;
mod join;
mod kit_recipes;
mod kits_atlas;
mod line;
mod media;
mod media_guards;
mod osnap;
mod page_focus;
mod path_cuts;
mod path_edit;
mod previews;
mod review;
mod selection_chrome;
mod sheet;
mod split_gp;
mod stroke;
mod style;
mod subobject;
mod text;
mod tip_direct;
mod tip_hud;
mod tip_numeric;
mod trim;
mod vertex_color;
mod vertex_drag;
mod vertex_style;
mod vertex_width;
mod visual_frames;
mod web;
mod wire_cursor;
mod wire_grips;
mod wires;
