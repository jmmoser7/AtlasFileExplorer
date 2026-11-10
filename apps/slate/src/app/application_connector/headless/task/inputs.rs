//! Off-frame immutable copies of exactly the already prepared wired attachments.
use super::*;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub(super) fn freeze(
    dir: &Path,
    mut inputs: InputSnapshot,
    model_sources: Vec<(u64, PathBuf)>,
    permit: ModelTaskPermit,
) -> Receiver<Result<InputSnapshot, String>> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let dir = dir.join("inputs");
    std::thread::spawn(move || {
        let result = (|| -> Result<InputSnapshot, String> {
            if !permit.live() {
                return Err("Model task expired before input capture".into());
            }
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let mut copies = HashMap::<String, String>::new();
            let mut total = 0usize;
            // Preserve the exact explicitly wired model file independently of
            // its captured view/depth. It never becomes a localImage payload.
            for (node, path) in &model_sources {
                if let Some(item) = inputs.wired.iter_mut().find(|item| item.node == *node) {
                    item.outputs
                        .insert("model_source".into(), path.to_string_lossy().into_owned());
                }
            }
            for item in &mut inputs.wired {
                for source in item
                    .images
                    .iter_mut()
                    .chain(item.depth.iter_mut())
                    .chain(item.outputs.get_mut("model_source"))
                {
                    if let Some(copy) = copies.get(source) {
                        *source = copy.clone();
                        continue;
                    }
                    if copies.len() >= 32 || !permit.live() {
                        return Err("Model task attachment budget or lifetime exceeded".into());
                    }
                    let path = Path::new(source);
                    if atlas_core::cloud::is_dehydrated(path) {
                        return Err("A declared model task input is not locally available".into());
                    }
                    let metadata = fs::metadata(path)
                        .map_err(|e| format!("Declared model input unavailable: {e}"))?;
                    if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
                        return Err("Declared model task attachment exceeds 64 MiB".into());
                    }
                    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
                    let mut bytes = Vec::new();
                    Read::by_ref(&mut file)
                        .take(64 * 1024 * 1024 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    total = total
                        .checked_add(bytes.len())
                        .ok_or("Attachment budget overflow")?;
                    if bytes.len() as u64 != metadata.len()
                        || bytes.is_empty()
                        || total > 256 * 1024 * 1024
                        || !permit.live()
                    {
                        return Err("Model task input changed, exceeded its budget, or expired while being captured".into());
                    }
                    let after = file.metadata().map_err(|e| e.to_string())?;
                    if after.len() != metadata.len()
                        || after.modified().ok() != metadata.modified().ok()
                    {
                        return Err("Declared model input changed during capture".into());
                    }
                    let extension = path
                        .extension()
                        .and_then(|v| v.to_str())
                        .filter(|v| v.len() <= 8 && v.bytes().all(|b| b.is_ascii_alphanumeric()))
                        .unwrap_or("bin");
                    let destination = dir.join(format!(
                        "{}-{}.{}",
                        copies.len(),
                        model_preview::view_meta::hash_bytes(&bytes),
                        extension
                    ));
                    OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&destination)
                        .and_then(|mut file| file.write_all(&bytes))
                        .map_err(|e| e.to_string())?;
                    let copy = destination.to_string_lossy().into_owned();
                    copies.insert(source.clone(), copy.clone());
                    *source = copy;
                }
                for output in item.outputs.values_mut() {
                    if let Some(copy) = copies.get(output) {
                        *output = copy.clone();
                    }
                }
                item.outputs
                    .retain(|_, path| copies.values().any(|copy| copy == path));
            }
            Ok(inputs)
        })();
        let _ = tx.send(result);
    });
    rx
}
