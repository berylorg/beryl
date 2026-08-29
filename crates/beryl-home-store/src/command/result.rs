use std::{error::Error, fmt};

use beryl_model::{DomainRevision, HomeRevision, RevisionError};
use thiserror::Error;

use crate::{
    DomainCallbackSource, DomainHandle, HealthGateError, HomeGeneration, HomeStore, ReadError,
    StorageDomain, domain::StoreInstanceId, health::FailureSeverity,
};

/// Beryl-owned name for one configured Fjall storage resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageResource {
    EncodedRangeEndpointBytes,
    ApplicationKeyspaces,
    KeyspaceNameBytes,
    JournalFiles,
    AtomicBatchRecords,
    AtomicBatchEncodedKeyBytes,
    AtomicBatchEncodedValueBytes,
    EncodedJournalRecordBytes,
    DecodedJournalRecordBytes,
    EncodedBlockBytes,
    DecodedBlockBytes,
    EncodedSeparatedValueBytes,
    DecodedSeparatedValueBytes,
    MergeSources,
    TableRecords,
    BlobFileRecords,
    FragmentationRecords,
    VersionHistorySlots,
    SharedBlockCacheBytes,
    MemtablePayloadBytes,
    MemtableRecords,
    Other,
}

/// Beryl-owned stable classification of one retained storage failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageErrorClass {
    Configuration,
    PolicyDenied {
        resource: StorageResource,
        requested: u64,
        limit: u64,
    },
    Corruption,
    Io(std::io::ErrorKind),
    Integrity,
    Poisoned,
    MaintenanceTerminal,
    KeyspaceIdentity,
    Durability,
    Other,
}

/// Beryl-owned exact commit classification retained from a storage mutation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageCommitState {
    NotCommitted,
    Committed,
    Indeterminate,
}

/// Move-only custody for a command whose durable outcome is indeterminate.
///
/// [`Self::install`] synchronously and infallibly transfers the sole descriptor into its exact
/// already-reserved per-home registry scope. Installation performs no reconciliation work.
#[must_use = "indeterminate reconciliation custody must be installed synchronously"]
pub struct ReconciliationCustody {
    pending: Option<PendingReconciliationCustody>,
}

struct PendingReconciliationCustody {
    slot: crate::reconciliation::ReconciliationSlot,
    domains: Vec<crate::command::MaterializedDomainDescriptor>,
    receipt: CommitReceipt,
    successor: Option<crate::successor::SuccessorDescriptor>,
}

impl fmt::Debug for ReconciliationCustody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReconciliationCustody")
            .field(
                "domain_count",
                &self
                    .pending
                    .as_ref()
                    .map_or(0, |pending| pending.domains.len()),
            )
            .finish_non_exhaustive()
    }
}

impl ReconciliationCustody {
    pub(crate) fn new(
        slot: crate::reconciliation::ReconciliationSlot,
        domains: Vec<crate::command::MaterializedDomainDescriptor>,
        receipt: CommitReceipt,
        successor: Option<crate::successor::SuccessorDescriptor>,
    ) -> Self {
        Self {
            pending: Some(PendingReconciliationCustody {
                slot,
                domains,
                receipt,
                successor,
            }),
        }
    }

    /// Installs custody into its originating home registry.
    ///
    /// This synchronous call returns no failure and only transfers ownership. It starts no reread,
    /// retry, rollback, publication, hook, worker, task, or reconciliation execution.
    pub fn install(mut self) {
        let _ = self.install_pending();
    }

    /// Installs custody and returns the exact operation handle used to trigger or join it.
    ///
    /// Like [`Self::install`], installation itself performs no reconciliation work.
    #[must_use]
    pub fn install_and_handle(mut self) -> crate::ReconciliationHandle {
        self.install_pending()
            .expect("live reconciliation custody contains its move-only pending state")
    }

    fn install_pending(&mut self) -> Option<crate::ReconciliationHandle> {
        let Some(pending) = self.pending.take() else {
            return None;
        };
        Some(pending.slot.install(RetainedReconciliationDescriptor {
            domains: pending.domains,
            receipt: pending.receipt,
            successor: pending.successor,
        }))
    }
}

impl Drop for ReconciliationCustody {
    fn drop(&mut self) {
        let _ = self.install_pending();
    }
}

pub(crate) struct RetainedReconciliationDescriptor {
    pub(crate) domains: Vec<crate::command::MaterializedDomainDescriptor>,
    pub(crate) receipt: CommitReceipt,
    pub(crate) successor: Option<crate::successor::SuccessorDescriptor>,
}

/// Exact durable-state classification for one executed command.
#[derive(Debug)]
#[must_use = "command outcomes carry exact commit evidence, receipts, or reconciliation state"]
pub enum CommandOutcome {
    /// No physical mutation committed; the error is definitive evidence.
    NotCommitted {
        /// Exact command failure.
        evidence: CommandError,
    },
    /// The complete batch is durable. A later local or health-confirmation failure is retained.
    Committed {
        /// Exact committed revision facts.
        receipt: CommitReceipt,
        /// Failure after commit, if one occurred.
        later_failure: Option<CommandError>,
    },
    /// The store could not classify whether the complete batch became durable.
    Indeterminate {
        /// Exact failure that made the outcome indeterminate.
        failure: CommandError,
        /// Opaque retained operation facts.
        reconciliation: ReconciliationCustody,
    },
}

/// Domain-callback stage that surfaced a storage-owned access failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContributorCallbackStage {
    /// Reserve bounded reconciliation quota before writer admission.
    Reservation,
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
        handle: &DomainHandle<D>,
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
        handle: &DomainHandle<D>,
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
    /// A domain rejected pre-admission reconciliation reservation.
    #[error("domain `{domain}` rejected reconciliation reservation: {source}")]
    ContributorReservation {
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
    /// Every retained reconciliation operation slot is occupied.
    #[error("no reconciliation operation capacity remains")]
    ReconciliationCapacity,
    /// The conservative descriptor budget exceeds the fixed per-operation limit.
    #[error(
        "reconciliation descriptor requests {requested} bytes, exceeding the {limit}-byte limit"
    )]
    ReconciliationDescriptorTooLarge {
        /// Conservative requested descriptor bytes.
        requested: usize,
        /// Fixed per-descriptor byte ceiling.
        limit: usize,
    },
    /// Admitted pending mutations did not match their pre-admission reconciliation quota.
    #[error(
        "domain `{domain}` reconciliation reservation mismatches family `{family}`: reserved {reserved}, actual {actual}"
    )]
    ReconciliationReservationMismatch {
        /// Stable domain name.
        domain: &'static str,
        /// Codec family whose quota did not match the contribution.
        family: &'static str,
        /// Pre-admission declared record count.
        reserved: usize,
        /// Admitted pending record count.
        actual: usize,
    },
    #[error("command successor roles do not declare exactly one matching typed source protocol")]
    InvalidSuccessorProtocol,
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
    /// A committed batch publication and its required later persistence step both failed.
    #[error(
        "committed batch failure `{commit}` was followed by persistence failure `{persistence}`"
    )]
    PersistenceAfterCommitFailure {
        /// Exact committed batch failure retained first.
        commit: Box<CommandError>,
        /// Exact later persistence or local failure.
        persistence: Box<CommandError>,
    },
}

impl CommandError {
    /// Returns the exact Beryl-owned storage error class when this command retained one.
    #[must_use]
    pub fn storage_class(&self) -> Option<StorageErrorClass> {
        self.classified_fjall()
            .map(|source| storage_error_class(source.class()))
    }

    /// Returns the exact Beryl-owned commit state when this command retained one.
    #[must_use]
    pub fn storage_commit_state(&self) -> Option<StorageCommitState> {
        self.classified_fjall()
            .and_then(|source| source.commit_state().map(storage_commit_state))
    }

    /// Returns deterministic conflict facts when this is a stale command.
    #[must_use]
    pub fn conflicts(&self) -> Option<&[RevisionConflict]> {
        match self {
            Self::Conflict { conflicts, .. } => Some(conflicts),
            _ => None,
        }
    }

    fn classified_fjall(&self) -> Option<&crate::health::ClassifiedFjallError> {
        match self {
            Self::Commit { source } | Self::Persistence { source } => {
                source.downcast_ref::<crate::health::ClassifiedFjallError>()
            }
            Self::PersistenceAfterCommitFailure { persistence, .. } => {
                persistence.classified_fjall()
            }
            Self::RevisionRead { source } => read_fjall(source),
            Self::ContributorAccess { source, .. } => match source {
                DomainCallbackSource::Read(source) => read_fjall(source),
                DomainCallbackSource::Sidecar(_) => None,
            },
            _ => None,
        }
    }
}

fn read_fjall(source: &ReadError) -> Option<&crate::health::ClassifiedFjallError> {
    match source {
        ReadError::Storage { source, .. } => {
            source.downcast_ref::<crate::health::ClassifiedFjallError>()
        }
        _ => None,
    }
}

fn storage_resource(resource: fjall::StorageResource) -> StorageResource {
    match resource {
        fjall::StorageResource::EncodedRangeEndpointBytes => {
            StorageResource::EncodedRangeEndpointBytes
        }
        fjall::StorageResource::ApplicationKeyspaces => StorageResource::ApplicationKeyspaces,
        fjall::StorageResource::KeyspaceNameBytes => StorageResource::KeyspaceNameBytes,
        fjall::StorageResource::JournalFiles => StorageResource::JournalFiles,
        fjall::StorageResource::AtomicBatchRecords => StorageResource::AtomicBatchRecords,
        fjall::StorageResource::AtomicBatchEncodedKeyBytes => {
            StorageResource::AtomicBatchEncodedKeyBytes
        }
        fjall::StorageResource::AtomicBatchEncodedValueBytes => {
            StorageResource::AtomicBatchEncodedValueBytes
        }
        fjall::StorageResource::EncodedJournalRecordBytes => {
            StorageResource::EncodedJournalRecordBytes
        }
        fjall::StorageResource::DecodedJournalRecordBytes => {
            StorageResource::DecodedJournalRecordBytes
        }
        fjall::StorageResource::EncodedBlockBytes => StorageResource::EncodedBlockBytes,
        fjall::StorageResource::DecodedBlockBytes => StorageResource::DecodedBlockBytes,
        fjall::StorageResource::EncodedSeparatedValueBytes => {
            StorageResource::EncodedSeparatedValueBytes
        }
        fjall::StorageResource::DecodedSeparatedValueBytes => {
            StorageResource::DecodedSeparatedValueBytes
        }
        fjall::StorageResource::MergeSources => StorageResource::MergeSources,
        fjall::StorageResource::TableRecords => StorageResource::TableRecords,
        fjall::StorageResource::BlobFileRecords => StorageResource::BlobFileRecords,
        fjall::StorageResource::FragmentationRecords => StorageResource::FragmentationRecords,
        fjall::StorageResource::VersionHistorySlots => StorageResource::VersionHistorySlots,
        fjall::StorageResource::SharedBlockCacheBytes => StorageResource::SharedBlockCacheBytes,
        fjall::StorageResource::MemtablePayloadBytes => StorageResource::MemtablePayloadBytes,
        fjall::StorageResource::MemtableRecords => StorageResource::MemtableRecords,
        fjall::StorageResource::Other => StorageResource::Other,
        _ => StorageResource::Other,
    }
}

fn storage_error_class(class: fjall::ErrorClass) -> StorageErrorClass {
    match class {
        fjall::ErrorClass::Configuration => StorageErrorClass::Configuration,
        fjall::ErrorClass::PolicyDenied {
            resource,
            requested,
            limit,
        } => StorageErrorClass::PolicyDenied {
            resource: storage_resource(resource),
            requested,
            limit,
        },
        fjall::ErrorClass::Corruption => StorageErrorClass::Corruption,
        fjall::ErrorClass::Io(kind) => StorageErrorClass::Io(kind),
        fjall::ErrorClass::Integrity => StorageErrorClass::Integrity,
        fjall::ErrorClass::Poisoned => StorageErrorClass::Poisoned,
        fjall::ErrorClass::MaintenanceTerminal => StorageErrorClass::MaintenanceTerminal,
        fjall::ErrorClass::KeyspaceIdentity => StorageErrorClass::KeyspaceIdentity,
        fjall::ErrorClass::Durability => StorageErrorClass::Durability,
        fjall::ErrorClass::Other => StorageErrorClass::Other,
        _ => StorageErrorClass::Other,
    }
}

fn storage_commit_state(state: fjall::CommitState) -> StorageCommitState {
    match state {
        fjall::CommitState::NotCommitted => StorageCommitState::NotCommitted,
        fjall::CommitState::Committed => StorageCommitState::Committed,
        fjall::CommitState::Indeterminate => StorageCommitState::Indeterminate,
        _ => StorageCommitState::Indeterminate,
    }
}
