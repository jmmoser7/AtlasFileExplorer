//! Actual production helper initialization only: synthetic routing, no native authority.
use atlas_agent::{
    collaborator::{transport, BoardLinkSession, HostToolsLaunch},
    AgentRequest,
};
use std::path::Path;
fn local_fixture(_: &Path) -> bool {
    false
}
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(result) = atlas_subscription::host_tools::run_if_requested(&args, local_fixture) {
        return result;
    }
    let root = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join("target/provider-setup-20261109")
        .join(format!("component-host-readiness-{}", std::process::id()));
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let cwd = root.join("isolated cwd");
    std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
    let request = AgentRequest {
        id: "component-readiness-request".into(),
        collaborator: Some(BoardLinkSession {
            session_id: "model-task-component-readiness".into(),
            grant_id: "synthetic-no-native-grant".into(),
            workspace_id: "synthetic-no-native-workspace".into(),
        }),
        ..Default::default()
    };
    let session = request.collaborator.as_ref().unwrap();
    let dir = session.transport_dir(&root);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("board-session.json"),
        serde_json::to_vec(&transport::ready_value(session, &request.id)).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    let launch = HostToolsLaunch::model_task(
        std::env::current_exe().map_err(|e| e.to_string())?,
        &root,
        &cwd,
        &request,
    )?;
    let result = atlas_subscription::probe_claude_host_tools(&cwd, &request, &launch)?;
    let receipt = serde_json::json!({"kind":"component-only-production-server-initialize","result":result,"native_slate_main":false,"tool_calls":0,"user_turns":0});
    std::fs::write(
        root.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    println!("{}", receipt);
    Ok(())
}
