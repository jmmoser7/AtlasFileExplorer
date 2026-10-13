use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) enum DeferredWorkbookAction {
    Save { tab: u64, path: PathBuf },
    Export { tab: u64, dir: PathBuf },
    Collect { tab: u64 },
    CloseDiscard { tab: u64 },
}

#[derive(Default)]
pub(crate) struct DeferredWorkbookActions(Vec<DeferredWorkbookAction>);

impl DeferredWorkbookActions {
    pub(crate) fn save(&mut self, tab: u64, path: PathBuf) {
        self.0.retain(|action| {
            !matches!(action, DeferredWorkbookAction::Save { tab: queued, .. } if *queued == tab)
        });
        self.0.push(DeferredWorkbookAction::Save { tab, path });
    }

    pub(crate) fn export(&mut self, tab: u64, dir: PathBuf) {
        self.0.retain(|action| {
            !matches!(action, DeferredWorkbookAction::Export { tab: queued, .. } if *queued == tab)
        });
        self.0.push(DeferredWorkbookAction::Export { tab, dir });
    }

    pub(crate) fn collect(&mut self, tab: u64) {
        if !self.0.iter().any(
            |action| matches!(action, DeferredWorkbookAction::Collect { tab: queued } if *queued == tab),
        ) {
            self.0.push(DeferredWorkbookAction::Collect { tab });
        }
    }

    pub(crate) fn close_discard(&mut self, tab: u64) {
        if !self.0.iter().any(
            |action| matches!(action, DeferredWorkbookAction::CloseDiscard { tab: queued } if *queued == tab),
        ) {
            self.0.push(DeferredWorkbookAction::CloseDiscard { tab });
        }
    }

    pub(crate) fn has_tab(&self, tab: u64) -> bool {
        self.0.iter().any(|action| match action {
            DeferredWorkbookAction::Save { tab: queued, .. }
            | DeferredWorkbookAction::Export { tab: queued, .. }
            | DeferredWorkbookAction::Collect { tab: queued }
            | DeferredWorkbookAction::CloseDiscard { tab: queued } => *queued == tab,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn cancel_io(&mut self, tab: u64) {
        self.0.retain(|action| {
            let queued = match action {
                DeferredWorkbookAction::Save { tab, .. }
                | DeferredWorkbookAction::Export { tab, .. }
                | DeferredWorkbookAction::Collect { tab }
                | DeferredWorkbookAction::CloseDiscard { tab } => *tab,
            };
            queued != tab || matches!(action, DeferredWorkbookAction::CloseDiscard { .. })
        });
    }

    pub(crate) fn clear_tab(&mut self, tab: u64) {
        self.0.retain(|action| {
            let queued = match action {
                DeferredWorkbookAction::Save { tab, .. }
                | DeferredWorkbookAction::Export { tab, .. }
                | DeferredWorkbookAction::Collect { tab }
                | DeferredWorkbookAction::CloseDiscard { tab } => *tab,
            };
            queued != tab
        });
    }

    pub(crate) fn take_tab(
        &mut self,
        tab: u64,
        active_tab: u64,
        export_ready: bool,
    ) -> Vec<DeferredWorkbookAction> {
        let mut ready = Vec::new();
        self.0.retain(|action| {
            let matches = match action {
                DeferredWorkbookAction::Save { tab: queued, .. }
                | DeferredWorkbookAction::Export { tab: queued, .. }
                | DeferredWorkbookAction::Collect { tab: queued }
                | DeferredWorkbookAction::CloseDiscard { tab: queued } => *queued == tab,
            };
            let wait_for_export = matches!(action, DeferredWorkbookAction::Export { .. })
                && (tab != active_tab || !export_ready);
            if matches && !wait_for_export {
                ready.push(action.clone());
            }
            !matches || wait_for_export
        });
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_export_stays_queued_while_save_replays_by_tab_id() {
        let mut queue = DeferredWorkbookActions::default();
        queue.save(7, PathBuf::from("book.slate"));
        queue.export(7, PathBuf::from("out"));

        let ready = queue.take_tab(7, 9, true);
        assert!(matches!(
            ready.as_slice(),
            [DeferredWorkbookAction::Save { tab: 7, .. }]
        ));
        assert!(queue.has_tab(7));

        let ready = queue.take_tab(7, 7, true);
        assert!(matches!(
            ready.as_slice(),
            [DeferredWorkbookAction::Export { tab: 7, .. }]
        ));
        assert!(queue.is_empty());
    }
}
