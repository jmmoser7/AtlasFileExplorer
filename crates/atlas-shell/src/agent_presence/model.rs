//! Borrowed display data and explicit human actions. Provider/session policy stays
//! in the application; this module never connects a harness or owns chat storage.
pub struct ChatEntry<'a> {
    pub id: &'a str,
    pub label: &'a str,
}
pub struct ProviderOption<'a> {
    pub id: &'a str,
    pub label: &'a str,
}
pub struct ConversationMessage<'a> {
    pub id: &'a str,
    pub text: &'a str,
    pub outgoing: bool,
    pub system: bool,
    pub status: &'a str,
}
/// Application-selected pending work. The shell owns presentation, never the
/// command payload, session policy, or the decision's authority.
pub struct ProposalDecision<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub recovery_required: bool,
    /// This exact decision is currently being witnessed by the application.
    pub applying: bool,
}
pub struct Participant<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub hat: &'a str,
    pub status: &'a str,
    pub provider: &'a str,
    pub human: bool,
    pub paused: bool,
    pub autonomous: bool,
    pub file_inputs: bool,
    pub own_chats: bool,
    pub parent_name: Option<&'a str>,
    pub chats: &'a [ChatEntry<'a>],
    pub local_profile: bool,
    pub photo_texture: Option<eframe::egui::load::SizedTexture>,
    pub busy: bool,
    /// The bounded slice selected by RosterState::message_range.
    pub messages: &'a [ConversationMessage<'a>],
    /// Full durable history length, including turns outside the visible page.
    pub message_total: usize,
    pub proposals: &'a [ProposalDecision<'a>],
    pub draft: &'a str,
    pub queued_provider: Option<&'a str>,
    pub model: &'a str,
    pub models: &'a [ProviderOption<'a>],
}
pub struct RetiredProfile<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub hat: &'a str,
    pub provider: &'a str,
}
#[derive(Debug, Clone, PartialEq)]
pub enum PresenceAction {
    AcceptProposal {
        participant: String,
        id: String,
    },
    RejectProposal {
        participant: String,
        id: String,
    },
    Add {
        name: String,
        hat: String,
        provider: String,
    },
    Rename {
        id: String,
        name: String,
    },
    SetHat {
        id: String,
        hat: String,
    },
    SetProvider {
        id: String,
        provider: String,
    },
    SwitchProvider {
        id: String,
        provider: String,
        after_task: bool,
    },
    CancelProviderSwitch {
        id: String,
    },
    SetModel {
        id: String,
        model: String,
    },
    SetPhoto {
        id: String,
    },
    RemovePhoto {
        id: String,
    },
    Draft {
        id: String,
        text: String,
    },
    CopyIdentity {
        id: String,
    },
    CopyLink {
        id: String,
    },
    OpenChat {
        id: String,
        chat: String,
    },
    ShowConversation {
        id: String,
    },
    Send {
        id: String,
        text: String,
    },
    Pause {
        id: String,
        paused: bool,
    },
    Cancel {
        id: String,
    },
    SetAutonomy {
        id: String,
        enabled: bool,
    },
    SetFileInputs {
        id: String,
        enabled: bool,
    },
    SetOwnChats {
        id: String,
        enabled: bool,
    },
    Revoke {
        id: String,
    },
    Retire {
        id: String,
        style: String,
    },
    Restore {
        id: String,
    },
    SpawnHelper {
        id: String,
    },
    Clone {
        id: String,
    },
    Schedule {
        id: String,
    },
    Locate {
        id: String,
    },
    ShowActivity {
        id: String,
    },
    ConnectPeople,
    Retry {
        id: String,
        message: String,
    },
}
