use super::*;

impl Background {
    pub(super) fn start(ctx: &egui::Context) -> Self {
        let (requests, jobs) = crossbeam_channel::unbounded();
        let (done, results) = crossbeam_channel::unbounded();
        let cancel = Cancellation::default();
        let stop = cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut worker = None;
            let mut startup_failure = None;
            while let Ok(work) = jobs.recv() {
                if stop.is_cancelled() {
                    break;
                }
                match work {
                    Work::Close(viewer) => {
                        if let Some(warm) = worker.as_mut() {
                            if close_viewer(warm, &viewer, &stop).is_err() {
                                worker = None;
                            }
                        }
                    }
                    Work::Solve(job) => {
                        if job.retry_startup {
                            startup_failure = None;
                            if worker.as_ref().is_some_and(Worker::is_recovery) {
                                worker = None;
                            }
                        }
                        let stage = |message| {
                            let _ = done.send(WorkerEvent::Stage {
                                viewer: job.viewer,
                                instance: job.instance,
                                generation: job.generation,
                                message,
                            });
                            ctx.request_repaint();
                        };
                        let result = run_job(&mut worker, &mut startup_failure, &job, &stop, stage);
                        let _ = done.send(WorkerEvent::Finished(Box::new(Completion {
                            viewer: job.viewer,
                            instance: job.instance,
                            source: job.source,
                            generation: job.generation,
                            was_bake: job.bake.is_some(),
                            result,
                        })));
                        ctx.request_repaint();
                    }
                }
            }
        });
        Self {
            requests,
            results,
            cancel,
            busy: None,
        }
    }
}

fn close_viewer(worker: &mut Worker, viewer_id: &str, stop: &Cancellation) -> Result<(), String> {
    worker
        .request(
            &Request {
                action: "close",
                viewer_id,
                source_path: None,
                source_revision: None,
                overrides: None,
                inputs: None,
            },
            stop,
        )
        .map(|_| ())
}

pub(super) fn viewer_id(doc: u64, instance: u64) -> String {
    format!("{doc:016x}{instance:016x}")
}

pub(super) fn source_revision(source: &Source) -> Result<String, String> {
    let metadata = std::fs::metadata(&source.path)
        .map_err(|error| format!("Could not read the Grasshopper definition: {error}"))?;
    Ok(format!(
        "{}:{}:{:?}",
        source.key,
        metadata.len(),
        metadata.modified()
    ))
}

fn run_job(
    worker: &mut Option<Worker>,
    startup_failure: &mut Option<String>,
    job: &Job,
    stop: &Cancellation,
    stage: impl Fn(&'static str),
) -> Result<Solved, SolveError> {
    if let Some(error) = startup_failure {
        return Err(error.clone().into());
    }
    if atlas_core::cloud::is_dehydrated(&job.source.path) {
        return Err(
            "Make the Grasshopper definition available locally, then retry."
                .to_owned()
                .into(),
        );
    }
    let revision = source_revision(&job.source)?;
    if job
        .bake
        .as_ref()
        .is_some_and(|bake| bake.revision != revision)
    {
        return Err(SolveError::SourceChanged);
    }
    let config = application_connector::headless_rhino_configuration()?;
    if worker.is_none() {
        stage("Starting headless Rhino and loading Grasshopper components…");
        *worker = Some(start_worker(&config, stop, startup_failure)?);
    }
    stage(if job.bake.is_some() {
        "Baking visible geometry…"
    } else {
        "Solving Grasshopper definition…"
    });
    let viewer = viewer_id(job.viewer.0, job.instance);
    let overrides = serde_json::to_value(&job.overrides).map_err(|error| error.to_string())?;
    let inputs = serde_json::to_value(&job.inputs).map_err(|error| error.to_string())?;
    let request = Request {
        action: if job.bake.is_some() { "bake" } else { "solve" },
        viewer_id: &viewer,
        source_path: Some(&job.source.path),
        source_revision: Some(&revision),
        overrides: Some(&overrides),
        inputs: Some(&inputs),
    };
    let mut output = match worker
        .as_mut()
        .expect("worker initialized")
        .request(&request, stop)
    {
        Ok(output) => output,
        Err(error) => {
            *worker = None;
            return Err(error.into());
        }
    };
    if output.response.status == "missing_packages" && job.bake.is_none() {
        packages::Restorer {
            worker,
            config: &config,
            startup_failure,
            stop,
            stage: &stage,
        }
        .resolve(&mut output, &request, &job.local_plugins);
    }
    let preview = if job.bake.is_none() {
        output.preview_path().map(|path| {
            stage("Preparing the 3D preview…");
            crate::app::model3d::parse_grasshopper_preview(&path)
        })
    } else {
        None
    };
    let bake = if let (Some(path), Some(bake)) = (output.bake_path(), &job.bake) {
        let draft = output::Draft::reserve(bake.destination.clone())?;
        std::fs::copy(path, draft.path())
            .map_err(|error| format!("Could not copy the Grasshopper bake: {error}"))?;
        Some(BakedFile(Some(draft.commit_unique()?)))
    } else {
        None
    };
    if source_revision(&job.source).as_deref() != Ok(revision.as_str()) {
        return Err(SolveError::SourceChanged);
    }
    Ok(Solved {
        output,
        revision,
        preview,
        bake,
    })
}

pub(super) fn start_worker(
    config: &atlas_connector_runtime::headless_rhino::HeadlessRhino,
    stop: &Cancellation,
    failure: &mut Option<String>,
) -> Result<Worker, String> {
    Worker::start(
        &config.executable,
        &config.rhino_system,
        &config.workspace,
        stop,
    )
    .inspect_err(|error| *failure = Some(error.clone()))
}

pub(super) fn response_status(output: &Output) -> String {
    output
        .response
        .diagnostics
        .first()
        .cloned()
        .or_else(|| output.response.error.clone())
        .unwrap_or_else(|| match output.response.status.as_str() {
            "ready" => "No visible preview geometry".into(),
            "missing_packages" => "Grasshopper needs additional packages".into(),
            _ => "Grasshopper did not complete the solve".into(),
        })
}
