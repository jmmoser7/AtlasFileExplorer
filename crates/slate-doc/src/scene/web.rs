//! Journaled web-host parameters.
use super::*;

/// Journaled web-portal parameters. The locator itself lives in
/// [`PortalNode::source`]; everything derived from loading it — poster texture,
/// scroll offset, page history, pool membership — is never stored here (D31).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebPortalRef {
    /// Entry file for a directory source. Ignored for file and URL sources.
    pub entry: String,
    pub viewport: WebViewport,
    /// `None` means "decide from the source kind": iframe for local material,
    /// poster for a remote page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export: Option<WebExport>,
    /// A presentation-only dashboard can be pinned to its poster.
    pub interactive_allowed: bool,
    pub poster_capture: WebPosterCapture,
    /// Optional dashboard-generation capability; ordinary pages omit it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Box<ArtifactGeneratorRef>>,
}

/// Default entry file for a directory source.
pub const WEB_DEFAULT_ENTRY: &str = "index.html";

impl Default for WebPortalRef {
    fn default() -> Self {
        Self {
            entry: WEB_DEFAULT_ENTRY.to_string(),
            viewport: WebViewport::default(),
            export: None,
            interactive_allowed: true,
            poster_capture: WebPosterCapture::OnBind,
            artifact: None,
        }
    }
}
