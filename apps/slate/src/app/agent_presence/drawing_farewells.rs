//! Short, visual-only departures for drawing-copy avatars.
//!
//! Drawing helpers belong to a work path, so they disappear as soon as that
//! path completes. This adapter remembers only the last painted appearance and
//! lets the shared presence painter finish the same departure used elsewhere.
use atlas_agent::presence::WorldPoint;
use atlas_shell::agent_presence as chrome;
use eframe::egui::Id;
use std::collections::HashMap;

const MAX_RECORDS: usize = 512;

#[derive(Clone)]
struct Departure {
    style: String,
    started: f64,
}

#[derive(Clone)]
struct Record {
    id: String,
    tab: u64,
    position: WorldPoint,
    hat: String,
    seen: u64,
    touched: u64,
    departure: Option<Departure>,
}

#[derive(Default)]
pub(super) struct DrawingFarewells {
    frame: u64,
    records: HashMap<Id, Record>,
}

impl DrawingFarewells {
    pub(super) fn needs_portal(&self, tab: u64) -> bool {
        self.records.values().any(|record| {
            record.tab == tab
                && record
                    .departure
                    .as_ref()
                    .map_or(record.seen != self.frame, |departure| {
                        departure.style == "portal"
                    })
        })
    }

    pub(super) fn ids(&self) -> impl Iterator<Item = &str> {
        self.records.values().map(|record| record.id.as_str())
    }

    pub(super) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1).max(1);
    }

    pub(super) fn observe(&mut self, tab: u64, id: &str, position: WorldPoint, hat: &str) {
        if !position.x.is_finite() || !position.y.is_finite() || id.is_empty() {
            return;
        }
        let key = Id::new((tab, id));
        if !self.records.contains_key(&key) {
            if self.records.len() >= MAX_RECORDS {
                self.evict_oldest();
            }
            self.records.insert(
                key,
                Record {
                    id: id.to_owned(),
                    tab,
                    position,
                    hat: hat.to_owned(),
                    seen: self.frame,
                    touched: self.frame,
                    departure: None,
                },
            );
        }
        let record = self.records.get_mut(&key).unwrap();
        record.tab = tab;
        record.position = position;
        record.hat.clear();
        record.hat.push_str(hat);
        record.seen = self.frame;
        record.touched = self.frame;
        // An avatar that is live again has no outstanding farewell.
        record.departure = None;
    }

    pub(super) fn finish_frame(
        &mut self,
        tab: u64,
        now: f64,
        portal_available: bool,
        sequence: &mut chrome::DepartureSequence,
    ) {
        if !now.is_finite() {
            return;
        }
        for record in self.records.values_mut() {
            if record.tab == tab && record.seen != self.frame && record.departure.is_none() {
                record.departure = Some(Departure {
                    style: sequence.resolve(chrome::RANDOM_DEPARTURE, portal_available),
                    started: now,
                });
                record.touched = self.frame;
            }
        }
        self.records.retain(|_, record| {
            record.departure.as_ref().is_none_or(|departure| {
                now - departure.started < chrome::departure_duration(&departure.style)
            })
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint(
        &self,
        painter: &eframe::egui::Painter,
        presence_painter: &mut chrome::PresencePainter,
        xf: &crate::app::board::BoardXf,
        tab: u64,
        now: f64,
        reduced: bool,
        canvas: eframe::egui::Rect,
        portal: Option<eframe::egui::Rect>,
    ) -> bool {
        let mut active = false;
        for record in self.records.values() {
            if record.tab != tab {
                continue;
            }
            let Some(departure) = &record.departure else {
                continue;
            };
            let progress = ((now - departure.started).max(0.0)
                / chrome::departure_duration(&departure.style)) as f32;
            let animated = presence_painter.paint_world_departure(
                painter,
                chrome::AvatarVisual {
                    id: &record.id,
                    center: xf.w2s(eframe::egui::pos2(
                        record.position.x as f32,
                        record.position.y as f32,
                    )),
                    expression: "neutral",
                    orientation: "forward",
                    hat: &record.hat,
                    prop: "",
                    opacity: 1.0,
                },
                &departure.style,
                progress,
                now,
                reduced,
                canvas,
                portal,
                xf.z,
            );
            active |= animated || progress < 1.0;
        }
        active
    }

    fn evict_oldest(&mut self) {
        if let Some(key) = self
            .records
            .iter()
            .min_by_key(|(_, record)| record.touched)
            .map(|(&key, _)| key)
        {
            self.records.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> WorldPoint {
        WorldPoint { x, y }
    }

    #[test]
    fn disappearance_keeps_last_pose_and_resolves_one_concrete_style() {
        let mut farewells = DrawingFarewells::default();
        let mut sequence = chrome::DepartureSequence::default();
        farewells.begin_frame();
        farewells.observe(7, "copy-a", point(12.0, 24.0), "cap");
        farewells.finish_frame(7, 1.0, true, &mut sequence);
        assert!(
            !farewells.needs_portal(7),
            "live copies need no portal lookup"
        );
        farewells.begin_frame();
        assert!(farewells.needs_portal(7));
        farewells.finish_frame(7, 2.0, true, &mut sequence);
        let record = &farewells.records[&Id::new((7, "copy-a"))];
        let departure = record.departure.as_ref().unwrap();
        assert_eq!(record.position, point(12.0, 24.0));
        assert_eq!(record.hat, "cap");
        assert_ne!(departure.style, chrome::RANDOM_DEPARTURE);
        let original = (departure.style.clone(), departure.started);
        farewells.finish_frame(7, 3.0, true, &mut sequence);
        let repeated = farewells.records[&Id::new((7, "copy-a"))]
            .departure
            .as_ref()
            .unwrap();
        assert_eq!((repeated.style.clone(), repeated.started), original);
    }

    #[test]
    fn tab_switch_does_not_depart_other_tab_and_expired_departure_is_pruned() {
        let mut farewells = DrawingFarewells::default();
        let mut sequence = chrome::DepartureSequence::default();
        farewells.begin_frame();
        farewells.observe(1, "one", point(1.0, 2.0), "");
        farewells.observe(2, "one", point(3.0, 4.0), "");
        assert_eq!(
            farewells.records.len(),
            2,
            "copy IDs are scoped to their board"
        );
        farewells.begin_frame();
        farewells.finish_frame(1, 0.0, false, &mut sequence);
        assert!(farewells.records[&Id::new((2, "one"))].departure.is_none());
        farewells.finish_frame(1, 100.0, false, &mut sequence);
        assert!(!farewells.records.contains_key(&Id::new((1, "one"))));
        assert!(farewells.records.contains_key(&Id::new((2, "one"))));
    }
}
