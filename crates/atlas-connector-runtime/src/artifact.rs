//! Worker-only artifact storage and transfer. No path or URL is accepted.
//!
//! A sender binds the derived, authenticated pipe before publishing the control
//! descriptor. On that pipe: descriptor JSON, nonempty raw byte frames (<=4MiB),
//! completion JSON, verified receiver receipt JSON. One deadline covers the
//! entire exchange. A receipt lets the sender close without losing unread bytes.

use crate::{
    deadline::Deadline,
    transport::{Cancellation, PipeAddress, PipeConnection, PipeListener},
};
use atlas_connector::{
    ArtifactCompletion, ArtifactReceipt, ArtifactTransfer, FrameError, ProtocolError,
    MAX_ARTIFACT_BYTES, MAX_FRAME_BYTES,
};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fmt, io, sync::Arc, time::Duration};

pub const MAX_STORE_TRANSFERS: usize = 128;

#[derive(Debug)]
pub enum ArtifactError {
    InvalidDescriptor(ProtocolError),
    InvalidBudget,
    BudgetExceeded,
    TooManyTransfers,
    IdConflict,
    DescriptorMismatch,
    HashMismatch,
    UnexpectedEnd,
    InvalidCompletion,
    Io(io::Error),
    Frame(FrameError),
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDescriptor(e) => write!(f, "invalid artifact descriptor: {e}"),
            Self::InvalidBudget => f.write_str("artifact budget must be in 1..64MiB"),
            Self::BudgetExceeded => f.write_str("artifact exceeds the session byte budget"),
            Self::TooManyTransfers => f.write_str("too many artifacts retained in this session"),
            Self::IdConflict => f.write_str("artifact transfer ID is already retained"),
            Self::DescriptorMismatch => {
                f.write_str("artifact pipe descriptor does not match control reply")
            }
            Self::HashMismatch => f.write_str("artifact SHA-256 does not match its descriptor"),
            Self::UnexpectedEnd => f.write_str("artifact pipe ended before the complete transfer"),
            Self::InvalidCompletion => f.write_str("invalid artifact completion or receipt"),
            Self::Io(e) => write!(f, "artifact I/O: {e}"),
            Self::Frame(e) => write!(f, "artifact frame: {e}"),
        }
    }
}

impl std::error::Error for ArtifactError {}
impl From<io::Error> for ArtifactError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<FrameError> for ArtifactError {
    fn from(e: FrameError) -> Self {
        Self::Frame(e)
    }
}

struct StoredArtifact {
    descriptor: ArtifactTransfer,
    bytes: Arc<[u8]>,
}

/// One session's immutable artifacts, bounded by bytes and descriptor count.
/// IDs are never silently replaced. Drop the store when its session ends.
pub struct ArtifactStore {
    entries: HashMap<String, StoredArtifact>,
    budget: usize,
    used: usize,
}

impl Default for ArtifactStore {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            budget: MAX_ARTIFACT_BYTES as usize,
            used: 0,
        }
    }
}

impl ArtifactStore {
    pub fn new(byte_budget: usize) -> Result<Self, ArtifactError> {
        validate_budget(byte_budget)?;
        Ok(Self {
            budget: byte_budget,
            ..Self::default()
        })
    }

    /// Hashing is bulk work: insert on a worker, never during canvas paint.
    pub fn insert(
        &mut self,
        transfer_id: impl Into<String>,
        media_type: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<ArtifactTransfer, ArtifactError> {
        let mut descriptor = ArtifactTransfer {
            transfer_id: transfer_id.into(),
            media_type: media_type.into(),
            byte_length: bytes.len() as u64,
            sha256: "0".repeat(64),
        };
        descriptor
            .validate()
            .map_err(ArtifactError::InvalidDescriptor)?;
        if self.entries.contains_key(&descriptor.transfer_id) {
            return Err(ArtifactError::IdConflict);
        }
        if self.entries.len() == MAX_STORE_TRANSFERS {
            return Err(ArtifactError::TooManyTransfers);
        }
        if bytes.len() > self.budget - self.used {
            return Err(ArtifactError::BudgetExceeded);
        }
        descriptor.sha256 = sha256(&bytes);
        self.used += bytes.len();
        self.entries.insert(
            descriptor.transfer_id.clone(),
            StoredArtifact {
                descriptor: descriptor.clone(),
                bytes: bytes.into(),
            },
        );
        Ok(descriptor)
    }

    /// Clones a shared reference, never copies the retained payload.
    pub fn get(&self, transfer_id: &str) -> Option<Arc<[u8]>> {
        self.entries
            .get(transfer_id)
            .map(|entry| Arc::clone(&entry.bytes))
    }

    pub fn descriptor(&self, transfer_id: &str) -> Option<&ArtifactTransfer> {
        self.entries.get(transfer_id).map(|entry| &entry.descriptor)
    }

    pub fn used_bytes(&self) -> usize {
        self.used
    }
    pub fn byte_budget(&self) -> usize {
        self.budget
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Derivation is SHA256(control token ASCII + one NUL byte + transfer ID UTF-8).
/// It supplies a name, not authority: normal pipe user/PID checks still apply.
pub fn transfer_address(
    control: &PipeAddress,
    descriptor: &ArtifactTransfer,
) -> Result<PipeAddress, ArtifactError> {
    descriptor
        .validate()
        .map_err(ArtifactError::InvalidDescriptor)?;
    let mut hash = Sha256::new();
    hash.update(control.token().as_bytes());
    hash.update([0]);
    hash.update(descriptor.transfer_id.as_bytes());
    Ok(PipeAddress::from_token(format!("{:x}", hash.finalize()))?)
}

pub struct ArtifactSender {
    listener: PipeListener,
    descriptor: ArtifactTransfer,
    bytes: Arc<[u8]>,
}

impl ArtifactSender {
    /// Worker-only. Bind BEFORE sending the descriptor on the control pipe.
    pub fn bind(
        control: &PipeAddress,
        descriptor: ArtifactTransfer,
        bytes: Arc<[u8]>,
        expected_receiver_pid: u32,
    ) -> Result<Self, ArtifactError> {
        verify_bytes(&descriptor, &bytes)?;
        let listener = PipeListener::bind(
            transfer_address(control, &descriptor)?,
            expected_receiver_pid,
        )?;
        Ok(Self {
            listener,
            descriptor,
            bytes,
        })
    }

    pub fn descriptor(&self) -> &ArtifactTransfer {
        &self.descriptor
    }
    pub fn address(&self) -> &PipeAddress {
        self.listener.address()
    }

    /// Worker-only. Includes accept, all chunks, and the verified receipt in one deadline.
    pub fn send(self, timeout: Duration, cancel: &Cancellation) -> Result<(), ArtifactError> {
        let budget = Deadline::new(timeout, cancel)?;
        let mut pipe = self.listener.accept(budget.remaining()?, cancel)?;
        pipe.send(&self.descriptor, budget.remaining()?, cancel)?;
        for chunk in self.bytes.chunks(MAX_FRAME_BYTES) {
            pipe.send_bytes(chunk, budget.remaining()?, cancel)?;
        }
        pipe.send(
            &ArtifactCompletion::new(&self.descriptor),
            budget.remaining()?,
            cancel,
        )?;
        let receipt: ArtifactReceipt = pipe
            .recv(budget.remaining()?, cancel)?
            .ok_or(ArtifactError::UnexpectedEnd)?;
        if receipt != ArtifactReceipt::new(&self.descriptor) {
            return Err(ArtifactError::InvalidCompletion);
        }
        pipe.close();
        Ok(())
    }
}

/// Worker-only. Validate the descriptor and receiver budget before connecting or
/// allocating payload memory. Never returns unverified or incomplete bytes.
pub fn receive_artifact(
    control: &PipeAddress,
    descriptor: &ArtifactTransfer,
    expected_sender_pid: u32,
    byte_budget: usize,
    timeout: Duration,
    cancel: &Cancellation,
) -> Result<Vec<u8>, ArtifactError> {
    descriptor
        .validate()
        .map_err(ArtifactError::InvalidDescriptor)?;
    validate_budget(byte_budget)?;
    if descriptor.byte_length > byte_budget as u64 {
        return Err(ArtifactError::BudgetExceeded);
    }
    let budget = Deadline::new(timeout, cancel)?;
    let mut pipe = PipeConnection::connect(
        transfer_address(control, descriptor)?,
        expected_sender_pid,
        budget.remaining()?,
        cancel,
    )?;
    let announced: ArtifactTransfer = pipe
        .recv(budget.remaining()?, cancel)?
        .ok_or(ArtifactError::UnexpectedEnd)?;
    if &announced != descriptor {
        return Err(ArtifactError::DescriptorMismatch);
    }
    let length = descriptor.byte_length as usize;
    let mut bytes = Vec::with_capacity(length);
    while bytes.len() < length {
        let chunk = pipe
            .recv_bytes(
                (length - bytes.len()).min(MAX_FRAME_BYTES),
                budget.remaining()?,
                cancel,
            )?
            .ok_or(ArtifactError::UnexpectedEnd)?;
        bytes.extend_from_slice(&chunk);
    }
    let completion: ArtifactCompletion = pipe
        .recv(budget.remaining()?, cancel)?
        .ok_or(ArtifactError::UnexpectedEnd)?;
    if completion != ArtifactCompletion::new(descriptor) {
        return Err(ArtifactError::InvalidCompletion);
    }
    verify_bytes(descriptor, &bytes)?;
    budget.remaining()?;
    pipe.send(
        &ArtifactReceipt::new(descriptor),
        budget.remaining()?,
        cancel,
    )?;
    Ok(bytes)
}

fn verify_bytes(descriptor: &ArtifactTransfer, bytes: &[u8]) -> Result<(), ArtifactError> {
    descriptor
        .validate()
        .map_err(ArtifactError::InvalidDescriptor)?;
    if bytes.len() as u64 != descriptor.byte_length {
        return Err(ArtifactError::DescriptorMismatch);
    }
    if sha256(bytes) != descriptor.sha256 {
        return Err(ArtifactError::HashMismatch);
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_budget(budget: usize) -> Result<(), ArtifactError> {
    if budget == 0 || budget > MAX_ARTIFACT_BYTES as usize {
        return Err(ArtifactError::InvalidBudget);
    }
    Ok(())
}
