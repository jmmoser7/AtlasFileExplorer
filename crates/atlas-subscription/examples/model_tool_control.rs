//! ONE explicitly authorized diagnostic generation; synthetic IPC, no native authority.
use atlas_agent::{
    collaborator::{BoardLinkSession, HostToolsLaunch},
    AgentRequest, AgentSession,
};
use std::{
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn local_fixture(_: &Path) -> bool {
    false
}
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(result) = atlas_subscription::host_tools::run_if_requested(&args, local_fixture) {
        return result;
    }
    let input = std::env::var("ATLAS_SUBSCRIPTION_CONTROL_REQUEST")
        .map_err(|_| "Explicit captured request path required")?;
    let root = std::path::PathBuf::from(
        std::env::var("ATLAS_SUBSCRIPTION_CONTROL_OUTPUT")
            .map_err(|_| "Explicit owned output directory required")?,
    );
    if !root.is_absolute() || root.exists() {
        return Err("Control output must be a new absolute owned directory".into());
    }
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut request: AgentRequest =
        serde_json::from_slice(&std::fs::read(input).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    request.id = atlas_agent::request_id();
    request.prompt="For this diagnostic control, read the submitted task context, inspect the native model, then reply DONE. Do not generate or change the model.".into();
    request.prompt_inputs_projected = false;
    request.pending_prompt_projection = None;
    request.history.clear();
    request.model = Some("fable".into());
    request.model_parameters = [("effort".into(), "max".into())].into();
    request.collaborator = Some(BoardLinkSession {
        session_id: format!("model-task-{}", request.id),
        grant_id: format!("synthetic-{}", request.id),
        workspace_id: "synthetic-no-native-authority".into(),
    });
    let cwd = root.join("isolated cwd");
    std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
    let launch = HostToolsLaunch::model_task(
        std::env::current_exe().map_err(|e| e.to_string())?,
        &root,
        &cwd,
        &request,
    )?;
    std::fs::write(
        root.join("request.json"),
        serde_json::to_vec_pretty(&request).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    let mut client =
        atlas_subscription::Client::start(atlas_subscription::Provider::ClaudeCode, &root, &cwd)?;
    client.set_cloud_guard(local_fixture);
    let broker = std::env::var("ATLAS_SUBSCRIPTION_CONTROL_BROKER")
        .map_err(|_| "Explicit synthetic broker required")?;
    let mut child = std::process::Command::new("node")
        .arg(broker)
        .arg(&root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let ready = Instant::now() + Duration::from_secs(10);
    while !root.join("broker-ready.json").is_file() {
        if Instant::now() >= ready {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Synthetic broker did not become ready; no model prompt submitted".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let mut session = AgentSession::default();
    let result = client.run_with_tools(
        &request,
        &mut session,
        &AtomicBool::new(false),
        Some(&launch),
        |session| {
            let _ = std::fs::write(
                root.join("session.json"),
                serde_json::to_vec_pretty(session).unwrap(),
            );
        },
    );
    let _ = std::fs::write(root.join("stop-broker"), "");
    let close = Instant::now() + Duration::from_secs(5);
    while child.try_wait().map_err(|e| e.to_string())?.is_none() {
        if Instant::now() >= close {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    std::fs::write(
        root.join("session.json"),
        serde_json::to_vec_pretty(&session).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    let receipt = serde_json::json!({"scope":"ONE auxiliary productionClient+productionhelper synthetic tool-return control; NOTnativePass","native_authority":false,"native_generation_allowed":false,"request":request.id,"selected_model":request.model,"selected_parameters":request.model_parameters,"client_ok":result.is_ok(),"error":result.err(),"public_assistant":session.turns.iter().filter(|turn|turn.role=="assistant").map(|turn|turn.text.as_str()).collect::<Vec<_>>()});
    std::fs::write(
        root.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    println!("{}", receipt);
    Ok(())
}
