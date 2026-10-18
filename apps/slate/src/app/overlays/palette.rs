//! Canvas command entry and quote-to-type adapter.
use super::*;

impl SlateApp {
    // ---------- canvas palette (double-click empty board) ----------

    /// Open the palette at a screen point, remembering the world point under
    /// it (placeables place there — the Grasshopper gesture).
    pub(crate) fn open_board_palette(&mut self, screen: Pos2, world: Pos2) {
        self.palette_state.open_at(screen, world);
        self.refresh_palette_items();
        self.push_history(CommandId("board.palette"), None);
    }

    /// Type-to-command: open the canvas palette with a seeded query at the
    /// pointer (or canvas center). Same UI as double-click empty board.
    pub(crate) fn open_command_entry(&mut self, ctx: &egui::Context, query: String) {
        let screen = ctx
            .input(|i| i.pointer.hover_pos())
            .filter(|p| self.canvas_rect.contains(*p))
            .unwrap_or_else(|| self.canvas_rect.center());
        let world = self.screen_to_world(screen);
        self.palette_state.open_with_query(screen, world, query);
        self.refresh_palette_items();
        self.push_history(CommandId("board.palette"), Some("type".into()));
    }

    fn refresh_palette_items(&mut self) {
        let ctx = self.command_ctx();
        // Grasshopper: a lone quote is the panel entry, a blank text document.
        if matches!(self.palette_state.query.as_str(), "\"" | "'") {
            let id = CommandId("board.media.text_new");
            self.palette_items = self
                .registry
                .by_id(id)
                .filter(|spec| spec.when.matches(ctx))
                .map(|spec| PaletteItem {
                    id,
                    name: spec.name,
                    score: f32::MAX,
                })
                .into_iter()
                .collect();
            return;
        }
        self.palette_items = palette_query(&self.registry, ctx, &self.palette_state.query);
        // Overlays spec: on an empty query, placeables (frame, text,
        // shapes…) rank above general commands. Stable partition keeps the
        // alphabetical order inside each section.
        if self.palette_state.query.trim().is_empty() {
            self.palette_items
                .sort_by_key(|it| !it.id.0.starts_with("board.tool."));
        }
    }

    /// Frame step for the palette overlay; call once per frame after the
    /// canvas so the popup floats above it.
    pub(crate) fn palette_frame(&mut self, ctx: &egui::Context) {
        if !self.palette_state.open {
            return;
        }
        let typed_panel = self
            .palette_state
            .query
            .strip_prefix(['\'', '"'])
            .filter(|text| !text.is_empty())
            .map(str::to_owned);
        if let Some(text) = typed_panel {
            let world = self.palette_state.world;
            self.palette_state.open = false;
            if self.place_text_document_at(world) {
                if let Some(edit) = self.text_doc_edit.as_mut() {
                    edit.buffer = text;
                    edit.auto_size = true;
                }
            }
            return;
        }
        let rows: Vec<PaletteRow> = self
            .palette_items
            .iter()
            .map(|it| PaletteRow {
                label: it.name.to_string(),
                hint: self
                    .registry
                    .by_id(it.id)
                    .map(|s| s.binding.split(&[',', '—', '('][..]).next().unwrap_or(""))
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            })
            .collect();
        match palette_ui(ctx, &mut self.palette_state, &rows) {
            PaletteAction::None => {}
            PaletteAction::Dismiss => {}
            PaletteAction::QueryChanged => self.refresh_palette_items(),
            PaletteAction::Execute(i) => {
                if let Some(item) = self.palette_items.get(i).cloned() {
                    self.palette_execute(ctx, item);
                }
            }
        }
    }

    /// Execute a palette row. Area tools with a click-to-place default
    /// (frame, rect, ellipse, portals, text, sticky) place at the stored
    /// world point immediately; line / pen / brush still arm.
    fn palette_execute(&mut self, ctx: &egui::Context, item: PaletteItem) {
        let world = self.palette_state.world;
        match item.id.0 {
            "board.tool.rect" => {
                self.place_default_at(crate::app::board::BoardTool::RectShape, world);
            }
            "board.tool.ellipse" => {
                self.place_default_at(crate::app::board::BoardTool::Ellipse, world);
            }
            "board.tool.frame" => {
                self.place_frame_at(world);
                self.push_history(item.id, Some("placed".into()));
            }
            "board.portal.web" => {
                self.place_web_portal_at(world);
            }
            "board.portal.agent" => {
                self.place_agent_portal_at(world);
            }
            "board.portal.slate" => {
                self.place_slate_portal_at(world);
            }
            "board.tool.text" => {
                self.place_text_at(world);
            }
            "board.tool.sticky" => {
                self.place_sticky_at(world);
            }
            "board.media.text_new" => {
                self.place_text_document_at(world);
            }
            _ => {
                self.dispatch(ctx, item.id, None);
            }
        }
    }
}
