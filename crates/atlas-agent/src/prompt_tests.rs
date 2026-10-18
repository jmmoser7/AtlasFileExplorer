use crate::*;
#[test]
fn only_explicit_wires_extend_a_prompt_and_transport_is_not_displayed() {
    let mut req = AgentRequest { id:"r".into(), prompt:"hello".into(),
        inputs: serde_json::from_value(serde_json::json!({"revision":"internal","context":[{"node":4,"text":"ambient","images":[]}],"wired":[]})).unwrap(), ..Default::default() };
    let plain = req.input_text();
    assert!(plain.starts_with("hello"));
    assert!(!plain.contains("application/slate-link+json"));
    assert_eq!(display_prompt(&plain), "hello");
    req.inputs.wired = req.inputs.context.clone();
    let text = req.input_text();
    assert!(text.contains("ambient"));
    assert!(!text.contains("revision"));
    assert_eq!(display_prompt(&text), "hello");
    assert_eq!(
        display_prompt(
            "hello\n\nCanvas input snapshot (data):\n{\"revision\":\"internal\",\"context\":[],\"wired\":[]}"
        ),
        "hello"
    );
    let literal = "Explain Canvas input snapshot (data):\nnot metadata";
    assert_eq!(display_prompt(literal), literal);
}

fn request(output_dir: Option<&str>) -> AgentRequest {
    AgentRequest {
        id: "r".into(),
        prompt: "make a chart".into(),
        output_dir: output_dir.map(Into::into),
        ..Default::default()
    }
}

#[test]
fn the_goal_guide_rides_only_on_goal_turns_and_is_not_displayed() {
    let plain = request(None).input_text();
    assert!(!plain.contains(GOAL_MARKER));
    let mut builder = request(None);
    builder.goal = Some(GoalTurn {
        goal: " The chart loads the CSV ".into(),
        claims: true,
        in_reply: false,
    });
    builder.inputs.wired = vec![ContextItem {
        node: 1,
        text: "wired".into(),
        images: vec![],
        outputs: Default::default(),
        active: None,
        depth: None,
        slot: None,
    }];
    let link = std::path::Path::new("C:\\ws\\.atlas-ai\\agent\\s1");
    let text = builder.input_text_in(Some(link));
    let guide = text.split_once(GOAL_MARKER).unwrap().1;
    assert!(guide.starts_with("Goal: The chart loads the CSV\nWhen you believe"));
    assert!(guide.contains(
        "\"goal\":{\"status\":\"met\",\"reason\":\"one line\"} to C:/ws/.atlas-ai/agent/s1/return.json (beside session.json)"
    ));
    assert!(
        text.find(ARTIFACT_MARKER) < text.find(GOAL_MARKER),
        "after the artifact guide"
    );
    assert_eq!(display_prompt(&text), "make a chart");
    assert_eq!(
        display_prompt(&format!(
            "make a chart{GOAL_MARKER}{}",
            goal_guide_in(builder.goal.as_ref().unwrap(), None)
        )),
        "make a chart"
    );
    let forged = format!("make a chart{GOAL_MARKER}not a guide");
    assert_eq!(display_prompt(&forged), forged);

    let judge = goal_guide_in(
        &GoalTurn {
            goal: "g".into(),
            claims: false,
            in_reply: false,
        },
        None,
    );
    assert!(judge.contains("\"status\":\"met\"|\"not_met\""));
    assert!(judge.contains("to return.json beside session.json for this message"));
}

/// First hands-on use, 28 September 2026: a read-only reviewer's
/// `return.json` write was refused by its sandbox, so it reports the
/// verdict as the reply's last line instead, and never writes a file.
#[test]
fn a_read_only_turn_reports_its_verdict_in_the_reply() {
    let judge = GoalTurn {
        goal: "tests pass".into(),
        claims: false,
        in_reply: true,
    };
    let guide = goal_guide_in(&judge, Some("C:/ws/.atlas-ai/agent/s1"));
    assert!(guide.starts_with("Goal: tests pass\n"), "{guide}");
    assert!(guide.contains("Slate goal: {\"status\":\"met\"|\"not_met\""));
    assert!(!guide.contains("return.json"), "{guide}");
    let mut request = request(None);
    request.goal = Some(judge.clone());
    assert_eq!(display_prompt(&request.input_text()), "make a chart");
    assert!(serde_json::to_string(&request)
        .unwrap()
        .contains("\"in_reply\":true"));
    let plain = GoalTurn {
        in_reply: false,
        ..judge
    };
    assert!(!serde_json::to_string(&plain).unwrap().contains("in_reply"));

    assert_eq!(
        goal_in_reply(
            "Looks right.\n\nSlate goal: {\"status\":\"met\",\"reason\":\"tests pass\"}\n"
        ),
        Some(GoalVerdict {
            status: GoalStatus::Met,
            reason: "tests pass".into()
        })
    );
    assert_eq!(
        goal_in_reply("`Slate goal: {\"status\":\"not_met\"}`").map(|v| v.status),
        Some(GoalStatus::NotMet)
    );
    for prose in [
        "The goal is met.",
        "Slate goal: {\"status\":\"met\"}\nbut one more thing",
        "Slate goal: met",
        "Slate goal: {\"status\":\"done\"}",
        "",
    ] {
        assert_eq!(goal_in_reply(prose), None, "{prose:?}");
    }
}

#[test]
fn crosstalk_fields_are_absent_from_ordinary_requests() {
    let text = serde_json::to_string(&request(None)).unwrap();
    assert!(!text.contains("policy") && !text.contains("relayed_from") && !text.contains("goal"));
    let relayed: AgentRequest = serde_json::from_str(
        r#"{"id":"r","prompt":"p","at":0,"policy":"read_only","relayed_from":"Codex · reviewer","goal":{"goal":"g","claims":false}}"#,
    )
    .unwrap();
    assert_eq!(relayed.policy, TurnPolicy::ReadOnly);
    assert!(!relayed.policy.is_default());
    assert_eq!(relayed.relayed_from.as_deref(), Some("Codex · reviewer"));
    assert!(!relayed.goal.as_ref().unwrap().claims);
    let round: AgentRequest =
        serde_json::from_str(&serde_json::to_string(&relayed).unwrap()).unwrap();
    assert_eq!(round, relayed);
}

#[test]
fn the_guide_names_the_output_folder_only_when_known() {
    let dir = "C:/ws/slate-outputs/board/2026-09-23-chart-a1b2c3";
    let text = request(Some(dir)).input_text();
    assert!(text.contains(&format!("in {dir}: deliverables at the top")));
    assert!(text.contains("\"as\":\"auto\""));
    assert!(text.contains("scratch/"));
    assert_eq!(display_prompt(&text), "make a chart");
    let plain = request(None).input_text();
    assert!(!plain.contains("deliverables at the top"));
    assert!(plain.contains("write return.json beside session.json"));
    assert_eq!(display_prompt(&plain), "make a chart");
    assert_eq!(artifact_guide(Some("  ")), artifact_guide(None));
    assert!(artifact_guide(None).starts_with("Every file you create or change"));
    let named = request(Some(dir))
        .input_text_in(Some(std::path::Path::new("C:\\ws\\.atlas-ai\\agent\\s1\\")));
    assert!(
        named.contains("write C:/ws/.atlas-ai/agent/s1/return.json (beside session.json) naming")
    );
    assert_eq!(display_prompt(&named), "make a chart");
    assert_eq!(request(None).input_text_in(None), plain);
}

#[test]
#[allow(deprecated)]
fn transcripts_with_the_older_guide_still_strip() {
    let old = format!(
        "hello{SLATE_LINK_MARKER}{SLATE_LINK_GUIDE}{FILE_ATLAS_PLACE_MARKER}{FILE_ATLAS_PLACE_GUIDE}{ARTIFACT_MARKER}{ARTIFACT_GUIDE}"
    );
    assert_eq!(display_prompt(&old), "hello");
    let wired = format!("{old}\n\nSlate wired attachments (data):\n[]");
    assert_eq!(display_prompt(&wired), "hello");
    let new = format!("hello{ARTIFACT_MARKER}{}", artifact_guide(Some("D:/out")));
    assert_eq!(display_prompt(&new), "hello");
    let forged = "hello\n\nChanged documents:\nsomething else";
    assert_eq!(display_prompt(forged), forged);
}
