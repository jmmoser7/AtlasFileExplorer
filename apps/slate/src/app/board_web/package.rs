//! Local dashboard packaging runs off the frame loop. Completion is bound to
//! its originating tab, saved path and portal parameters before a journaled rebind.
use super::*;
use crossbeam_channel::{bounded, Receiver, TryRecvError};

pub(super) struct PackageJob {
    tab: u64,
    workbook: PathBuf,
    node: NodeId,
    portal: PortalNode,
    rx: Receiver<std::io::Result<PackagedSource>>,
}

struct PackagedSource {
    entry: PathBuf,
    pin: Option<slate_doc::scene::ArtifactCopyPin>,
}

impl PackageJob {
    fn matches(&self, app: &SlateApp) -> bool {
        !app.tabs.is_empty()
            && app.tab().id == self.tab
            && app.tab().path.as_ref() == Some(&self.workbook)
            && app.doc().scene.node(self.node).is_some_and(
                |node| matches!(&node.kind, NodeKind::Portal(portal) if portal == &self.portal),
            )
    }
}

fn package_destination(
    workbook: &Path,
    node: NodeId,
    portal: &PortalNode,
    source: &Path,
) -> PathBuf {
    let name = if let Some(binding) = portal.web.as_ref().and_then(|web| web.artifact.as_deref()) {
        // Identity survives disposable materialization paths and later runs on
        // the same node. Hashing authored metadata does no source I/O.
        let identity = serde_json::json!([binding.family, binding.revision]);
        let hash = atlas_ai::versions::hash_artifact_input(identity.to_string().as_bytes());
        format!("artifact-{}-{hash}", node.0)
    } else {
        let stem = source.file_stem().unwrap_or_default().to_string_lossy();
        format!("{stem}-node-{}", node.0)
    };
    atlas_core::workbook_assets::web_package_dir(workbook, &name)
}

fn package(
    workbook: &Path,
    node: NodeId,
    portal: &PortalNode,
    source: &Path,
) -> std::io::Result<PackagedSource> {
    use atlas_core::workbook_assets::{
        copy_file, copy_tree, require_copy_destination, require_local_source, write_file,
    };
    let verified = portal
        .web
        .as_ref()
        .and_then(|web| web.artifact.as_deref())
        .map(|_| {
            super::super::board_agent::materialize_artifact_source(Some(workbook), portal)
                .map_err(std::io::Error::other)
        })
        .transpose()?;
    let source = verified
        .as_ref()
        .map(|value| value.entry.clone())
        .unwrap_or_else(|| source.to_path_buf());
    let pin = verified.map(|value| slate_doc::scene::ArtifactCopyPin {
        revision: value.revision,
        source_hash: value.source_hash,
    });
    let destination = package_destination(workbook, node, portal, &source);
    require_local_source(&source)?;
    let provenance = destination.join("origin.txt");
    require_copy_destination(&provenance)?;
    let copied = if source.is_dir() {
        copy_tree(&source, &destination)?;
        destination.to_path_buf()
    } else {
        let file = destination.join(source.file_name().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "page has no file name")
        })?);
        if let Some(pin) = pin.as_ref().filter(|_| file.exists()) {
            atlas_ai::versions::verify_artifact_source(&file, &pin.source_hash)?;
            if !provenance.is_file() {
                return Err(std::io::Error::other(
                    "The existing artifact package has no origin record.",
                ));
            }
            return Ok(PackagedSource {
                entry: file,
                pin: Some(pin.clone()),
            });
        }
        if atlas_core::workbook_assets::same_path(&source, &file) {
            return Ok(PackagedSource { entry: file, pin });
        }
        copy_file(&source, &file)?;
        file
    };
    if let Some(pin) = &pin {
        // Check the copied bytes as well: a disposable cache can change after
        // verification, and an existing accepted package is never overwritten.
        atlas_ai::versions::verify_artifact_source(&copied, &pin.source_hash)?;
    }
    // A permanent fork names where it came from (Art. IX.4). Fail explicitly
    // if provenance cannot be written; never rebind to an incomplete package.
    write_file(&provenance, format!("{}\n", source.display()).as_bytes())?;
    Ok(PackagedSource { entry: copied, pin })
}

impl SlateApp {
    pub(crate) fn web_package_selected(&mut self) -> bool {
        let Some((id, locator)) = self.selected_web_locator() else {
            return false;
        };
        if self.web.package.is_some() {
            self.toast("A page is already being packaged.");
            return true;
        }
        if web_origin(&locator).is_some() {
            self.toast("Only local pages can be packaged; a URL stays a link.");
            return true;
        }
        let Some(workbook) = self.tab().path.clone() else {
            self.toast("Save the workbook first — packaging copies material beside it.");
            return true;
        };
        let Some(Node {
            kind: NodeKind::Portal(portal),
            ..
        }) = self.doc().scene.node(id)
        else {
            return false;
        };
        let portal = portal.clone();
        let source = resolve_web_source(Some(&workbook), &locator);
        let package_workbook = workbook.clone();
        let package_portal = portal.clone();
        let (tx, rx) = bounded(1);
        self.web.package = Some(PackageJob {
            tab: self.tab().id,
            workbook,
            node: id,
            portal,
            rx,
        });
        std::thread::spawn(move || {
            let _ = tx.send(package(&package_workbook, id, &package_portal, &source));
        });
        self.toast("Packaging the local page…");
        true
    }

    pub(super) fn web_package_pump(&mut self, ctx: &egui::Context) {
        let Some(job) = self.web.package.as_ref() else {
            return;
        };
        let result = match job.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            }
            Err(TryRecvError::Disconnected) => {
                Err(std::io::Error::other("packaging worker stopped"))
            }
        };
        let job = self.web.package.take().expect("pending package");
        match result {
            Ok(copied) if job.matches(self) => {
                let locator = web_source_locator(Some(&job.workbook), &copied.entry);
                if self.bind_web_source(job.node, locator) {
                    if let Some(pin) = copied.pin {
                        let mut after = self.doc().scene.node(job.node).unwrap().clone();
                        let Some(binding) = slate_doc::scene::artifact_mut(&mut after) else {
                            self.toast("Page copied; its artifact binding changed before the copy could be pinned.");
                            return;
                        };
                        binding.packaged = Some(pin);
                        if !self.fold_into_last_step(&after) {
                            self.toast("Page copied; its artifact pin could not be attached to the package edit.");
                            return;
                        }
                    }
                    self.toast("Packaged beside the workbook; the portal now points at the copy.");
                } else {
                    self.toast("Page packaged; the portal could not be rebound.");
                }
            }
            Ok(copied) => self.toast(format!(
                "Page packaged at {}; its source tab or portal changed, so no portal was rebound.",
                copied.entry.display()
            )),
            Err(error) => self.toast(format!("Could not package: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact_source(
        base: &Path,
        workbook: &Path,
        family: &str,
        revision: &str,
        html: &[u8],
    ) -> (PortalNode, atlas_agent::ArtifactRevisionRef) {
        let run =
            atlas_ai::versions::prepare_artifact_run(&base.join("history"), family, revision, None)
                .unwrap();
        std::fs::write(run.staging_output.join("index.html"), html).unwrap();
        let provenance = atlas_agent::ArtifactProvenance {
            prompt: "Package this chart".into(),
            provider: "fixture".into(),
            model: "fixture-model".into(),
            checkpoint: atlas_agent::ArtifactCheckpoint {
                conversation: "fixture-session".into(),
                through: 2,
                turns: vec![
                    atlas_agent::AgentTurn {
                        role: "user".into(),
                        text: "Package this chart".into(),
                        at: 1,
                    },
                    atlas_agent::AgentTurn {
                        role: "assistant".into(),
                        text: "Chart complete".into(),
                        at: 2,
                    },
                ],
            },
            inputs: vec![],
        };
        let (reference, _) =
            atlas_ai::versions::publish_artifact_revision(&run, provenance).unwrap();
        let mut portal = PortalNode::unbound_web("Artifact");
        portal.source = Some(SourceUri {
            locator: web_source_locator(Some(workbook), &reference.materialized_entry),
        });
        portal.web.as_mut().unwrap().artifact =
            Some(Box::new(slate_doc::scene::ArtifactGeneratorRef {
                family: family.into(),
                history_root: Some(SourceUri {
                    locator: web_source_locator(Some(workbook), &reference.source_root),
                }),
                revision: Some(revision.into()),
                revisions: vec![revision.into()],
                checkpoint: 2,
                ..Default::default()
            }));
        (portal, reference)
    }

    #[test]
    fn same_stem_artifacts_package_separately_and_copy_only_the_selected_source() {
        let h = super::super::super::tests::web_board("package_same_stem");
        let workbook = h.base.join("charts.slate");
        let first = b"<html><body>first chart</body></html>";
        let second = b"<html><body>second chart</body></html>";
        let (p1, r1) = artifact_source(&h.base, &workbook, "one", "r1", first);
        let (p2, r2) = artifact_source(&h.base, &workbook, "two", "r1", second);
        assert_eq!(
            r1.materialized_entry.file_name(),
            r2.materialized_entry.file_name()
        );
        let one = package(&workbook, NodeId(1), &p1, &r1.materialized_entry).unwrap();
        let two = package(&workbook, NodeId(2), &p2, &r2.materialized_entry).unwrap();
        assert_ne!(one.entry, two.entry);
        assert_eq!(std::fs::read(&one.entry).unwrap(), first);
        assert_eq!(std::fs::read(&two.entry).unwrap(), second);
        let mut files = std::fs::read_dir(one.entry.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(
            files,
            vec![std::ffi::OsString::from("index.html"), "origin.txt".into()]
        );
        assert_eq!(
            one.pin.unwrap().source_hash,
            atlas_ai::versions::hash_artifact_input(first)
        );
    }

    #[test]
    fn packaging_a_new_revision_on_the_same_node_preserves_the_previous_copy() {
        let h = super::super::super::tests::web_board("package_node_revisions");
        let workbook = h.base.join("charts.slate");
        let first = b"<html><body>accepted r1</body></html>";
        let second = b"<html><body>accepted r2</body></html>";
        let (p1, r1) = artifact_source(&h.base, &workbook, "one", "r1", first);
        let (p2, r2) = artifact_source(&h.base, &workbook, "one", "r2", second);
        let one = package(&workbook, NodeId(4), &p1, &r1.materialized_entry).unwrap();
        let two = package(&workbook, NodeId(4), &p2, &r2.materialized_entry).unwrap();
        assert_ne!(one.entry, two.entry);
        assert_eq!(std::fs::read(&one.entry).unwrap(), first);
        assert_eq!(std::fs::read(&two.entry).unwrap(), second);
        assert_eq!(
            package_destination(
                &workbook,
                NodeId(4),
                &p1,
                Path::new("other-cache/index.html")
            ),
            one.entry.parent().unwrap().to_path_buf()
        );
    }

    #[test]
    fn packaging_refuses_a_tampered_materialization_and_verifies_portable_copies() {
        let h = super::super::super::tests::web_board("package_verified_source");
        let workbook = h.base.join("charts.slate");
        let html = b"<html><body>exact source</body></html>";
        let (mut portal, reference) = artifact_source(&h.base, &workbook, "one", "r1", html);
        let copy = package(&workbook, NodeId(7), &portal, &reference.materialized_entry).unwrap();
        std::fs::write(
            &reference.materialized_entry,
            b"<html><body>tampered cache</body></html>",
        )
        .unwrap();
        assert!(package(&workbook, NodeId(7), &portal, &reference.materialized_entry).is_err());
        assert_eq!(std::fs::read(&copy.entry).unwrap(), html);
        portal.source = Some(SourceUri {
            locator: web_source_locator(Some(&workbook), &copy.entry),
        });
        portal
            .web
            .as_mut()
            .unwrap()
            .artifact
            .as_mut()
            .unwrap()
            .packaged = copy.pin.clone();
        assert!(reference
            .source_root
            .starts_with(h.base.canonicalize().unwrap()));
        std::fs::remove_dir_all(&reference.source_root).unwrap();
        let repeated = package(&workbook, NodeId(7), &portal, &copy.entry).unwrap();
        assert_eq!(repeated.entry, copy.entry);
        assert_eq!(std::fs::read(&copy.entry).unwrap(), html);
        std::fs::write(
            &copy.entry,
            b"<html><body>edited packaged source</body></html>",
        )
        .unwrap();
        assert!(package(&workbook, NodeId(7), &portal, &copy.entry).is_err());
    }

    #[test]
    fn package_completion_journals_source_and_portable_pin_in_one_step() {
        let mut h = super::super::super::tests::web_board("package_pin_undo");
        let workbook = h.base.join("charts.slate");
        h.app.tab_mut().path = Some(workbook.clone());
        let (portal, reference) = artifact_source(
            &h.base,
            &workbook,
            "one",
            "r1",
            b"<html><body>chart</body></html>",
        );
        let node = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(0.0, 0.0, 800.0, 450.0),
            NodeKind::Portal(portal.clone()),
        );
        let id = h.app.add_nodes(vec![node])[0];
        let copied = package(&workbook, id, &portal, &reference.materialized_entry).unwrap();
        let entry = copied.entry.clone();
        let (tx, rx) = bounded(1);
        assert!(tx.send(Ok(copied)).is_ok());
        let tab = h.app.tab().id;
        h.app.web.package = Some(PackageJob {
            tab,
            workbook: workbook.clone(),
            node: id,
            portal: portal.clone(),
            rx,
        });
        let depth = h.app.tab().journal.undo_depth();
        h.app.web_package_pump(&h.ctx);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        let after = h.app.doc().scene.node(id).unwrap();
        assert!(slate_doc::scene::artifact(after)
            .unwrap()
            .packaged
            .is_some());
        let NodeKind::Portal(p) = &after.kind else {
            unreachable!()
        };
        assert_eq!(
            resolve_web_source(Some(&workbook), &p.source.as_ref().unwrap().locator),
            entry
        );
        h.app.board_undo();
        let NodeKind::Portal(restored) = &h.app.doc().scene.node(id).unwrap().kind else {
            unreachable!()
        };
        assert_eq!(restored, &portal);
        h.app.board_redo();
        assert!(
            slate_doc::scene::artifact(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .packaged
                .is_some()
        );
    }

    #[test]
    fn completion_requires_the_original_tab_path_and_portal() {
        let mut harness = super::super::super::tests::web_board("package_identity");
        let app = &mut harness.app;
        let path = PathBuf::from("dashboard.slate");
        app.tab_mut().path = Some(path.clone());
        app.paste_web_url("https://example.com/dashboard", Pos2::ZERO);
        let node = app.doc().scene.nodes.last().unwrap();
        let id = node.id;
        let NodeKind::Portal(portal) = &node.kind else {
            unreachable!()
        };
        let portal = portal.clone();
        let (_tx, rx) = bounded(1);
        let job = PackageJob {
            tab: app.tab().id,
            workbook: path,
            node: id,
            portal,
            rx,
        };
        assert!(job.matches(app));
        app.tab_mut().path = Some("renamed.slate".into());
        assert!(!job.matches(app));
        app.tab_mut().path = Some(job.workbook.clone());
        app.bind_web_source(id, "changed".into());
        assert!(!job.matches(app));
        app.new_tab();
        assert!(!job.matches(app));
    }
}
