//! Browser state is distinct from source availability.
use super::ago;
use std::time::Duration;

/// Every state a web portal can be in, each of which says itself on the frame
/// rather than leaving it blank (D30).
#[derive(Debug, Clone, PartialEq)]
pub enum WebState {
    /// No locator yet — the draw grammar always commits here (D03).
    Unbound,
    /// Not yet resolved. Neutral, never blocking (P1.portal.health).
    Unknown,
    /// A remote origin the human has not permitted yet (D32).
    Blocked {
        origin: String,
    },
    Loading,
    /// Holding a pool slot and rendering.
    Live,
    /// Eligible, but the pool was full — showing its last frame (D29).
    Budgeted,
    /// Stale by construction, and the age is stated.
    Poster {
        age: Duration,
    },
    Missing {
        locator: String,
    },
    /// The source is present, but the browser could not open or load it.
    Failed {
        reason: String,
    },
    Refused {
        reason: String,
    },
    /// No WebView2 runtime on this machine.
    NoRuntime,
    /// Painted smaller than `LIVE_MIN_PX`.
    TooSmall,
}

impl WebState {
    /// The short word the chrome strip shows.
    pub fn label(&self) -> String {
        match self {
            WebState::Unbound => "unbound".into(),
            WebState::Unknown => "resolving…".into(),
            WebState::Blocked { origin } => format!("blocked · {origin}"),
            WebState::Loading => "loading…".into(),
            WebState::Live => "live".into(),
            WebState::Budgeted => "budgeted".into(),
            WebState::Poster { age } => format!("poster · {}", ago(*age)),
            WebState::Missing { .. } => "missing".into(),
            WebState::Failed { .. } => "could not load".into(),
            WebState::Refused { reason } => format!("refused · {reason}"),
            WebState::NoRuntime => "no WebView2 runtime".into(),
            WebState::TooSmall => "zoom in to run".into(),
        }
    }

    /// The health dot of P1.portal.health — three states, never more.
    pub fn health(&self) -> WebHealth {
        match self {
            WebState::Live | WebState::Budgeted | WebState::Poster { .. } | WebState::Loading => {
                WebHealth::Ok
            }
            WebState::Missing { .. } | WebState::Failed { .. } | WebState::Refused { .. } => {
                WebHealth::Missing
            }
            _ => WebHealth::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebHealth {
    Ok,
    Unknown,
    Missing,
}
