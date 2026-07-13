use std::error::Error;

use beryl_model::{DomainRevision, HomeRevision, RevisionError};
use thiserror::Error;

use crate::{DomainHandle, ReadError, StorageDomain, domain::StoreInstanceId};

/// Deterministic stale-revision fact returned before validation or assembly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RevisionConflict {
    /// The complete home revision changed.
    Home {
        /// Revision supplied by the caller.
        expected: HomeRevision,
        /// Revision observed on the serialized writer snapshot.
        current: HomeRevision,
    },
    /// One participating logical-domain revision changed.
    Domain {
        /// Stable domain name.
        domain: &'static str,
        /// Revision supplied by the caller.
        expected: DomainRevision,
        /// Revision observed on the serialized writer snapshot.
        current: DomainRevision,
    },
}

/// Successful durable command revisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    pub(crate) store: StoreInstanceId,
    pub(crate) home_revision: HomeRevision,
    pub(crate) domains: Vec<(usize, DomainRevision)>,
}

impl CommitReceipt {
    /// Returns the new complete-home revision.
    #[must_use]
    pub const fn home_revision(&self) -> HomeRevision {
        self.home_revision
    }

    /// Returns the new revision for one participating typed domain.
    #[must_use]
    pub fn domain_revision<D: StorageDomain>(
        &self,
        handle: DomainHandle<D>,
    ) -> Option<DomainRevision> {
        if handle.store != self.store {
            return None;
        }
        self.domains
            .iter()
            .find_map(|(slot, revision)| (*slot == handle.slot).then_some(*revision))
    }
}

/// Why a serialized revision-checked command did not reach durable success.
#[derive(Debug, Error)]
pub enum CommandError {
    /// The process-wide health gate is not accepting state-dependent work.
    #[error(transparent)]
    HealthGate(#[from] crate::HealthGateError),
    /// Cancellation was observed before this command acquired writer admission.
    #[error("command was cancelled before writer admission")]
    CancelledBeforeAdmission,
    /// The same thread attempted to enter this store's writer recursively.
    #[error("reentrant use of the same Beryl-home writer is forbidden")]
    ReentrantWriter,
    /// A prior panic poisoned the process-wide writer mutex.
    #[error("the Beryl-home writer mutex is poisoned")]
    WriterPoisoned,
    /// A panic poisoned the in-process home generation lock.
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,
    /// A retained sidecar belongs to another or obsolete healthy generation.
    #[error("sidecar admission does not belong to this healthy home generation")]
    ForeignSidecar,
    /// A command must mutate at least one registered domain.
    #[error("home command contains no domain contributions")]
    EmptyCommand,
    /// A contribution carries a handle from another store generation.
    #[error("domain contribution `{domain}` does not belong to this home generation")]
    ForeignDomain {
        /// Stable typed domain name.
        domain: &'static str,
    },
    /// Persistent domain schema/family authority changed or became malformed.
    #[error("registered domain `{domain}` no longer matches its persistent declaration")]
    DomainRegistrationInvariant {
        /// Stable typed domain name.
        domain: &'static str,
    },
    /// Expected revisions did not match the serialized writer snapshot.
    #[error("command conflicts with {conflicts_len} current revision(s)")]
    Conflict {
        /// Number repeated for concise error display.
        conflicts_len: usize,
        /// Home first, then domains sorted by stable name.
        conflicts: Vec<RevisionConflict>,
    },
    /// A domain rejected current authoritative state.
    #[error("domain `{domain}` rejected command validation: {source}")]
    ContributorValidation {
        /// Stable typed domain name.
        domain: &'static str,
        /// Domain-owned source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The registered authoritative-domain validator rejected current state.
    #[error("domain `{domain}` failed registered invariant validation: {source}")]
    DomainValidation {
        /// Stable typed domain name.
        domain: &'static str,
        /// Domain-owned validator source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// A domain failed while building its typed pending mutation set.
    #[error("domain `{domain}` failed command contribution: {source}")]
    ContributorAssembly {
        /// Stable typed domain name.
        domain: &'static str,
        /// Domain-owned source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// A domain validated but contributed no record mutation.
    #[error("domain `{domain}` contributed no record mutation")]
    EmptyContribution {
        /// Stable typed domain name.
        domain: &'static str,
    },
    /// A required home or domain revision is exhausted.
    #[error("cannot advance {scope} revision: {source}")]
    RevisionExhausted {
        /// Human-readable revision scope.
        scope: String,
        /// Pure revision failure.
        #[source]
        source: RevisionError,
    },
    /// A required revision or domain registry record could not be read.
    #[error("command could not read current revision metadata: {source}")]
    RevisionRead {
        /// Typed read source.
        #[source]
        source: ReadError,
    },
    /// Pending domain metadata could not be encoded.
    #[error("command could not encode revision metadata: {source}")]
    Metadata {
        /// Bounded encoder source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The one physical Fjall batch failed before durable success.
    #[error("cross-domain batch commit failed: {source}")]
    Commit {
        /// Engine source hidden behind the package boundary.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The required post-commit `SyncAll` barrier failed.
    #[error("cross-domain batch persistence barrier failed: {source}")]
    Persistence {
        /// Engine source hidden behind the package boundary.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

impl CommandError {
    /// Returns deterministic conflict facts when this is a stale command.
    #[must_use]
    pub fn conflicts(&self) -> Option<&[RevisionConflict]> {
        match self {
            Self::Conflict { conflicts, .. } => Some(conflicts),
            _ => None,
        }
    }
}
