//! The single source of truth for colors across the Atlas ecosystem.
//!
//! Both File Atlas and Slate build their visuals from [`Palette`], whose slots
//! are data: the `[theme.light]` / `[theme.dark]` tables in `ui-tokens.toml`,
//! shadowed by any `[theme.<name>]` table dropped into [`user_theme_dir`].
//! egui's own `Visuals` are derived from that same palette
//! ([`Palette::visuals`]), so a theme cannot be half-applied. Apps must not
//! define their own color constants for chrome — divergence between the apps
//! is a bug.

use crate::sidebar::SidebarTheme;
use crate::tokens::{self, Hex, ThemeSlots};
use eframe::egui::{self, Color32};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// egui visuals for the `dark` theme.
pub fn dark_visuals() -> egui::Visuals {
    Palette::dark().visuals()
}

/// egui visuals for the `light` theme.
pub fn light_visuals() -> egui::Visuals {
    Palette::light().visuals()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Which egui widget base the slots below are painted over.
    pub dark_mode: bool,
    pub bg: Color32,
    pub grid_dot: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub ink: Color32,
    pub sub: Color32,
    pub line: Color32,
    pub accent: Color32,
    pub portal: Color32,
    pub thumb_bg: Color32,
    pub select: Color32,
    pub staged: Color32,
    /// `Visuals::panel_fill`.
    pub panel: Color32,
    /// `Visuals::window_fill`.
    pub window: Color32,
    /// `Visuals::extreme_bg_color`.
    pub extreme_bg: Color32,
    /// `Visuals::selection.bg_fill` — egui's own selection highlight, which is
    /// not the canvas selection colour [`Palette::select`].
    pub select_fill: Color32,
    /// `Visuals::selection.stroke.color`.
    pub select_stroke: Color32,
}

impl Palette {
    pub fn light() -> Self {
        Self::named("light")
    }

    pub fn dark() -> Self {
        Self::named("dark")
    }

    pub fn for_mode(dark_mode: bool) -> Self {
        if dark_mode {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// The theme called `name`: a user theme of that name if one is installed,
    /// otherwise the built-in tokens. An unknown name falls back to `dark`.
    pub fn named(name: &str) -> Self {
        match user_themes().get(name) {
            Some(slots) => Self::from_slots(slots),
            None => Self::builtin(name != "light"),
        }
    }

    /// egui's visuals for this theme.
    ///
    /// Every colour egui fills comes from a slot above, so applying a theme is
    /// one call and can never be partial.
    pub fn visuals(&self) -> egui::Visuals {
        let mut visuals = if self.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = self.panel;
        visuals.window_fill = self.window;
        visuals.extreme_bg_color = self.extreme_bg;
        visuals.selection.bg_fill = self.select_fill;
        visuals.selection.stroke.color = self.select_stroke;
        visuals
    }

    /// Theme subset used by the sidebar layout primitives.
    pub fn sidebar_theme(&self) -> SidebarTheme {
        SidebarTheme {
            card: self.card,
            border: self.border,
            ink: self.ink,
            sub: self.sub,
        }
    }

    /// The checked-in theme, ignoring any user theme of the same name.
    fn builtin(dark_mode: bool) -> Self {
        let tokens = tokens::current();
        Self::from_slots(if dark_mode {
            &tokens.theme.dark
        } else {
            &tokens.theme.light
        })
    }

    fn from_slots(slots: &ThemeSlots) -> Self {
        Self {
            dark_mode: slots.dark_base,
            bg: slots.bg.color(),
            grid_dot: slots.grid_dot.color(),
            card: slots.card.color(),
            card_hover: slots.card_hover.color(),
            border: slots.border.color(),
            border_strong: slots.border_strong.color(),
            ink: slots.ink.color(),
            sub: slots.sub.color(),
            line: slots.line.color(),
            accent: slots.accent.color(),
            portal: slots.portal.color(),
            thumb_bg: slots.thumb_bg.color(),
            select: slots.select.color(),
            staged: slots.staged.color(),
            panel: slots.panel.color(),
            window: slots.window.color(),
            extreme_bg: slots.extreme_bg.color(),
            select_fill: slots.select_fill.color(),
            select_stroke: slots.select_stroke.color(),
        }
    }
}

/// Directory scanned for user themes, beside the chrome preferences: every
/// `.toml` file in it may hold one or more `[theme.<name>]` tables shaped like
/// `ui-tokens.toml`'s. `crates/atlas-shell/themes/` holds a worked example.
pub fn user_theme_dir() -> PathBuf {
    atlas_core::index::data_dir().join("themes")
}

/// Installed user themes by name. Read once — this sits behind every frame's
/// [`Palette::for_mode`].
fn user_themes() -> &'static BTreeMap<String, ThemeSlots> {
    static THEMES: OnceLock<BTreeMap<String, ThemeSlots>> = OnceLock::new();
    THEMES.get_or_init(|| read_theme_dir(&user_theme_dir()))
}

fn read_theme_dir(dir: &Path) -> BTreeMap<String, ThemeSlots> {
    let mut themes = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return themes;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        })
        .collect();
    files.sort();
    for file in files {
        read_theme_file(&file, &mut themes);
    }
    themes
}

fn read_theme_file(path: &Path, out: &mut BTreeMap<String, ThemeSlots>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("theme file {} could not be read; skipped", path.display());
        return;
    };
    let value = match text.parse::<toml::Value>() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("theme file {} is not valid TOML ({error})", path.display());
            return;
        }
    };
    let Some(tables) = value.get("theme").and_then(toml::Value::as_table) else {
        eprintln!("theme file {} has no [theme.<name>] table", path.display());
        return;
    };
    for (name, table) in tables {
        match table.as_table() {
            Some(table) => {
                out.insert(name.clone(), slots_from_table(name, table, path));
            }
            None => eprintln!("theme {name} in {} is not a table", path.display()),
        }
    }
}

/// Unknown, malformed, and missing slots warn once and keep the built-in dark
/// value; a hand-written theme never panics and never fails a launch.
fn slots_from_table(name: &str, table: &toml::Table, path: &Path) -> ThemeSlots {
    let mut slots = ThemeSlots::default();
    for (key, value) in table {
        if key == "dark_base" {
            match value.as_bool() {
                Some(flag) => slots.dark_base = flag,
                None => eprintln!("theme {name}: dark_base is not a boolean"),
            }
            continue;
        }
        let Some(slot) = slots.slot_mut(key) else {
            eprintln!("theme {name}: unknown slot {key} ({})", path.display());
            continue;
        };
        match value.as_str().and_then(Hex::parse) {
            Some(hex) => *slot = hex,
            None => eprintln!("theme {name}: slot {key} is not a #rrggbb colour"),
        }
    }
    let missing = ThemeSlots::SLOTS
        .iter()
        .filter(|slot| !table.contains_key(**slot))
        .count();
    if missing > 0 {
        eprintln!("theme {name}: {missing} slot(s) unset; using built-in dark values");
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("atlas_theme_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The colours as they were written in Rust before the palette became
    /// data. Nothing about this change may move a pixel.
    #[test]
    fn palette_from_tokens_matches_previous_constants() {
        let light = Palette::builtin(false);
        assert_eq!(light.bg, Color32::from_rgb(0xf6, 0xf7, 0xf8));
        assert_eq!(light.grid_dot, Color32::from_rgb(0xdf, 0xe3, 0xe7));
        assert_eq!(light.card, Color32::WHITE);
        assert_eq!(light.card_hover, Color32::from_rgb(0xfb, 0xfc, 0xfd));
        assert_eq!(light.border, Color32::from_rgb(0xdf, 0xe3, 0xe8));
        assert_eq!(light.border_strong, Color32::from_rgb(0xc7, 0xcd, 0xd4));
        assert_eq!(light.ink, Color32::from_rgb(0x1b, 0x1e, 0x22));
        assert_eq!(light.sub, Color32::from_rgb(0x87, 0x8e, 0x96));
        assert_eq!(light.line, Color32::from_rgb(0xcb, 0xd1, 0xd8));
        assert_eq!(light.accent, Color32::from_rgb(0x0f, 0x76, 0x6e));
        assert_eq!(light.portal, Color32::from_rgb(0x8b, 0x5c, 0xf6));
        assert_eq!(light.thumb_bg, Color32::from_rgb(0xee, 0xf0, 0xf2));
        assert_eq!(light.select, Color32::from_rgb(0x1f, 0x6f, 0xb2));
        assert_eq!(light.staged, Color32::from_rgb(0xc4, 0x84, 0x1d));

        let dark = Palette::builtin(true);
        assert_eq!(dark.bg, Color32::from_rgb(0x0e, 0x10, 0x13));
        assert_eq!(dark.grid_dot, Color32::from_rgb(0x23, 0x27, 0x2d));
        assert_eq!(dark.card, Color32::from_rgb(0x1c, 0x20, 0x26));
        assert_eq!(dark.card_hover, Color32::from_rgb(0x24, 0x29, 0x31));
        assert_eq!(dark.border, Color32::from_rgb(0x33, 0x39, 0x41));
        assert_eq!(dark.border_strong, Color32::from_rgb(0x4a, 0x52, 0x5c));
        assert_eq!(dark.ink, Color32::from_rgb(0xdd, 0xe2, 0xe8));
        assert_eq!(dark.sub, Color32::from_rgb(0x87, 0x8e, 0x96));
        assert_eq!(dark.line, Color32::from_rgb(0x3a, 0x41, 0x4a));
        assert_eq!(dark.accent, Color32::from_rgb(0x2d, 0xd4, 0xbf));
        assert_eq!(dark.portal, Color32::from_rgb(0xa7, 0x8b, 0xfa));
        assert_eq!(dark.thumb_bg, Color32::from_rgb(0x15, 0x18, 0x1c));
        assert_eq!(dark.select, Color32::from_rgb(0x6f, 0xb7, 0xff));
        assert_eq!(dark.staged, Color32::from_rgb(0xe0, 0xa8, 0x3c));
    }

    /// `dark_visuals` / `light_visuals` as they were written before they were
    /// derived from the palette.
    #[test]
    fn visuals_match_previous_constants() {
        let mut expected = egui::Visuals::dark();
        expected.panel_fill = Color32::from_rgb(0x14, 0x16, 0x1a);
        expected.window_fill = Color32::from_rgb(0x1a, 0x1d, 0x23);
        expected.extreme_bg_color = Color32::from_rgb(0x0e, 0x10, 0x13);
        expected.selection.bg_fill = Color32::from_rgb(0x2b, 0x5c, 0x8a);
        assert_eq!(Palette::builtin(true).visuals(), expected);

        let mut expected = egui::Visuals::light();
        expected.panel_fill = Color32::from_rgb(0xf8, 0xf9, 0xfb);
        expected.window_fill = Color32::WHITE;
        expected.extreme_bg_color = Color32::from_rgb(0xee, 0xf0, 0xf2);
        expected.selection.bg_fill = Color32::from_rgb(0xd7, 0xe8, 0xff);
        expected.selection.stroke.color = Color32::from_rgb(0x1f, 0x6f, 0xb2);
        assert_eq!(Palette::builtin(false).visuals(), expected);
    }

    #[test]
    fn visuals_are_derived_from_palette() {
        let mut palette = Palette::builtin(true);
        palette.panel = Color32::from_rgb(1, 2, 3);
        palette.window = Color32::from_rgb(4, 5, 6);
        palette.extreme_bg = Color32::from_rgb(7, 8, 9);
        palette.select_fill = Color32::from_rgb(10, 11, 12);
        palette.select_stroke = Color32::from_rgb(13, 14, 15);

        let visuals = palette.visuals();
        assert_eq!(visuals.panel_fill, palette.panel);
        assert_eq!(visuals.window_fill, palette.window);
        assert_eq!(visuals.extreme_bg_color, palette.extreme_bg);
        assert_eq!(visuals.selection.bg_fill, palette.select_fill);
        assert_eq!(visuals.selection.stroke.color, palette.select_stroke);

        palette.dark_mode = false;
        assert!(!palette.visuals().dark_mode);
    }

    #[test]
    fn user_theme_file_overrides_builtin() {
        let dir = scratch_dir("override");
        std::fs::write(
            dir.join("mine.toml"),
            "[theme.dark]\ndark_base = true\nbg = \"#010203\"\naccent = \"#ff0000\"\n",
        )
        .unwrap();

        let themes = read_theme_dir(&dir);
        let dark = Palette::from_slots(themes.get("dark").unwrap());
        assert_eq!(dark.bg, Color32::from_rgb(1, 2, 3));
        assert_eq!(dark.accent, Color32::from_rgb(0xff, 0, 0));
        // Slots the file left out keep the built-in dark values.
        assert_eq!(dark.staged, Palette::builtin(true).staged);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_theme_slot_falls_back_without_panicking() {
        let dir = scratch_dir("unknown");
        std::fs::write(
            dir.join("broken.toml"),
            concat!(
                "[theme.broken]\n",
                "dark_base = \"yes please\"\n",
                "not_a_slot = \"#123456\"\n",
                "bg = \"rebeccapurple\"\n",
                "accent = 17\n",
                "ink = \"#00ff00\"\n",
            ),
        )
        .unwrap();
        std::fs::write(dir.join("garbage.toml"), "this is not toml = = =\n").unwrap();
        std::fs::write(dir.join("empty.toml"), "unrelated = 1\n").unwrap();
        std::fs::write(dir.join("ignored.txt"), "[theme.dark]\nbg = \"#000000\"\n").unwrap();

        let themes = read_theme_dir(&dir);
        assert_eq!(themes.len(), 1, "only the parsable theme is installed");
        let broken = Palette::from_slots(themes.get("broken").unwrap());
        let builtin = Palette::builtin(true);
        assert_eq!(broken.ink, Color32::from_rgb(0, 0xff, 0));
        assert_eq!(broken.bg, builtin.bg);
        assert_eq!(broken.accent, builtin.accent);
        assert!(broken.dark_mode);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The committed example is the documentation for the theme format, so it
    /// has to stay loadable and complete as slots are added.
    #[test]
    fn example_theme_declares_every_slot() {
        let dir = scratch_dir("example");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("themes")
            .join("example-high-contrast.toml");
        std::fs::copy(&source, dir.join("example-high-contrast.toml")).unwrap();

        let themes = read_theme_dir(&dir);
        assert!(themes.contains_key("high-contrast"));

        let text = std::fs::read_to_string(&source).unwrap();
        let value = text.parse::<toml::Value>().unwrap();
        let table = value["theme"]["high-contrast"].as_table().unwrap();
        for name in ThemeSlots::SLOTS {
            assert!(table.contains_key(*name), "{name} missing from the example");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
