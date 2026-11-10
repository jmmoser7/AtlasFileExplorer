//! One explicitly authorized auxiliary control through the production Client.
//! Reads the retained native public request; supplies no expected answer/hints.
use atlas_agent::{AgentRequest, AgentSession, ContextItem, InputSlot};
use atlas_subscription::{Client, Provider};
use serde_json::json;
use std::{path::PathBuf, sync::atomic::AtomicBool};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 4 {
        return Err("Expected retained native session, retained workbook, new evidence link directory, isolated owned working directory".into());
    }
    let retained: AgentSession = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    let workbook: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let user = retained
        .turns
        .iter()
        .rev()
        .find(|turn| turn.role == "user")
        .ok_or("Retained native session has no user turn")?;
    let source = workbook["scene"]["nodes"]
        .as_array()
        .ok_or("No scene")?
        .iter()
        .find(|node| node["id"] == 2)
        .and_then(|node| node["kind"]["image"]["text_override"].as_str())
        .ok_or("Retained native source node2 has no authored override")?;
    let link = PathBuf::from(&args[2]);
    let cwd = PathBuf::from(&args[3]);
    std::fs::create_dir_all(&link)?;
    std::fs::create_dir_all(&cwd)?;
    let mut request = AgentRequest {
        id: format!("auxiliary-{}", atlas_agent::request_id()),
        prompt: user.text.clone(),
        prompt_inputs_projected: true,
        oneshot: true,
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
        ..Default::default()
    };
    request.inputs.wired.push(ContextItem {
        node: 2,
        text: source.into(),
        images: vec![],
        outputs: Default::default(),
        active: None,
        depth: None,
        slot: Some(InputSlot::Prompt),
    });
    let mut client = Client::start(Provider::Cursor, &link, &cwd)?;
    let mut session = AgentSession::default();
    let result = client.run(&request, &mut session, &AtomicBool::new(false), |state| {
        let _ = std::fs::write(
            link.join("public-session.json"),
            serde_json::to_vec_pretty(state).unwrap(),
        );
    });
    std::fs::write(
        link.join("public-session.json"),
        serde_json::to_vec_pretty(&session)?,
    )?;
    let answer = session
        .turns
        .iter()
        .rev()
        .find(|turn| turn.role == "assistant")
        .map(|turn| turn.text.as_str())
        .unwrap_or_default();
    std::fs::write(link.join("response.txt"), answer)?;
    let receipt = json!({"scope":"ONE authorized auxiliary production-Client control; not native UI acceptance","retained_request":retained.request,"request":request.id,"provider":"cursor-cli","account_default":true,"memory_on":false,"source_node":2,"source_chars":source.chars().count(),"public_user_chars":request.prompt.chars().count(),"result_ok":result.is_ok(),"error":result.as_ref().err(),"response_utf8_bytes":answer.len()});
    std::fs::write(
        link.join("control-receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("{receipt}");
    result.map_err(Into::into)
}
