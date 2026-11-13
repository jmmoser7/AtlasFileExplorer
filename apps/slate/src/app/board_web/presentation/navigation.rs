//! Open the current web portal in the system browser.
use super::*;

impl SlateApp {
    pub(crate) fn web_open_external(&mut self) -> bool {
        let Some(id) = self.selected_web_portal() else {
            return false;
        };
        self.web_open_external_for(id)
    }

    pub(crate) fn web_open_external_for(&mut self, id: NodeId) -> bool {
        let Some(Node {
            kind: NodeKind::Portal(portal),
            ..
        }) = self.doc().scene.node(id)
        else {
            return false;
        };
        let Some(locator) = self
            .effective_web_portal(id, portal)
            .source
            .as_ref()
            .map(|source| source.locator.clone())
        else {
            return false;
        };
        let workbook = self.tab().path.clone();
        let target = if web_origin(&locator).is_some() {
            locator
        } else {
            resolve_web_source(workbook.as_deref(), &locator)
                .to_string_lossy()
                .into_owned()
        };
        match open_externally(&target) {
            Ok(()) => true,
            Err(e) => {
                self.toast(format!("Could not open {target}: {e}"));
                false
            }
        }
    }
}
