//! The cancel stack: one Esc pops exactly one layer, highest first.

/// A cancellable layer of app state. Variants are declared in pop-priority
/// order: [`CancelLayer::ActiveOperation`] pops first,
/// [`CancelLayer::Chrome`] last. (Deliberate divergence from Rhino's
/// "one Esc resets everything": the layered pop is shipped, user-visible
/// behavior in both apps — this contract makes it testable and identical.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CancelLayer {
    /// A running drag/tool operation (wire drag, zoom-window marquee).
    ActiveOperation,
    /// An uncommitted draft: path draft, crop mode, text edit.
    Draft,
    /// A non-Select tool is active — Esc returns to Select.
    Mode,
    /// A non-empty selection — Esc clears it.
    Selection,
    /// Open menus, popovers, or the palette.
    Chrome,
}

/// Given the layers that are live this frame (any order, duplicates
/// harmless), return the single layer one Esc press should pop:
/// ActiveOperation → Draft → Mode → Selection → Chrome. Returns `None` when
/// nothing is live (the app lets Esc fall through).
///
/// The app assembles `live` from its state each frame and matches on the
/// result — replacing the ad-hoc Escape cascades in both apps' `hotkeys`.
#[must_use]
pub fn cancel_target(live: &[CancelLayer]) -> Option<CancelLayer> {
    live.iter().min().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_stack_pops_in_documented_order() {
        let mut live = vec![
            CancelLayer::Chrome,
            CancelLayer::Selection,
            CancelLayer::Mode,
            CancelLayer::Draft,
            CancelLayer::ActiveOperation,
        ];
        let expected = [
            CancelLayer::ActiveOperation,
            CancelLayer::Draft,
            CancelLayer::Mode,
            CancelLayer::Selection,
            CancelLayer::Chrome,
        ];
        // Simulate successive Esc presses: pop the returned layer each time.
        for want in expected {
            let got = cancel_target(&live).unwrap();
            assert_eq!(got, want);
            live.retain(|l| *l != got);
        }
        assert_eq!(cancel_target(&live), None);
    }

    #[test]
    fn order_is_independent_of_input_order() {
        let live = [
            CancelLayer::Selection,
            CancelLayer::Draft,
            CancelLayer::Chrome,
        ];
        assert_eq!(cancel_target(&live), Some(CancelLayer::Draft));
    }

    #[test]
    fn empty_input_pops_nothing() {
        assert_eq!(cancel_target(&[]), None);
    }
}
