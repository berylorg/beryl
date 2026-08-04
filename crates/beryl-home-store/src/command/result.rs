use std::{error::Error, fmt};

use beryl_model::{DomainRevision, HomeRevision, RevisionError};
use thiserror::Error;

use crate::{
    DomainCallbackSource, DomainHandle, HealthGateError, HomeGeneration, HomeStore, ReadError,
    StorageDomain, domain::StoreInstanceId, health::FailureSeverity,
};

/// Domain-callback stage that surfaced a storage-owned access failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContributorCallbackStage {
    /// Validate current participant state before assembling mutations.
    Validation,
    /// Assemble the participant's typed pending mutations.
    Contribution,
}

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

/// Successful durable mutation-command revisions.
///
/// Validation-only participants do not appear in the affected-domain set.
#[derive(Clone, Eq, PartialEq)]
pub struct CommitReceipt {
    pub(crate) store: StoreInstanceId,
    pub(crate) generation: HomeGeneration,
    pub(crate) home_revision: HomeRevision,
    pub(crate) domains: Vec<(usize, DomainRevision)>,
}

impl fmt::Debug for CommitReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommitReceipt")
            .field("generation", &self.generation)
            .field("home_revision", &self.home_revision)
            .field("affected_domain_count", &self.domains.len())
            .finish_non_exhaustive()
    }
}

impl CommitReceipt {
    /// Returns the new complete-home revision.
    #[must_use]
    pub const fn home_revision(&self) -> HomeRevision {
        self.home_revision
    }

    /// Returns the exact process-local healthy generation that committed the command.
    #[must_use]
    pub const fn generation(&self) -> HomeGeneration {
        self.generation
    }

    pub(crate) fn domain_revision<D: StorageDomain>(
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

/// Why a successful command receipt cannot authorize a current publication.
#[derive(Debug, Error)]
pub enum CommitReceiptError {
    /// The process-wide health gate is not accepting state-dependent work.
    #[error(transparent)]
    HealthGate(#[from] HealthGateError),
    /// Fjall reported retained maintenance failure before the receipt result could publish.
    #[error("receipt revision lookup could not confirm storage health: {source}")]
    StorageHealth {
        /// Stable classified engine source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// A panic poisoned the in-process home generation lock.
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,
    /// The receipt belongs to another or obsolete healthy store generation.
    #[error(
        "command receipt belongs to another or obsolete Beryl-home generation: receipt {receipt_generation:?}, current {current_generation:?}"
    )]
    StaleOrForeign {
        /// Generation that durably completed the command.
        receipt_generation: HomeGeneration,
        /// Current healthy generation asked to accept the result.
        current_generation: HomeGeneration,
    },
    /// The typed domain handle belongs to another or obsolete registration.
    #[error("domain handle `{domain}` does not belong to this home generation")]
    ForeignDomain {
        /// Stable typed domain name.
        domain: &'static str,
    },
}

impl HomeStore {
    /// Returns one affected domain revision only when the receipt still belongs
    /// to this store's exact current healthy generation.
    pub fn receipt_domain_revision<D: StorageDomain>(
        &self,
        receipt: &CommitReceipt,
        handle: DomainHandle<D>,
    ) -> Result<Option<DomainRevision>, CommitReceiptError> {
        let admission = self.health.admit()?;
        let generation_guard = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(CommitReceiptError::GenerationPoisoned);
            }
        };
        let generation = match generation_guard.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(CommitReceiptError::GenerationPoisoned);
            }
        };
        if receipt.generation != admission.generation() || receipt.store != generation.instance_id {
            return Err(CommitReceiptError::StaleOrForeign {
                receipt_generation: receipt.generation,
                current_generation: admission.generation(),
            });
        }
        if generation.resolve_domain(handle).is_none() {
            return Err(CommitReceiptError::ForeignDomain { domain: D::NAME });
        }
        let revision = receipt.domain_revision(handle);
        admission.confirm_database(&generation.database, |source| {
            CommitReceiptError::StorageHealth {
                source: Box::new(source),
            }
        })?;
        drop(generation_guard);
        Ok(revision)
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
    /// Validation-only participants cannot produce a durable command result by themselves.
    #[error("home command contains validation participants but no mutation participant")]
    ValidationOnlyCommand,
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
    /// A storage-owned read or sidecar access failed inside a domain callback.
    #[error("domain `{domain}` failed storage access during {stage:?}: {source}")]
    ContributorAccess {
        /// Stable typed domain name.
        domain: &'static str,
        /// Exact callback stage.
        stage: ContributorCallbackStage,
        /// Exact typed storage-owned source.
        #[source]
        source: DomainCallbackSource,
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
