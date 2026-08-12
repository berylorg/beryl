use std::{fmt, num::NonZeroUsize, time::Duration};

use beryl_model::BerylHomeId;
use thiserror::Error;

use crate::HomeGeneration;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableThemeFileId(String);

impl StableThemeFileId {
    pub fn new(value: impl Into<String>) -> Result<Self, StableThemeFileIdError> {
        let value = value.into();
        let bytes = value.as_bytes();
        let edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        if !(1..=64).contains(&bytes.len())
            || !edge(bytes[0])
            || !edge(bytes[bytes.len() - 1])
            || !bytes.iter().all(|byte| edge(*byte) || *byte == b'-')
        {
            return Err(StableThemeFileIdError);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StableThemeFileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("stable theme ids must be 1..=64 lowercase ASCII letters/digits with interior hyphens")]
pub struct StableThemeFileIdError;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThemeFileIdentity {
    length: u64,
    sha256: [u8; 32],
}

impl ThemeFileIdentity {
    #[must_use]
    pub const fn new(length: u64, sha256: [u8; 32]) -> Self {
        Self { length, sha256 }
    }
    #[must_use]
    pub const fn length(self) -> u64 {
        self.length
    }
    #[must_use]
    pub const fn sha256(self) -> [u8; 32] {
        self.sha256
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ThemeFileSelector {
    Manifest,
    Document(StableThemeFileId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeOperationLimits {
    max_source_bytes: u64,
    io_buffer_bytes: NonZeroUsize,
    max_staged_files: NonZeroUsize,
    max_evidence_files: NonZeroUsize,
    max_evidence_bytes: NonZeroUsize,
}

impl ThemeOperationLimits {
    pub fn new(
        max_source_bytes: u64,
        io_buffer_bytes: NonZeroUsize,
        max_staged_files: NonZeroUsize,
        max_evidence_files: NonZeroUsize,
        max_evidence_bytes: NonZeroUsize,
    ) -> Result<Self, ThemeOperationLimitsError> {
        if max_source_bytes == 0 || max_staged_files.get() > 2 || max_evidence_files.get() > 4 {
            return Err(ThemeOperationLimitsError);
        }
        Ok(Self {
            max_source_bytes,
            io_buffer_bytes,
            max_staged_files,
            max_evidence_files,
            max_evidence_bytes,
        })
    }
    #[must_use]
    pub const fn max_source_bytes(self) -> u64 {
        self.max_source_bytes
    }
    #[must_use]
    pub const fn io_buffer_bytes(self) -> NonZeroUsize {
        self.io_buffer_bytes
    }
    #[must_use]
    pub const fn max_staged_files(self) -> NonZeroUsize {
        self.max_staged_files
    }
    #[must_use]
    pub const fn max_evidence_files(self) -> NonZeroUsize {
        self.max_evidence_files
    }
    #[must_use]
    pub const fn max_evidence_bytes(self) -> NonZeroUsize {
        self.max_evidence_bytes
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("theme operation limits must be nonzero and permit at most two staged/four evidence files")]
pub struct ThemeOperationLimitsError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRepositorySnapshot {
    pub(crate) home_id: BerylHomeId,
    pub(crate) store_instance: u64,
    pub(crate) generation: HomeGeneration,
    pub(crate) manifest: Option<ThemeFileIdentity>,
}

impl ThemeRepositorySnapshot {
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }
    #[must_use]
    pub const fn generation(&self) -> HomeGeneration {
        self.generation
    }
    #[must_use]
    pub const fn manifest_identity(&self) -> Option<ThemeFileIdentity> {
        self.manifest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeFileRange {
    pub(crate) identity: ThemeFileIdentity,
    pub(crate) offset: u64,
    pub(crate) bytes: Vec<u8>,
    pub(crate) eof: bool,
}

impl ThemeFileRange {
    #[must_use]
    pub const fn identity(&self) -> ThemeFileIdentity {
        self.identity
    }
    #[must_use]
    pub const fn total_length(&self) -> u64 {
        self.identity.length()
    }
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    #[must_use]
    pub const fn eof(&self) -> bool {
        self.eof
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeCommitEvidence {
    pub(crate) snapshot: ThemeRepositorySnapshot,
    pub(crate) document: Option<(StableThemeFileId, ThemeFileIdentity)>,
    pub(crate) later_failure: Option<ThemeRepositoryStage>,
}

impl ThemeCommitEvidence {
    #[must_use]
    pub const fn snapshot(&self) -> &ThemeRepositorySnapshot {
        &self.snapshot
    }
    #[must_use]
    pub fn document(&self) -> Option<(&StableThemeFileId, ThemeFileIdentity)> {
        self.document.as_ref().map(|(id, identity)| (id, *identity))
    }
    #[must_use]
    pub const fn later_failure(&self) -> Option<ThemeRepositoryStage> {
        self.later_failure
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeReconciliationEvidence {
    pub(crate) home_id: BerylHomeId,
    pub(crate) operation: super::operations::ThemeOperationEvidence,
}

impl ThemeReconciliationEvidence {
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }
}

#[derive(Debug)]
pub enum ThemeMutationOutcome {
    NotCommitted,
    Committed(ThemeCommitEvidence),
    Indeterminate(ThemeReconciliationEvidence),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeReconciliationOutcome {
    ExactOld(ThemeRepositorySnapshot),
    ExactNew(ThemeCommitEvidence),
    Collision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRepositoryStage {
    Snapshot,
    Verify,
    ReadRange,
    DocumentWrite,
    DocumentSync,
    DocumentReplace,
    DocumentRemove,
    InstalledDirectorySync,
    ManifestWrite,
    ManifestSync,
    ManifestReplace,
    ThemesDirectorySync,
    ConfirmHealth,
}

#[derive(Debug, Error)]
pub enum ThemeRepositoryError {
    #[error("theme repository snapshot is stale or belongs to another store")]
    StaleSnapshot,
    #[error("theme reconciliation evidence belongs to another home")]
    ForeignEvidence,
    #[error("theme repository file is absent")]
    FileAbsent,
    #[error("theme repository file identity did not match the expected exact identity")]
    IdentityMismatch,
    #[error("theme repository operation exceeded an explicit bound")]
    LimitExceeded,
    #[error("theme repository source was shorter, longer, or different from its intended identity")]
    SourceMismatch,
    #[error("theme repository internal lock is poisoned")]
    LockPoisoned,
    #[error("theme repository access rejected by home health: {0}")]
    Health(#[from] crate::HealthGateError),
    #[error("theme repository failed at {stage:?}: {source}")]
    Io {
        stage: ThemeRepositoryStage,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ThemeWatchHint {
    ManifestChanged,
    DocumentChanged(StableThemeFileId),
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeWatchLimits {
    interval: Duration,
    queue_capacity: NonZeroUsize,
    max_entries_per_poll: NonZeroUsize,
    max_file_bytes: u64,
    io_buffer_bytes: NonZeroUsize,
}

impl ThemeWatchLimits {
    pub fn new(
        interval: Duration,
        queue_capacity: NonZeroUsize,
        max_entries_per_poll: NonZeroUsize,
        max_file_bytes: u64,
        io_buffer_bytes: NonZeroUsize,
    ) -> Result<Self, ThemeWatchLimitsError> {
        if interval.is_zero() || max_file_bytes == 0 {
            return Err(ThemeWatchLimitsError);
        }
        Ok(Self {
            interval,
            queue_capacity,
            max_entries_per_poll,
            max_file_bytes,
            io_buffer_bytes,
        })
    }
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }
    #[must_use]
    pub const fn queue_capacity(self) -> NonZeroUsize {
        self.queue_capacity
    }
    #[must_use]
    pub const fn max_entries_per_poll(self) -> NonZeroUsize {
        self.max_entries_per_poll
    }
    #[must_use]
    pub const fn max_file_bytes(self) -> u64 {
        self.max_file_bytes
    }
    #[must_use]
    pub const fn io_buffer_bytes(self) -> NonZeroUsize {
        self.io_buffer_bytes
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("theme watcher limits must be nonzero")]
pub struct ThemeWatchLimitsError;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ThemeWatchError {
    #[error("the theme watcher is already subscribed for this store generation")]
    AlreadySubscribed,
    #[error("the theme watcher is shut down")]
    ShutDown,
    #[error("theme watcher internal state is poisoned")]
    LockPoisoned,
    #[error("theme watcher access rejected by home health: {0}")]
    Health(#[from] crate::HealthGateError),
}

pub struct ThemeWatchSubscription {
    pub(crate) shared: std::sync::Arc<super::watcher::WatchShared>,
    pub(crate) worker: Option<std::thread::JoinHandle<()>>,
}

impl ThemeWatchSubscription {
    pub fn try_recv(&self) -> Result<Option<ThemeWatchHint>, ThemeWatchError> {
        self.shared.try_recv()
    }
    pub fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Option<ThemeWatchHint>, ThemeWatchError> {
        self.shared.recv_timeout(timeout)
    }
    pub fn shutdown(mut self) {
        self.shutdown_inner();
    }
    fn shutdown_inner(&mut self) {
        self.shared.shutdown();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for ThemeWatchSubscription {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}
