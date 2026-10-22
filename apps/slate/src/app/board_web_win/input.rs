//! Native page input: wheel quantization and DevTools keyboard translation.

use super::*;

/// Fractional Windows wheel units are retained per view and axis. Fifty egui
/// points still produce one 120-unit notch; tiny smooth deltas never turn into
/// a full notch. Cell lets the existing keyboard-release owner reset gestures.
#[derive(Default)]
pub(super) struct WheelRemainders {
    axes: Cell<[f64; 2]>,
    generation: Cell<u64>,
}

impl WheelRemainders {
    pub(super) fn reset(&self) {
        self.axes.set([0.0; 2]);
    }

    fn delta(&self, delta: f32, horizontal: bool, generation: u64) -> Option<u32> {
        if self.generation.replace(generation) != generation {
            self.reset();
        }
        if !delta.is_finite() || delta == 0.0 {
            return None;
        }
        let mut axes = self.axes.get();
        let axis = usize::from(horizontal);
        let total = (axes[axis] + f64::from(delta) * (120.0 / 50.0))
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX));
        let units = total.round() as i32;
        axes[axis] = total - f64::from(units);
        self.axes.set(axes);
        (units != 0).then_some(units as u32)
    }
}

impl Webview2Host {
    pub(super) fn send_page_input(&mut self, id: NodeId, input: WebInput) {
        let Some(comp) = self
            .views
            .get(&id)
            .and_then(|v| v.shared.borrow().comp.clone())
        else {
            return;
        };
        let scale = self.views.get(&id).map_or(1.0, |v| v.scale);
        let point = |x: f32, y: f32| capture_point(x, y, scale);
        let none = COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE;
        let mouse = |kind: COREWEBVIEW2_MOUSE_EVENT_KIND,
                     data: u32,
                     p: POINT,
                     keys: COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS| {
            let _ = unsafe { comp.SendMouseInput(kind, keys, data, p) };
        };
        match input {
            WebInput::Move { x, y, buttons } => mouse(
                COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE,
                0,
                point(x, y),
                held_keys(buttons),
            ),
            WebInput::Down { x, y, button } => mouse(
                match button {
                    1 => COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN,
                    2 => COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOWN,
                    _ => COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
                },
                0,
                point(x, y),
                none,
            ),
            WebInput::Up { x, y, button } => mouse(
                match button {
                    1 => COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP,
                    2 => COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_UP,
                    _ => COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP,
                },
                0,
                point(x, y),
                none,
            ),
            WebInput::Wheel {
                x,
                y,
                delta,
                horizontal,
            } => {
                let view = &self.views[&id];
                let generation = view.shared.borrow().document_generation;
                if let Some(data) = view.wheel.delta(delta, horizontal, generation) {
                    mouse(
                        if horizontal {
                            COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL
                        } else {
                            COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL
                        },
                        data,
                        point(x, y),
                        none,
                    );
                }
            }
            WebInput::Leave => {
                self.views[&id].wheel.reset();
                mouse(
                    COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE,
                    0,
                    POINT { x: 0, y: 0 },
                    none,
                );
            }
            // Keyboard goes through DevTools rather than the parent window's
            // focus, so egui keeps deciding what reaches the page and Esc can
            // still peel focus off it (D22).
            WebInput::Key {
                key,
                pressed,
                modifiers,
            } => {
                if let Some(params) = key_event_params(key, pressed, modifiers) {
                    self.devtools(id, "Input.dispatchKeyEvent", &params);
                }
            }
            WebInput::Text(text) => {
                if !text.is_empty() {
                    self.devtools(id, "Input.insertText", &text_params(&text));
                }
            }
        }
    }
}

fn text_params(text: &str) -> String {
    serde_json::json!({ "text": text }).to_string()
}

fn key_event_params(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> Option<String> {
    let code = virtual_key(key)?;
    let name = match key {
        egui::Key::ArrowLeft => "ArrowLeft",
        egui::Key::ArrowRight => "ArrowRight",
        egui::Key::ArrowUp => "ArrowUp",
        egui::Key::ArrowDown => "ArrowDown",
        egui::Key::Space => " ",
        _ => key.name(),
    };
    let name = if (0x41..=0x5A).contains(&code) && !modifiers.shift {
        name.to_ascii_lowercase()
    } else {
        name.to_owned()
    };
    // rawKeyDown carries keyboard actions without synthesizing committed text;
    // the platform Text/Paste event supplies that text exactly once.
    Some(
        serde_json::json!({
            "type": if pressed { "rawKeyDown" } else { "keyUp" },
            "windowsVirtualKeyCode": code,
            "nativeVirtualKeyCode": code,
            "key": name,
            "modifiers": u8::from(modifiers.alt)
                | (u8::from(modifiers.ctrl) << 1)
                | (u8::from(modifiers.mac_cmd) << 2)
                | (u8::from(modifiers.shift) << 3),
        })
        .to_string(),
    )
}

/// Buttons held during a move, so a drag reads as a drag inside the page.
fn held_keys(buttons: u8) -> COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS {
    let mut keys = COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE;
    if buttons & 1 != 0 {
        keys |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_LEFT_BUTTON;
    }
    if buttons & 2 != 0 {
        keys |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_RIGHT_BUTTON;
    }
    if buttons & 4 != 0 {
        keys |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_MIDDLE_BUTTON;
    }
    keys
}

/// One Windows key mapping for editing and dashboard shortcuts. Committed
/// characters travel separately through Input.insertText.
fn virtual_key(key: egui::Key) -> Option<u32> {
    use egui::Key::*;
    Some(match key {
        ArrowDown => 0x28,
        ArrowLeft => 0x25,
        ArrowRight => 0x27,
        ArrowUp => 0x26,
        Backspace => 0x08,
        Delete => 0x2E,
        End => 0x23,
        Enter => 0x0D,
        Home => 0x24,
        Insert => 0x2D,
        PageDown => 0x22,
        PageUp => 0x21,
        Tab => 0x09,
        Space => 0x20,
        F5 => 0x74,
        A => 0x41,
        B => 0x42,
        C => 0x43,
        D => 0x44,
        E => 0x45,
        F => 0x46,
        G => 0x47,
        H => 0x48,
        I => 0x49,
        J => 0x4A,
        K => 0x4B,
        L => 0x4C,
        M => 0x4D,
        N => 0x4E,
        O => 0x4F,
        P => 0x50,
        Q => 0x51,
        R => 0x52,
        S => 0x53,
        T => 0x54,
        U => 0x55,
        V => 0x56,
        W => 0x57,
        X => 0x58,
        Y => 0x59,
        Z => 0x5A,
        Num0 => 0x30,
        Num1 => 0x31,
        Num2 => 0x32,
        Num3 => 0x33,
        Num4 => 0x34,
        Num5 => 0x35,
        Num6 => 0x36,
        Num7 => 0x37,
        Num8 => 0x38,
        Num9 => 0x39,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(data: Option<u32>) -> i32 {
        data.map_or(0, |value| value as i32)
    }

    #[test]
    fn wheel_zero_and_invalid_samples_do_not_emit_or_poison_fractional_input() {
        let wheel = WheelRemainders::default();
        assert_eq!(wheel.delta(0.2, false, 0), None);
        for value in [0.0, -0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(wheel.delta(value, false, 0), None);
        }
        assert_eq!(signed(wheel.delta(0.01, false, 0)), 1);
    }

    #[test]
    fn wheel_notches_and_tiny_signed_deltas_keep_the_existing_calibration() {
        for sign in [-1.0, 1.0] {
            let wheel = WheelRemainders::default();
            assert_eq!(
                signed(wheel.delta(sign * 50.0, false, 0)),
                sign as i32 * 120
            );
            assert_eq!(wheel.delta(sign * 0.01, false, 0), None);
            let nearby = WheelRemainders::default();
            assert_eq!(signed(nearby.delta(sign * 0.21, false, 0)), sign as i32);
            let total: i32 = (0..4999)
                .map(|_| signed(wheel.delta(sign * 0.01, false, 0)))
                .sum();
            assert_eq!(total, sign as i32 * 120);
        }
    }

    #[test]
    fn wheel_fractional_samples_conserve_aggregate_motion_through_direction_changes() {
        let wheel = WheelRemainders::default();
        let mut requested = 0.0;
        let mut delivered = 0;
        for delta in [0.01, 0.2, -0.03, 0.14, -0.35, 0.4, -0.17]
            .into_iter()
            .cycle()
            .take(1000)
        {
            requested += f64::from(delta) * 2.4;
            delivered += signed(wheel.delta(delta, false, 0));
            assert!((requested - f64::from(delivered)).abs() <= 0.500_001);
        }
    }

    #[test]
    fn wheel_axes_views_leave_and_navigation_reset_are_isolated() {
        let first = WheelRemainders::default();
        let second = WheelRemainders::default();
        assert_eq!(first.delta(0.2, false, 7), None);
        assert_eq!(first.delta(0.2, true, 7), None);
        assert_eq!(second.delta(0.01, false, 7), None);
        assert_eq!(signed(first.delta(0.01, false, 7)), 1);
        assert_eq!(signed(first.delta(0.01, true, 7)), 1);
        first.reset(); // The Leave / keyboard release path uses this same owner.
        assert_eq!(first.delta(0.2, false, 7), None);
        assert_eq!(
            first.delta(0.01, false, 8),
            None,
            "a new document has no old residual"
        );
        assert_eq!(first.delta(0.2, false, 8), Some(1));
        assert_eq!(second.delta(0.01, false, 7), None);
    }

    #[test]
    fn text_params_preserve_a_whole_unicode_commit_and_escape_json_once() {
        let text = "e\u{301} 👩\u{200d}💻 漢字 \"quote\" \\ \n\t\u{0}";
        let params: serde_json::Value = serde_json::from_str(&text_params(text)).unwrap();
        assert_eq!(params["text"], text);
        assert_eq!(params.as_object().unwrap().len(), 1);
    }

    #[test]
    fn key_translation_preserves_modifiers_and_never_generates_committed_text() {
        let modifiers = egui::Modifiers {
            alt: true,
            ctrl: true,
            shift: true,
            mac_cmd: true,
            command: true,
        };
        let params: serde_json::Value =
            serde_json::from_str(&key_event_params(egui::Key::A, true, modifiers).unwrap())
                .unwrap();
        assert_eq!(params["modifiers"], 15);
        assert_eq!(params["windowsVirtualKeyCode"], 0x41);
        assert_eq!(params["key"], "A");
        assert_eq!(params["type"], "rawKeyDown");
        assert!(params.get("text").is_none());
        let released: serde_json::Value = serde_json::from_str(
            &key_event_params(egui::Key::A, false, egui::Modifiers::CTRL).unwrap(),
        )
        .unwrap();
        assert_eq!(released["type"], "keyUp");
        assert_eq!(released["modifiers"], 2);
        assert_eq!(released["key"], "a");
        assert!(key_event_params(egui::Key::Escape, true, modifiers).is_none());
    }

    #[test]
    fn arrow_keys_use_dom_names_not_egui_display_labels() {
        for (key, name, code) in [
            (egui::Key::ArrowLeft, "ArrowLeft", 0x25),
            (egui::Key::ArrowRight, "ArrowRight", 0x27),
            (egui::Key::ArrowUp, "ArrowUp", 0x26),
            (egui::Key::ArrowDown, "ArrowDown", 0x28),
        ] {
            for pressed in [true, false] {
                let params: serde_json::Value = serde_json::from_str(
                    &key_event_params(key, pressed, egui::Modifiers::SHIFT).unwrap(),
                )
                .unwrap();
                assert_eq!(params["key"], name);
                assert_eq!(params["windowsVirtualKeyCode"], code);
                assert_eq!(params["modifiers"], 8);
            }
        }
    }

    #[test]
    fn every_letter_and_digit_has_one_windows_shortcut_key() {
        use egui::Key::*;
        for (keys, start) in [
            (
                &[
                    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
                ][..],
                0x41,
            ),
            (
                &[Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9][..],
                0x30,
            ),
        ] {
            for (offset, key) in keys.iter().enumerate() {
                assert_eq!(virtual_key(*key), Some(start + offset as u32));
            }
        }
    }
}
