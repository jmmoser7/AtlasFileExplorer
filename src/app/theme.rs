//! Theme: egui visuals plus the canvas color palette.
//!
//! `Palette` is the single source of canvas/chrome colors — take colors from
//! `AtlasApp::palette()` rather than hardcoding, so light/dark stay in sync.

use eframe::egui::{self, Color32};

pub(crate) fn dark_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(0x14, 0x16, 0x1a);
    v.window_fill = Color32::from_rgb(0x1a, 0x1d, 0x23);
    v.extreme_bg_color = Color32::from_rgb(0x0e, 0x10, 0x13);
    v.selection.bg_fill = Color32::from_rgb(0x2b, 0x5c, 0x8a);
    v
}

pub(crate) fn light_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.panel_fill = Color32::from_rgb(0xf8, 0xf9, 0xfb);
    v.window_fill = Color32::WHITE;
    v.extreme_bg_color = Color32::from_rgb(0xee, 0xf0, 0xf2);
    v.selection.bg_fill = Color32::from_rgb(0xd7, 0xe8, 0xff);
    v.selection.stroke.color = Color32::from_rgb(0x1f, 0x6f, 0xb2);
    v
}

#[derive(Clone, Copy)]
pub(in crate::app) struct Palette {
    pub(in crate::app) bg: Color32,
    pub(in crate::app) grid_dot: Color32,
    pub(in crate::app) card: Color32,
    pub(in crate::app) card_hover: Color32,
    pub(in crate::app) border: Color32,
    pub(in crate::app) border_strong: Color32,
    pub(in crate::app) ink: Color32,
    pub(in crate::app) sub: Color32,
    pub(in crate::app) line: Color32,
    pub(in crate::app) accent: Color32,
    pub(in crate::app) portal: Color32,
    pub(in crate::app) thumb_bg: Color32,
    pub(in crate::app) select: Color32,
    pub(in crate::app) staged: Color32,
}

impl Palette {
    pub(in crate::app) fn light() -> Self {
        Self {
            bg: Color32::from_rgb(0xf6, 0xf7, 0xf8),
            grid_dot: Color32::from_rgb(0xdf, 0xe3, 0xe7),
            card: Color32::WHITE,
            card_hover: Color32::from_rgb(0xfb, 0xfc, 0xfd),
            border: Color32::from_rgb(0xdf, 0xe3, 0xe8),
            border_strong: Color32::from_rgb(0xc7, 0xcd, 0xd4),
            ink: Color32::from_rgb(0x1b, 0x1e, 0x22),
            sub: Color32::from_rgb(0x87, 0x8e, 0x96),
            line: Color32::from_rgb(0xcb, 0xd1, 0xd8),
            accent: Color32::from_rgb(0x0f, 0x76, 0x6e),
            portal: Color32::from_rgb(0x8b, 0x5c, 0xf6),
            thumb_bg: Color32::from_rgb(0xee, 0xf0, 0xf2),
            select: Color32::from_rgb(0x1f, 0x6f, 0xb2),
            staged: Color32::from_rgb(0xc4, 0x84, 0x1d),
        }
    }

    pub(in crate::app) fn dark() -> Self {
        Self {
            bg: Color32::from_rgb(0x0e, 0x10, 0x13),
            grid_dot: Color32::from_rgb(0x23, 0x27, 0x2d),
            card: Color32::from_rgb(0x1c, 0x20, 0x26),
            card_hover: Color32::from_rgb(0x24, 0x29, 0x31),
            border: Color32::from_rgb(0x33, 0x39, 0x41),
            border_strong: Color32::from_rgb(0x4a, 0x52, 0x5c),
            ink: Color32::from_rgb(0xdd, 0xe2, 0xe8),
            sub: Color32::from_rgb(0x87, 0x8e, 0x96),
            line: Color32::from_rgb(0x3a, 0x41, 0x4a),
            accent: Color32::from_rgb(0x2d, 0xd4, 0xbf),
            portal: Color32::from_rgb(0xa7, 0x8b, 0xfa),
            thumb_bg: Color32::from_rgb(0x15, 0x18, 0x1c),
            select: Color32::from_rgb(0x6f, 0xb7, 0xff),
            staged: Color32::from_rgb(0xe0, 0xa8, 0x3c),
        }
    }
}
