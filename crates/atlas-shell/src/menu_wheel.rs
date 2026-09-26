//! Navigable menus own the wheel while hovered
//! (`.cursor/rules/navigable-menus.mdc`, P0.10).
//!
//! A list, dropdown, or scrolling popup calls [`claim`] with its screen rect
//! every pass it is open. Canvas cameras ask [`wheel_owned`] before wheel
//! zoom, Shift+wheel pan, or pinch. egui's own popups (combo boxes) count
//! without a claim.

use eframe::egui::{Context, Id, Pos2, Rect};

const CLAIMS_ID: &str = "atlas.menu_wheel";

/// Claims from this pass and the one before it: a menu painted after the
/// canvas this frame still holds the wheel, with egui's usual one-pass lag.
#[derive(Clone, Default)]
struct Claims {
    pass: u64,
    current: Vec<Rect>,
    previous: Vec<Rect>,
}

/// A navigable menu is open at `rect` (screen) this pass.
pub fn claim(ctx: &Context, rect: Rect) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let claims = d.get_temp_mut_or_default::<Claims>(Id::new(CLAIMS_ID));
        if claims.pass != pass {
            std::mem::swap(&mut claims.current, &mut claims.previous);
            if claims.pass + 1 != pass {
                claims.previous.clear();
            }
            claims.current.clear();
            claims.pass = pass;
        }
        claims.current.push(rect);
    });
}

/// The pointer is over a navigable menu: the wheel scrolls the menu, so a
/// canvas must not zoom or pan with it.
pub fn wheel_owned(ctx: &Context) -> bool {
    let Some(p) = ctx.pointer_latest_pos() else {
        return false;
    };
    over_claim(ctx, p) || over_egui_popup(ctx, p)
}

fn over_claim(ctx: &Context, p: Pos2) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let claims = d.get_temp_mut_or_default::<Claims>(Id::new(CLAIMS_ID));
        let hit = |rects: &[Rect]| rects.iter().any(|r| r.contains(p));
        if claims.pass == pass {
            hit(&claims.current) || hit(&claims.previous)
        } else {
            claims.pass + 1 == pass && hit(&claims.current)
        }
    })
}

fn over_egui_popup(ctx: &Context, p: Pos2) -> bool {
    ctx.layer_id_at(p)
        .is_some_and(|layer| ctx.memory(|m| m.is_popup_open(layer.id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{self, Vec2};

    fn frame(ctx: &Context, pointer: Pos2, body: impl FnMut(&egui::Context)) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events: vec![egui::Event::PointerMoved(pointer)],
            ..Default::default()
        };
        let _ = ctx.run(input, body);
    }

    #[test]
    fn a_claimed_menu_owns_the_wheel_for_this_pass_and_the_next() {
        let ctx = Context::default();
        let menu = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(160.0, 200.0));
        let inside = menu.center();
        let mut owned = Vec::new();
        frame(&ctx, inside, |ctx| {
            claim(ctx, menu);
            owned.push(wheel_owned(ctx));
        });
        frame(&ctx, inside, |ctx| owned.push(wheel_owned(ctx)));
        frame(&ctx, inside, |ctx| owned.push(wheel_owned(ctx)));
        assert_eq!(owned, [true, true, false], "a closed menu lets go");
        frame(&ctx, Pos2::new(600.0, 500.0), |ctx| {
            claim(ctx, menu);
            owned.push(wheel_owned(ctx));
        });
        assert!(!owned[3], "only while hovered");
    }

    #[test]
    fn an_open_egui_combo_box_owns_the_wheel_without_a_claim() {
        let ctx = Context::default();
        let mut selected = 0usize;
        let mut list = Rect::NOTHING;
        let run = |ctx: &Context, pointer: Pos2, selected: &mut usize, list: &mut Rect| {
            let mut owned = false;
            frame(ctx, pointer, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let popup = ui.make_persistent_id(Id::new("faces")).with("popup");
                    ui.memory_mut(|m| m.open_popup(popup));
                    egui::ComboBox::from_id_salt("faces").show_ui(ui, |ui| {
                        for i in 0..5 {
                            ui.selectable_value(selected, i, format!("Face {i}"));
                        }
                        *list = ui.min_rect();
                    });
                });
                owned = wheel_owned(ctx);
            });
            owned
        };
        run(&ctx, Pos2::ZERO, &mut selected, &mut list);
        run(&ctx, Pos2::ZERO, &mut selected, &mut list);
        assert!(list.is_positive(), "the list opened");
        assert!(run(&ctx, list.center(), &mut selected, &mut list));
        assert!(!run(
            &ctx,
            Pos2::new(790.0, 590.0),
            &mut selected,
            &mut list
        ));
    }
}
