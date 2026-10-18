//! A synthetic backend may reply only after the real file worker delivers its request.
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn wait_request(path: &Path, expected: &str) -> atlas_agent::AgentRequest {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(request) = serde_json::from_slice::<atlas_agent::AgentRequest>(&bytes) {
                if request.id == expected {
                    return request;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "message worker did not deliver request {expected}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
