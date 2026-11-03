//! Durable locator for a borrowed application surface. Runtime handles never persist.
use crate::scene::{NodeId, PortalClass, PortalKind, PortalNode, SourceUri};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationSurface {
    pub parent: NodeId,
    pub surface: String,
    pub open_command: String,
}
impl ApplicationSurface {
    pub fn new(parent: NodeId, surface: &str, command: &str) -> Option<Self> {
        if parent.0 == 0 || !valid(surface) || !valid(command) {
            return None;
        }
        Some(Self {
            parent,
            surface: surface.into(),
            open_command: command.into(),
        })
    }
    pub fn locator(&self) -> String {
        format!(
            "slate-surface/v1/{}/{}/{}",
            self.parent.0, self.surface, self.open_command
        )
    }
    pub fn parse(text: &str) -> Option<Self> {
        let fields: Vec<_> = text.split('/').collect();
        if fields.len() != 5 || fields[0] != "slate-surface" || fields[1] != "v1" {
            return None;
        }
        Self::new(NodeId(fields[2].parse().ok()?), fields[3], fields[4])
    }
    pub fn portal(&self, title: &str) -> PortalNode {
        let mut portal = PortalNode::unbound_web(title);
        portal.kind = PortalKind::Application;
        portal.class = PortalClass::Host;
        portal.web = None;
        portal.source = Some(SourceUri {
            locator: self.locator(),
        });
        portal
    }
    pub fn from_portal(portal: &PortalNode) -> Option<Self> {
        (portal.kind == PortalKind::Application).then_some(())?;
        Self::parse(&portal.source.as_ref()?.locator)
    }
}
fn valid(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locator_roundtrip_has_only_durable_identity() {
        let value =
            ApplicationSurface::new(NodeId(7), "grasshopper", "org.slate.rhino.grasshopper_show")
                .unwrap();
        assert_eq!(ApplicationSurface::parse(&value.locator()), Some(value));
        for bad in [
            "slate-surface/v1/0/gh/open",
            "slate-surface/v1/7/./open",
            "slate-surface/v1/7/../open",
            "slate-surface/v1/7/gh/..",
            "slate-surface/v1/7/gh/open/123",
            "12345",
        ] {
            assert!(ApplicationSurface::parse(bad).is_none());
        }
    }
}
