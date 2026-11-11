use super::*;

// McNeel rhino3dm 8.17.0: new File3dm(), ModelUnitSystem = Meters, toByteArray().
const EMPTY_MODEL_METERS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/empty-meters.3dm"
));

type ReadyResult = Result<super::super::super::super::items::ReadyFile, String>;
pub(in crate::app::application_connector) type EmptyCreation =
    (Receiver<ReadyResult>, Cancellation, Sender<bool>);
#[cfg(test)]
pub(in crate::app::application_connector) type TestEmptyCreation = (
    Receiver<ReadyResult>,
    Cancellation,
    Sender<bool>,
    Receiver<()>,
);

pub(in crate::app::application_connector) fn create_empty(downstream: bool) -> EmptyCreation {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let (accepted, admission) = crossbeam_channel::bounded(1);
    let cancel = Cancellation::default();
    let work_cancel = cancel.clone();
    std::thread::spawn(move || {
        // This worker owns only the unique path it successfully create_new'd.
        // The UI never deletes a generic ReadyFile or chooses cleanup paths.
        let mut owned = None;
        let result = (|| {
            // A wire-created card needs an ordinary empty source immediately.
            // Geometry work on that source still uses the licensed helper.
            let bytes: std::borrow::Cow<'_, [u8]> = if downstream {
                std::borrow::Cow::Borrowed(EMPTY_MODEL_METERS)
            } else {
                std::borrow::Cow::Owned(
                    configuration()?
                        .generate(
                            &atlas_connector_runtime::headless_rhino::GeometryPlan {
                                units: atlas_connector_runtime::headless_rhino::Units::Meters,
                                operations: vec![],
                            },
                            &work_cancel,
                            |_| {},
                        )?
                        .bytes,
                )
            };
            if work_cancel.is_cancelled() {
                return Err("Empty model creation cancelled".into());
            }
            let directory = atlas_core::index::data_dir().join("headless-rhino/models");
            std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
            let path = directory.join(format!("empty-{}.3dm", atlas_agent::request_id()));
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            owned = Some(path.clone());
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            drop(file);
            atlas_core::workbook_assets::write_headless_model_provenance(
                &path,
                path.file_stem().unwrap().to_str().unwrap(),
            )
            .map_err(|e| e.to_string())?;
            super::super::super::super::items::ReadyFile::read(path)
        })();
        deliver_empty_result(tx, admission, result, owned);
    });
    (rx, cancel, accepted)
}

/// Ownership stays on the dedicated blank worker until the existing UI pump
/// accepts its single prepared commit. Receiver loss/app close rejects it too.
fn deliver_empty_result(
    tx: Sender<ReadyResult>,
    admission: Receiver<bool>,
    result: ReadyResult,
    owned: Option<std::path::PathBuf>,
) {
    let has_file = result.is_ok();
    let delivered = tx.send(result).is_ok() && has_file;
    finish_empty_admission(admission, delivered, owned);
}

fn finish_empty_admission(
    admission: Receiver<bool>,
    delivered: bool,
    owned: Option<std::path::PathBuf>,
) {
    let accepted = delivered && admission.recv().unwrap_or(false);
    if !accepted {
        if let Some(path) = owned {
            let _ = std::fs::remove_file(atlas_core::workbook_assets::sidecar_path(&path));
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
pub(in crate::app::application_connector) fn owned_empty_for_test(
    path: std::path::PathBuf,
) -> TestEmptyCreation {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let (accepted, admission) = crossbeam_channel::bounded(1);
    let (done, completion) = crossbeam_channel::bounded(1);
    let (queued, ready) = crossbeam_channel::bounded(1);
    std::thread::spawn(move || {
        let result = super::super::super::super::items::ReadyFile::read(path.clone());
        let has_file = result.is_ok();
        let delivered = tx.send(result).is_ok() && has_file;
        let _ = queued.send(());
        finish_empty_admission(admission, delivered, Some(path));
        let _ = done.send(());
    });
    ready
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("worker fixture delivered metadata");
    (rx, Cancellation::default(), accepted, completion)
}
