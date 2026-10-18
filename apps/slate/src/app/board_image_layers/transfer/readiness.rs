use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub(crate) struct LayerBakeReadiness {
    state: Arc<Mutex<Option<Result<(), String>>>>,
}

impl LayerBakeReadiness {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn resolve(&self, result: &Result<(u64, i64), String>) {
        let result = result.as_ref().map(|_| ()).map_err(Clone::clone);
        if let Ok(mut state) = self.state.lock() {
            if state.is_none() {
                *state = Some(result);
            }
        }
    }

    pub(crate) fn result(&self) -> Option<Result<(), String>> {
        self.state.lock().ok().and_then(|state| state.clone())
    }
}
