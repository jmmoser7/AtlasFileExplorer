use serde_json::{json, Value};
use std::{
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn read_text_page(
    root: PathBuf,
    path: PathBuf,
    locator: &str,
    generation: u64,
    expected_size: u64,
    expected_mtime: i64,
    cursor: usize,
    limit: usize,
) -> Result<Value, String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "txt"
            | "md"
            | "csv"
            | "tsv"
            | "json"
            | "xml"
            | "yaml"
            | "yml"
            | "toml"
            | "ini"
            | "log"
            | "rs"
            | "py"
            | "js"
            | "ts"
            | "html"
            | "css"
    ) {
        return Err("This portal reader supports bounded UTF-8 text files only".into());
    }
    if atlas_core::secrets::is_machine_private(&path) {
        return Err("Machine-private app state is outside portal source access".into());
    }
    if atlas_core::cloud::is_dehydrated(&path) {
        return Err("Cloud-only placeholder was skipped to avoid downloading it".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Could not verify Atlas root: {e}"))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("Could not verify file path: {e}"))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err("Resolved file escaped the granted Atlas root".into());
    }
    if atlas_core::secrets::is_machine_private(&canonical_path) {
        return Err("Resolved source is machine-private app state".into());
    }
    let metadata = std::fs::metadata(&canonical_path).map_err(|e| e.to_string())?;
    if metadata.len() != expected_size
        || metadata.len() > 64 * 1024 * 1024
        || metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64)
            != expected_mtime
    {
        return Err("File changed since the Atlas scan; refresh before reading".into());
    }
    if cursor as u64 > metadata.len() {
        return Err("Text cursor is past end of file".into());
    }
    let mut file = std::fs::File::open(&canonical_path).map_err(|e| e.to_string())?;
    if cursor > 0 {
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut prefix = vec![0; cursor];
        file.read_exact(&mut prefix).map_err(|e| e.to_string())?;
        std::str::from_utf8(&prefix).map_err(|_| "Cursor must be a UTF-8 character boundary")?;
    }
    file.seek(SeekFrom::Start(cursor as u64))
        .map_err(|e| e.to_string())?;
    let mut bytes = vec![0; limit.saturating_add(4)];
    let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
    bytes.truncate(n);
    if let Err(error) = std::str::from_utf8(&bytes) {
        if error.error_len().is_some() {
            return Err("File is not valid UTF-8".into());
        }
        bytes.truncate(error.valid_up_to());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "File is not valid UTF-8")?;
    let used = text.len().min(limit);
    let mut end = used;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let next = cursor + end;
    let after_read = std::fs::metadata(&canonical_path).map_err(|e| e.to_string())?;
    if after_read.len() != expected_size
        || after_read
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64)
            != expected_mtime
    {
        return Err("File changed while the bounded text page was read".into());
    }
    Ok(json!({
        "locator": locator,
        "text": &text[..end],
        "cursor": cursor,
        "next_cursor": (next < metadata.len() as usize).then_some(next),
        "generation": generation
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("slate-portal-read-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn snapshot(path: &std::path::Path) -> (u64, i64) {
        let metadata = fs::metadata(path).unwrap();
        let mtime = metadata
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        (metadata.len(), mtime)
    }

    fn page(
        root: &std::path::Path,
        path: &std::path::Path,
        cursor: usize,
        limit: usize,
    ) -> Result<Value, String> {
        let (size, mtime) = snapshot(path);
        read_text_page(
            root.to_path_buf(),
            path.to_path_buf(),
            "notes.txt",
            7,
            size,
            mtime,
            cursor,
            limit,
        )
    }

    #[test]
    fn granted_folder_does_not_expose_machine_private_app_state() {
        let root = TestDir::new();
        let private = root.0.join("NativeFileAtlas").join("secrets");
        fs::create_dir_all(&private).unwrap();
        let file = private.join("fixture.txt");
        fs::write(&file, "synthetic fixture").unwrap();
        assert!(page(&root.0, &file, 0, 64)
            .unwrap_err()
            .contains("Machine-private"));
    }

    #[test]
    fn text_pages_stop_only_at_utf8_character_boundaries() {
        let root = TestDir::new();
        let file = root.file("notes.txt", "aé🙂z".as_bytes());
        let first = page(&root.0, &file, 0, 4).unwrap();
        assert_eq!(first["text"], "aé");
        assert_eq!(first["next_cursor"], 3);
        let second = page(&root.0, &file, 3, 4).unwrap();
        assert_eq!(second["text"], "🙂");
        assert_eq!(second["next_cursor"], 7);
        let third = page(&root.0, &file, 7, 4).unwrap();
        assert_eq!(third["text"], "z");
        assert_eq!(third["next_cursor"], Value::Null);

        let split = root.file("split.txt", "12345🙂end".as_bytes());
        let first = page(&root.0, &split, 0, 4).unwrap();
        assert_eq!(first["text"], "1234");
        assert_eq!(first["next_cursor"], 4);
        let second = page(&root.0, &split, 4, 4).unwrap();
        assert_eq!(second["text"], "5");
        assert_eq!(second["next_cursor"], 5);
        let third = page(&root.0, &split, 5, 4).unwrap();
        assert_eq!(third["text"], "🙂");
        assert_eq!(third["next_cursor"], 9);
    }

    #[test]
    fn invalid_interior_utf8_fails_instead_of_returning_a_stuck_cursor() {
        let root = TestDir::new();
        let file = root.file("notes.txt", &[b'a', 0xff, b'z']);
        assert!(page(&root.0, &file, 0, 4)
            .unwrap_err()
            .contains("valid UTF-8"));
    }

    #[test]
    fn text_read_rejects_stale_metadata_and_files_outside_the_root() {
        let root = TestDir::new();
        let file = root.file("notes.txt", b"inside");
        let (size, mtime) = snapshot(&file);
        assert!(read_text_page(
            root.0.clone(),
            file.clone(),
            "notes.txt",
            7,
            size + 1,
            mtime,
            0,
            16,
        )
        .is_err());

        let outside = TestDir::new();
        let outside_file = outside.file("secret.txt", b"outside");
        let (outside_size, outside_mtime) = snapshot(&outside_file);
        assert!(read_text_page(
            root.0.clone(),
            outside_file,
            "secret.txt",
            7,
            outside_size,
            outside_mtime,
            0,
            16,
        )
        .unwrap_err()
        .contains("escaped"));
    }
}
