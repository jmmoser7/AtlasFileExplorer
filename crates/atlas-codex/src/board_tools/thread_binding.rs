//! A replacement tools thread must replace the same resume pointer as initial startup.
use std::path::Path;

pub(super) fn persist(directory: Option<&Path>, thread: &str) -> Result<(), String> {
    if let Some(directory) = directory {
        std::fs::write(directory.join("codex-thread.txt"), thread)
            .map_err(|error| format!("Could not preserve the replacement Codex thread: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_updates_resume_pointer_without_rewriting_logical_history() {
        let dir =
            std::env::temp_dir().join(format!("slate-tools-thread-{}", atlas_agent::request_id()));
        std::fs::create_dir(&dir).unwrap();
        let history = br#"{"conversation":"old-thread","turns":[{"role":"user","text":"original brief"},{"role":"assistant","text":"original acknowledgement"}]}"#;
        std::fs::write(dir.join("session.json"), history).unwrap();
        std::fs::write(dir.join("checkpoint.json"), b"[\"retained checkpoint\"]").unwrap();
        std::fs::write(dir.join("codex-thread.txt"), "old-thread").unwrap();
        persist(Some(&dir), "replacement-tools-thread").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("codex-thread.txt")).unwrap(),
            "replacement-tools-thread"
        );
        assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), history);
        assert_eq!(
            std::fs::read(dir.join("checkpoint.json")).unwrap(),
            b"[\"retained checkpoint\"]"
        );
        for name in ["session.json", "checkpoint.json", "codex-thread.txt"] {
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn unavailable_resume_directory_refuses_before_the_paid_turn() {
        let absent =
            std::env::temp_dir().join(format!("slate-absent-thread-{}", atlas_agent::request_id()));
        assert!(!absent.exists());
        assert!(persist(Some(&absent), "replacement-tools-thread").is_err());
    }
}
