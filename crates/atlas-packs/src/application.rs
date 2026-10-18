//! Installation offers for application connectors. Session capabilities and
//! authority are established separately by the connector protocol.

use atlas_connector::ConnectorDescriptor;

use crate::PackId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationOffer {
    pub id: PackId,
    pub title: String,
    pub descriptor: ConnectorDescriptor,
}

impl ApplicationOffer {
    /// Pure format eligibility; never probes or reads the selected file.
    pub fn supports_extension(&self, extension: &str) -> bool {
        self.descriptor.extensions.iter().any(|supported| {
            supported
                .trim_start_matches('.')
                .eq_ignore_ascii_case(extension.trim_start_matches('.'))
        })
    }
}
