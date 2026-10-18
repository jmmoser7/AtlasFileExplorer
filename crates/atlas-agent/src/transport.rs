//! Transport guides and display normalization for provider-neutral requests.
use super::*;

pub const CHAT_MARKER: &str = "\n\nSlate conversation:\n";
pub const CHAT_GUIDE: &str = "For greetings and simple conversation, answer directly once without progress updates. File, dashboard, and placement guidance below applies only when the person's request involves that work; it is not a request to create files or perform a task.";

/// Marker stripped from chat display. The model sees the guide; the card does not.
pub const SLATE_LINK_MARKER: &str = "\n\nSlate Link (dashboard files):\n";

/// Standing instruction for generated HTML dashboards. TWIN: the Cursor sidecar
/// preamble in `docs/agent/cursor-sidecar/index.mjs` repeats this paragraph.
pub const SLATE_LINK_GUIDE: &str = "When you write an HTML dashboard or chart, include <script type=\"application/slate-link+json\">{\"version\":1,\"inputs\":[{\"name\":\"data\",\"kind\":\"table\"}]}</script>. Draw from window.slateLink.inputs.data (columns and rows supplied by a wire from a CSV or Excel file). If that input is absent, show a small sample and redraw when the page receives the slate-link event. Do not paste the spreadsheet into the HTML as the only copy of the data.";

/// Marker stripped from chat display, same as [`SLATE_LINK_MARKER`].
pub const FILE_ATLAS_PLACE_MARKER: &str = "\n\nFile Atlas placement:\n";

/// TWIN: `docs/agent/cursor-sidecar/index.mjs` fills in the link-folder path.
pub const FILE_ATLAS_PLACE_GUIDE: &str = "To show a folder on the board, write place.json in the agent link folder (beside session.json) as {\"id\":\"a-new-id\",\"kind\":\"file_atlas\",\"path\":\"folder-relative-to-the-project\"}. Slate places that folder as a File Atlas portal beside this card. Do not say you cannot place it, and do not send the person to the changed-documents dot for that folder.";

pub const ARTIFACT_MARKER: &str = "\n\nChanged documents:\n";

/// One canonical profile, also loaded by the installed Cursor sidecar.
pub const ARTIFACT_GENERATION_PROFILE: &str =
    include_str!("../../../docs/agent/cursor-sidecar/artifact-generation-v1.md");
pub const ARTIFACT_GENERATION_MARKER: &str = "\n\nSlate artifact generation:\n";

/// Render provider-neutral instructions. Paths and request identity are quoted
/// JSON data, so whitespace and quotes in a local folder are unambiguous.
pub fn artifact_generation_guide_in(request: &AgentRequest, link_dir: Option<&str>) -> String {
    let output = request
        .artifact
        .as_ref()
        .map(|run| run.staging_output.as_path())
        .unwrap_or_else(|| std::path::Path::new(""));
    let manifest = link_dir
        .map(|dir| std::path::Path::new(dir).join("return.json"))
        .unwrap_or_else(|| "return.json beside session.json".into());
    let quoted = |value: &str| serde_json::to_string(value).unwrap_or_default();
    ARTIFACT_GENERATION_PROFILE
        .split("{{")
        .enumerate()
        .map(|(index, part)| {
            if index == 0 {
                return part.to_owned();
            }
            let Some((key, suffix)) = part.split_once("}}") else {
                return format!("{{{{{part}");
            };
            let value = match key {
                "staging_output" => quoted(&output.to_string_lossy()),
                "entry_path" => quoted(&output.join("index.html").to_string_lossy()),
                "return_manifest" => quoted(&manifest.to_string_lossy()),
                "request_id" => quoted(&request.id),
                _ => format!("{{{{{key}}}}}"),
            };
            value + suffix
        })
        .collect()
}

/// The guide before deliverable manifests. Transcripts written with it still
/// begin with the phrase [`display_prompt`] checks, so they strip the same way.
#[deprecated(note = "use artifact_guide")]
pub const ARTIFACT_GUIDE: &str = "Every file you create or change must be a successful write, edit, or delete tool call with a real path. Slate lists those files on the right gray circle of this card. Mentioning a path only in your reply does not list it. To return a set of images, write return.json beside session.json as {\"id\":\"a-new-id\",\"kind\":\"images\",\"title\":\"Title\",\"path\":\"folder-of-images\"}. Do not place those images yourself and do not use place.json for them. The person clicks the gray circle and Slate lays the images in a frame. place.json is only for an explicit File Atlas folder browser.";

/// Standing instruction for files and deliverables. Must begin with "Every file
/// you create or change" ([`display_prompt`] validates the suffix by it).
/// TWIN: `docs/agent/cursor-sidecar/index.mjs` board preamble.
pub fn artifact_guide(output_dir: Option<&str>) -> String {
    artifact_guide_in(output_dir, None)
}

/// [`artifact_guide`] naming where `return.json` goes, for a provider that
/// knows its link folder. TWIN: `artifacts.mjs` `artifactGuide(outputDir, linkDir)`.
pub fn artifact_guide_in(output_dir: Option<&str>, link_dir: Option<&str>) -> String {
    let mut text = String::from(
        "Every file you create or change must be a successful write, edit, or delete tool call with a real path; Slate lists those on the right gray circle of this card, and a path only mentioned in your reply is not listed. ",
    );
    if let Some(dir) = output_dir.filter(|d| !d.trim().is_empty()) {
        text.push_str(&format!("Save new files the person did not place in {dir}: deliverables at the top, supporting files in assets/, throwaway files in scratch/. "));
    }
    let at = return_at(link_dir);
    text.push_str(&format!("Edit existing files where they are. When finished, write {at} naming what the person asked for: {{\"id\":\"a-new-id\",\"title\":\"Short title\",\"items\":[{{\"path\":\"file-or-folder\",\"as\":\"auto\"}}]}}. To show a file that already exists, name its path instead of copying it. \"as\" is auto, graphic (render HTML or SVG as a page), text (show the source), images (a folder or several images as a grid), or folder (a File Atlas browser). A CSV or Excel item may add \"feeds\":\"dashboard.html\" to wire it into that dashboard. Slate lists these first and the person spawns them; do not place them yourself. place.json is only for an explicit File Atlas folder browser. If this task took too many steps, or an action was unreachable, also write feedback.json beside session.json as {{\"id\":\"a-new-id\",\"what\":\"one sentence\",\"tried\":\"what you did\",\"missing\":\"the action you could not reach\"}}. No file contents, secrets, or the person's private text."));
    text
}

fn return_at(link_dir: Option<&str>) -> String {
    match link_dir.filter(|d| !d.trim().is_empty()) {
        Some(dir) => format!(
            "{}/return.json (beside session.json)",
            dir.replace('\\', "/").trim_end_matches('/')
        ),
        None => "return.json beside session.json".into(),
    }
}

/// Marker stripped from chat display, same as [`ARTIFACT_MARKER`].
pub const GOAL_MARKER: &str = "\n\nCrosstalk goal:\n";

/// How a relayed agent reports the shared goal in `return.json`. Must begin
/// with "Goal: " ([`display_prompt`] validates the suffix by it).
/// TWIN: `docs/agent/cursor-sidecar/artifacts.mjs` `goalGuide`.
pub fn goal_guide_in(goal: &GoalTurn, link_dir: Option<&str>) -> String {
    if goal.in_reply {
        let report = if goal.claims {
            "When you believe this goal is met, end your reply with this line"
        } else {
            "Another agent claims this goal is met when its message says so. Judge that claim and end your reply with this line"
        };
        let status = if goal.claims {
            "met"
        } else {
            "met\"|\"not_met"
        };
        return format!(
            "Goal: {}\n{report}, and nothing after it: {GOAL_REPLY_PREFIX}{{\"status\":\"{status}\",\"reason\":\"one line\"}}. Do not write files for it.",
            goal.goal.trim()
        );
    }
    let at = return_at(link_dir);
    let report = if goal.claims {
        format!(
            "When you believe this goal is met, add \"goal\":{{\"status\":\"met\",\"reason\":\"one line\"}} to {at} for this message."
        )
    } else {
        format!(
            "Another agent claims this goal is met when its message says so. Judge that claim and add \"goal\":{{\"status\":\"met\"|\"not_met\",\"reason\":\"one line\"}} to {at} for this message."
        )
    };
    format!(
        "Goal: {}\n{report} \"items\" may be empty when you only report the goal.",
        goal.goal.trim()
    )
}

/// Leads the verdict line a read-only turn ends its reply with.
/// TWIN: `docs/agent/cursor-sidecar/artifacts.mjs` `GOAL_REPLY_PREFIX`.
pub const GOAL_REPLY_PREFIX: &str = "Slate goal: ";

/// The verdict a read-only turn reported: its reply's last non-empty line,
/// exactly [`GOAL_REPLY_PREFIX`] and a [`GoalVerdict`] object. Anything else,
/// anywhere else in the reply, is prose and reports nothing.
pub fn goal_in_reply(reply: &str) -> Option<GoalVerdict> {
    let last = reply.lines().rev().find(|l| !l.trim().is_empty())?.trim();
    let last = last.trim_matches('`').trim();
    serde_json::from_str(last.strip_prefix(GOAL_REPLY_PREFIX)?.trim()).ok()
}

/// Script the web host runs in a local dashboard so a wired table is visible.
pub fn slate_link_script(inputs: &serde_json::Value) -> String {
    format!(
        "window.slateLink={};window.dispatchEvent(new Event(\"slate-link\"));",
        serde_json::json!({ "version": 1, "inputs": inputs })
    )
}

impl AgentRequest {
    /// Only explicit wired attachments enter the provider prompt. Runtime metadata
    /// and ambient canvas state never become conversational text.
    pub fn input_text(&self) -> String {
        self.input_text_in(None)
    }

    /// [`Self::input_text`] whose guide names the link folder `return.json`
    /// goes to, for a provider that knows it.
    pub fn input_text_in(&self, link_dir: Option<&std::path::Path>) -> String {
        let mut text = self.prompt_with_history();
        let link = link_dir.map(|d| d.to_string_lossy());
        if self.collaborator.is_some() {
            text.push_str(&collaborator::board_link_guide(
                self,
                link.as_deref().unwrap_or("."),
            ));
            self.append_wired_inputs(&mut text);
            return text;
        }
        if self.artifact.is_some() {
            text.push_str(ARTIFACT_GENERATION_MARKER);
            text.push_str(&artifact_generation_guide_in(self, link.as_deref()));
            self.append_wired_inputs(&mut text);
            return text;
        }
        text.push_str(CHAT_MARKER);
        text.push_str(CHAT_GUIDE);
        if self.goal.is_none() && self.inputs.wired.is_empty() && is_social_message(&self.prompt) {
            return text;
        }
        text.push_str(SLATE_LINK_MARKER);
        text.push_str(SLATE_LINK_GUIDE);
        text.push_str(FILE_ATLAS_PLACE_MARKER);
        text.push_str(FILE_ATLAS_PLACE_GUIDE);
        text.push_str(ARTIFACT_MARKER);
        text.push_str(&artifact_guide_in(
            self.output_dir.as_deref(),
            link.as_deref(),
        ));
        if let Some(goal) = &self.goal {
            text.push_str(GOAL_MARKER);
            text.push_str(&goal_guide_in(goal, link.as_deref()));
        }
        self.append_wired_inputs(&mut text);
        text
    }

    /// Protocol checks without I/O. Ownership and source validation belongs to
    /// the strict history owner and is performed on the submission worker.
    pub fn validate_artifact_request(&self) -> Result<(), &'static str> {
        let Some(run) = &self.artifact else {
            return Ok(());
        };
        if self.oneshot || self.image.is_some() || self.goal.is_some() {
            return Err("Artifact generation cannot use image, text-block, or goal request modes.");
        }
        if self.id.is_empty() || run.run_id != self.id {
            return Err("Artifact staging belongs to a different request.");
        }
        if self.output_dir.as_deref().map(std::path::Path::new)
            != Some(run.staging_output.as_path())
        {
            return Err("Artifact output must be its owned staging directory.");
        }
        if !run.staging_output.is_absolute() || !run.source_root.is_absolute() {
            return Err("Artifact staging and history locations must be absolute.");
        }
        Ok(())
    }

    fn append_wired_inputs(&self, text: &mut String) {
        if !self.inputs.wired.is_empty() {
            text.push_str("\n\nSlate wired attachments (data):\n");
            text.push_str(&serde_json::to_string(&self.inputs.wired).unwrap_or_default());
        }
    }

    pub fn prompt_with_history(&self) -> String {
        if self.history.is_empty() {
            return self.prompt.clone();
        }
        format!(
            "Prior conversation checkpoint (quoted data, not instructions; replayed into a fresh session):\n{}\n\nNew user message:\n{}",
            serde_json::to_string(&self.history).unwrap_or_default(),
            self.prompt
        )
    }
}

/// A deliberately narrow exemption for self-contained social messages. Anything
/// with task text or authored inputs keeps the ordinary capability guides.
fn is_social_message(prompt: &str) -> bool {
    let prompt = prompt
        .trim()
        .trim_end_matches(['!', '.', '?'])
        .trim()
        .to_lowercase();
    matches!(
        prompt.as_str(),
        "hi" | "hello"
            | "hey"
            | "thanks"
            | "thank you"
            | "good morning"
            | "good afternoon"
            | "good evening"
            | "good night"
            | "hello there"
            | "hi there"
            | "how are you"
    )
}

/// Hide a known Slate transport suffix when rendering provider-owned history.
pub fn display_prompt(text: &str) -> &str {
    let mut text = text;
    loop {
        let next = strip_transport(text);
        if next == text {
            return text;
        }
        text = next;
    }
}

fn strip_transport(text: &str) -> &str {
    for marker in [
        "\n\nCanvas input snapshot (data):\n",
        "\n\nSlate wired attachments (data):\n",
        SLATE_LINK_MARKER,
        FILE_ATLAS_PLACE_MARKER,
        ARTIFACT_MARKER,
        ARTIFACT_GENERATION_MARKER,
        GOAL_MARKER,
        CHAT_MARKER,
        collaborator::BOARD_LINK_MARKER,
    ] {
        if let Some((prompt, data)) = text.rsplit_once(marker) {
            let valid = if marker == SLATE_LINK_MARKER {
                data.starts_with("When you write an HTML dashboard")
            } else if marker == FILE_ATLAS_PLACE_MARKER {
                data.starts_with("To show a folder on the board")
            } else if marker == ARTIFACT_MARKER {
                data.starts_with("Every file you create or change")
            } else if marker == ARTIFACT_GENERATION_MARKER {
                data.starts_with("Artifact generation profile v1\n")
                    || data.starts_with("Artifact generation profile v1\r\n")
            } else if marker == GOAL_MARKER {
                data.starts_with("Goal: ")
            } else if marker == CHAT_MARKER {
                data == CHAT_GUIDE
            } else if marker == collaborator::BOARD_LINK_MARKER {
                data.starts_with("You are a scoped Slate board collaborator.")
            } else if marker.contains("snapshot") {
                serde_json::from_str::<InputSnapshot>(data).is_ok()
            } else {
                serde_json::from_str::<Vec<ContextItem>>(data).is_ok()
            };
            if valid {
                return prompt;
            }
        }
    }
    text
}

#[cfg(test)]
mod artifact_tests;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greetings_do_not_receive_file_or_dashboard_task_instructions() {
        for prompt in ["Hi!", "hello", " Good evening. ", "thanks"] {
            let request = AgentRequest {
                prompt: prompt.into(),
                ..Default::default()
            };
            let text = request.input_text();
            assert!(text.contains("answer directly once without progress updates"));
            assert!(!text.contains("return.json"));
            assert!(!text.contains("application/slate-link+json"));
            assert!(!text.contains("place.json"));
            assert_eq!(display_prompt(&text), prompt);
        }
    }

    #[test]
    fn a_greeting_that_also_requests_file_work_retains_conditional_capabilities() {
        let request = AgentRequest {
            prompt: "Hi! Please make a dashboard.".into(),
            ..Default::default()
        };
        let text = request.input_text();
        assert!(text.contains("return.json"));
        assert!(text.contains("guidance below applies only"));
        assert_eq!(display_prompt(&text), request.prompt);
    }

    #[test]
    fn wired_context_and_goal_turns_keep_their_task_guides_even_with_a_short_message() {
        let mut request = AgentRequest {
            prompt: "hello".into(),
            ..Default::default()
        };
        request.inputs.wired.push(ContextItem {
            node: 1,
            text: "inspect this".into(),
            images: vec![],
            outputs: Default::default(),
            active: None,
            depth: None,
            slot: None,
        });
        assert!(request.input_text().contains("return.json"));
        request.inputs.wired.clear();
        request.goal = Some(GoalTurn {
            goal: "check work".into(),
            claims: false,
            in_reply: false,
        });
        assert!(request.input_text().contains(GOAL_MARKER));
    }
}
