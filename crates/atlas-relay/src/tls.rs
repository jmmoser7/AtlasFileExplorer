use crate::RelayError;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, sync::Arc};

const SERVER_NAME: &str = "atlas-relay.local";
const INVITE_PREFIX: &str = "slate-participant-v1:";

/// Bearer room invite. It must be shared through a trusted channel. Its
/// certificate is public; its credential is private. Never place it in a
/// workbook, export, URL query, diagnostic log, or an agent prompt.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invite {
    pub(crate) endpoint: String,
    pub(crate) room: String,
    pub(crate) credential: String,
    pub(crate) certificate_der: Vec<u8>,
    pub(crate) certificate_sha256: String,
}

impl std::fmt::Debug for Invite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Invite")
            .field("endpoint", &self.endpoint)
            .field("credential", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl Invite {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn certificate_fingerprint(&self) -> &str {
        &self.certificate_sha256
    }

    /// Explicit copy/share operation, rather than `Display` or `Debug`.
    pub fn encode(&self) -> Result<String, RelayError> {
        serde_json::to_string(self)
            .map(|s| format!("{INVITE_PREFIX}{s}"))
            .map_err(|_| RelayError::Invalid("Could not encode relay invite"))
    }

    pub fn decode(text: &str) -> Result<Self, RelayError> {
        if text.len() > 24 * 1024 {
            return Err(RelayError::Invalid("Invite is too large"));
        }
        let json = text
            .trim()
            .strip_prefix(INVITE_PREFIX)
            .ok_or(RelayError::Invalid("Not a Slate participant invite"))?;
        let invite: Self =
            serde_json::from_str(json).map_err(|_| RelayError::Invalid("Invalid relay invite"))?;
        invite.validate()?;
        Ok(invite)
    }

    /// Replace only the advertised numeric address when exposing a host to
    /// another machine. The original room identity and TLS pin are retained.
    pub fn with_address(mut self, addr: SocketAddr) -> Result<Self, RelayError> {
        if addr.ip().is_unspecified() || addr.port() == 0 {
            return Err(RelayError::Invalid(
                "Invite needs a reachable address and port",
            ));
        }
        self.endpoint = format!("wss://{addr}/participants");
        self.validate()?;
        Ok(self)
    }

    pub(crate) fn validate(&self) -> Result<SocketAddr, RelayError> {
        let url = url::Url::parse(&self.endpoint)
            .map_err(|_| RelayError::Invalid("Invalid relay endpoint"))?;
        if url.scheme() != "wss"
            || url.path() != "/participants"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(RelayError::Invalid(
                "Relay endpoint must be a plain secure WebSocket address",
            ));
        }
        let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
        let ip: std::net::IpAddr = host
            .parse()
            .map_err(|_| RelayError::Invalid("Relay endpoint needs a numeric IP address"))?;
        let port = url
            .port_or_known_default()
            .ok_or(RelayError::Invalid("Relay endpoint needs a port"))?;
        if ip.is_unspecified() || port == 0 {
            return Err(RelayError::Invalid(
                "Relay endpoint needs a reachable address and port",
            ));
        }
        if !crate::valid_id(&self.room)
            || self.room.len() != 32
            || self.credential.len() != 64
            || !self.credential.bytes().all(|c| c.is_ascii_hexdigit())
            || self.certificate_der.is_empty()
            || self.certificate_der.len() > 4096
            || digest(&self.certificate_der) != self.certificate_sha256
        {
            return Err(RelayError::Invalid(
                "Invalid room identity or certificate pin",
            ));
        }
        Ok(SocketAddr::new(ip, port))
    }
}

pub(crate) fn server(addr: SocketAddr) -> Result<(Invite, Arc<rustls::ServerConfig>), RelayError> {
    let key = rcgen::generate_simple_self_signed(vec![SERVER_NAME.to_owned()])
        .map_err(|_| RelayError::Invalid("Could not create relay certificate"))?;
    let der = key.cert.der().to_vec();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(tls_error)?
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(der.clone())],
            PrivatePkcs8KeyDer::from(key.key_pair.serialize_der()).into(),
        )
        .map_err(tls_error)?;
    let invite = Invite {
        endpoint: format!("wss://{addr}/participants"),
        room: random_hex(16)?,
        credential: random_hex(32)?,
        certificate_sha256: digest(&der),
        certificate_der: der,
    };
    Ok((invite, Arc::new(config)))
}

pub(crate) fn client(invite: &Invite) -> Result<rustls::ClientConnection, RelayError> {
    invite.validate()?;
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from(invite.certificate_der.clone()))
        .map_err(tls_error)?;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(tls_error)?
    .with_root_certificates(roots)
    .with_no_client_auth();
    // Ordinary WebPKI validation checks the signature, lifetime and fixed SAN.
    // There is no permissive verifier and no global machine trust-store edit.
    rustls::ClientConnection::new(Arc::new(config), ServerName::try_from(SERVER_NAME).unwrap())
        .map_err(tls_error)
}

fn random_hex(bytes: usize) -> Result<String, RelayError> {
    let mut value = vec![0; bytes];
    rustls::crypto::ring::default_provider()
        .secure_random
        .fill(&mut value)
        .map_err(|_| RelayError::Invalid("Operating-system randomness is unavailable"))?;
    Ok(value.iter().map(|b| format!("{b:02x}")).collect())
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn tls_error(err: impl std::fmt::Display) -> RelayError {
    RelayError::Transport(err.to_string())
}
