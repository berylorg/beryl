use std::{
    any::TypeId,
    collections::HashSet,
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use beryl_model::{DomainRevision, HomeRevision};
use thiserror::Error;

use crate::{
    AdmittedSidecar, DomainCallbackError, DomainHandle, DomainReader, ReadError, RecordCodec,
    StorageDomain,
    domain::{RegisteredDomain, StoreInstanceId},
    read::{encode_stored_key, encode_value},
};

mod participant;
mod result;

pub(crate) use participant::{DomainMutationPlan, DomainParticipant};
use participant::{mutation_plan, validation_plan};
pub use result::{
    CommandError, CommitReceipt, CommitReceiptError, ContributorCallbackStage, RevisionConflict,
};

/// Cooperative cancellation observed only before serialized writer admission.
#[derive(Clone, Default)]
pub struct CommandCancellation {
    cancelled: Arc<AtomicBool>,
}

impl CommandCancellation {
    /// Creates a live cancellation signal.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation if the command has not yet been admitted.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// One domain-owned revision-checked mutation plan.
pub trait DomainMutation<D: StorageDomain>: Send + 'static {
    /// Domain-owned validation or contribution failure.
    type Error: DomainCallbackError;

    /// Validates current authoritative state before any batch is assembled.
    ///
    /// This method must be deterministic, bounded, and free of external I/O or
    /// side effects. Same-store writer reentry is rejected.
    fn validate(&self, reader: &DomainReader<'_, D>) -> Result<(), Self::Error>;

    /// Adds typed puts and deletes after every participant validates.
    ///
    /// Contribution must perform only bounded typed reads and builder calls;
    /// CAS, filesystem, windowing, and other external work belongs outside the
    /// short admitted command.
    fn contribute(
        &self,
        reader: &DomainReader<'_, D>,
        mutations: &mut MutationBuilder<'_, D>,
    ) -> Result<(), Self::Error>;
}

/// One domain-owned validation-only guard for a heterogeneous home command.
///
/// A validator can inspect only its typed domain through the same serialized
/// writer snapshot and expected-revision fence as mutation participants. It
/// cannot assemble records, retain sidecars, or advance revisions, and it is
/// accepted only alongside at least one real mutation participant.
pub trait DomainValidator<D: StorageDomain>: Send + 'static {
    /// Domain-owned validation failure.
    type Error: DomainCallbackError;

    /// Validates current authoritative state before any mutation batch is assembled.
    ///
    /// This method must be deterministic, bounded, and free of external I/O or
    /// side effects. Same-store writer reentry is rejected.
    fn validate(&self, reader: &DomainReader<'_, D>) -> Result<(), Self::Error>;
}

/// Failure while translating a typed domain plan into pending mutations.
#[derive(Debug, Error)]
pub enum MutationBuildError {
    /// The codec names no family in this registered domain.
    #[error("record codec names unknown family `{family}` in domain `{domain}`")]
    UnknownFamily {
        /// Stable domain name.
        domain: &'static str,
        /// Unknown logical family.
        family: &'static str,
    },
    /// The family is registered to a different exact Rust codec type.
    #[error("record codec does not own family `{family}` in domain `{domain}`")]
    CodecTypeMismatch {
        /// Stable domain name.
        domain: &'static str,
        /// Logical family with a different registered codec owner.
        family: &'static str,
    },
    /// The same physical key is changed twice by one domain contribution.
    #[error("domain `{domain}` contributes more than one mutation for the same record key")]
    DuplicateRecord {
        /// Stable domain name.
        domain: &'static str,
    },
    /// A typed codec rejected its key or value.
    #[error(transparent)]
    Codec(#[from] ReadError),
}

pub(crate) enum PendingAction {
    Put(Vec<u8>),
    Delete,
}

pub(crate) struct PendingMutation {
    pub(crate) family_slot: usize,
    pub(crate) key: Vec<u8>,
    pub(crate) action: PendingAction,
}

/// Package-owned typed mutation collector with no raw batch access.
pub struct MutationBuilder<'a, D: StorageDomain> {
    domain: &'a RegisteredDomain,
    pending: Vec<PendingMutation>,
    touched: HashSet<(usize, Vec<u8>)>,
    _typed: PhantomData<fn(D) -> D>,
}

impl<'a, D: StorageDomain> MutationBuilder<'a, D> {
    pub(crate) fn new(domain: &'a RegisteredDomain) -> Self {
        Self {
            domain,
            pending: Vec::new(),
            touched: HashSet::new(),
            _typed: PhantomData,
        }
    }

    /// Adds one typed insert or replacement.
    pub fn put<R: RecordCodec<D>>(
        &mut self,
        key: &R::Key,
        value: &R::Value,
    ) -> Result<(), MutationBuildError> {
        let family_slot = self.family_slot::<R>(R::FAMILY)?;
        let key = encode_stored_key::<D, R>(key)?;
        let value = encode_value::<D, R>(value)?;
        self.push(family_slot, key, PendingAction::Put(value))
    }

    /// Adds one typed deletion.
    pub fn delete<R: RecordCodec<D>>(&mut self, key: &R::Key) -> Result<(), MutationBuildError> {
        let family_slot = self.family_slot::<R>(R::FAMILY)?;
        let key = encode_stored_key::<D, R>(key)?;
        self.push(family_slot, key, PendingAction::Delete)
    }

    fn family_slot<R: RecordCodec<D>>(
        &self,
        name: &'static str,
    ) -> Result<usize, MutationBuildError> {
        let slot = self
            .domain
            .family_slot(name)
            .ok_or(MutationBuildError::UnknownFamily {
                domain: D::NAME,
                family: name,
            })?;
        if self.domain.families[slot].codec_type != TypeId::of::<R>() {
            return Err(MutationBuildError::CodecTypeMismatch {
                domain: D::NAME,
                family: name,
            });
        }
        Ok(slot)
    }

    fn push(
        &mut self,
        family_slot: usize,
        key: Vec<u8>,
        action: PendingAction,
    ) -> Result<(), MutationBuildError> {
        if !self.touched.insert((family_slot, key.clone())) {
            return Err(MutationBuildError::DuplicateRecord { domain: D::NAME });
        }
        self.pending.push(PendingMutation {
            family_slot,
            key,
            action,
        });
        Ok(())
    }

    pub(crate) fn into_pending(self) -> Vec<PendingMutation> {
        self.pending
    }
}

/// Opaque, typed domain contribution accepted by a home command.
pub struct MutationContribution {
    pub(crate) plan: DomainMutationPlan,
    pub(crate) expected_revision: DomainRevision,
}

/// Opaque, typed validation-only domain participant accepted by a home command.
pub struct ValidationContribution {
    pub(crate) plan: participant::DomainValidationPlan,
    pub(crate) expected_revision: DomainRevision,
}

/// One typed single-domain mutation whose physical revisions are captured only after writer
/// admission.
///
/// The domain mutation must still carry and validate every logical record revision that authorizes
/// its effect. This boundary prevents unrelated home or same-domain commits from making a prepared
/// single-domain mutation stale before it reaches the serialized writer; it is not a blind-write
/// or retry capability.
pub struct CurrentDomainCommand {
    pub(crate) plan: DomainMutationPlan,
    pub(crate) cancellation: CommandCancellation,
    #[cfg(feature = "test-faults")]
    pub(crate) fault_scope: crate::fault::FaultScope,
}

impl std::fmt::Debug for CurrentDomainCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CurrentDomainCommand")
            .field("domain", &self.plan.domain)
            .finish_non_exhaustive()
    }
}

impl CurrentDomainCommand {
    /// Associates a cooperative pre-admission cancellation signal.
    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CommandCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }
}

impl std::fmt::Debug for MutationContribution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MutationContribution")
            .field("domain", &self.plan.domain)
            .field("expected_revision", &self.expected_revision)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ValidationContribution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidationContribution")
            .field("domain", &self.plan.domain)
            .field("expected_revision", &self.expected_revision)
            .finish_non_exhaustive()
    }
}

impl<D: StorageDomain> DomainHandle<D> {
    /// Seals one domain-owned plan with the revision it expects to mutate.
    pub fn contribution<M: DomainMutation<D>>(
        self,
        expected_revision: DomainRevision,
        mutation: M,
    ) -> MutationContribution {
        MutationContribution {
            plan: mutation_plan::<D, M>(self.store, self.slot, self.owner, mutation),
            expected_revision,
        }
    }

    /// Seals one validation-only participant with the domain revision it expects to guard.
    pub fn validation<V: DomainValidator<D>>(
        self,
        expected_revision: DomainRevision,
        validator: V,
    ) -> ValidationContribution {
        ValidationContribution {
            plan: validation_plan::<D, V>(self.store, self.slot, self.owner, validator),
            expected_revision,
        }
    }

    /// Seals one domain-owned plan whose physical revisions will be captured under writer
    /// admission.
    pub fn current_command<M: DomainMutation<D>>(self, mutation: M) -> CurrentDomainCommand {
        CurrentDomainCommand {
            plan: mutation_plan::<D, M>(self.store, self.slot, self.owner, mutation),
            cancellation: CommandCancellation::new(),
            #[cfg(feature = "test-faults")]
            fault_scope: crate::fault::FaultScope::of::<M>(),
        }
    }
}

/// Failure while constructing a heterogeneous home command.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CommandBuildError {
    /// One command may participate in a logical domain at most once across both roles.
    #[error("command contains duplicate participation for domain `{domain}`")]
    DuplicateDomain {
        /// Stable duplicate domain name.
        domain: &'static str,
    },
    /// One command retained the same durable sidecar more than once.
    #[error("command contains duplicate sidecar admission")]
    DuplicateSidecar,
}

/// One revision-checked command spanning one or more logical domains.
pub struct HomeCommand {
    pub(crate) expected_home_revision: HomeRevision,
    pub(crate) cancellation: CommandCancellation,
    pub(crate) participants: Vec<DomainParticipant>,
    pub(crate) sidecars: Vec<AdmittedSidecar>,
}

impl HomeCommand {
    /// Constructs an empty command against one exact home revision.
    #[must_use]
    pub fn new(expected_home_revision: HomeRevision) -> Self {
        Self {
            expected_home_revision,
            cancellation: CommandCancellation::new(),
            participants: Vec::new(),
            sidecars: Vec::new(),
        }
    }

    /// Associates a cooperative pre-admission cancellation signal.
    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CommandCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Adds one sealed typed domain contribution.
    pub fn add(
        &mut self,
        contribution: MutationContribution,
    ) -> Result<&mut Self, CommandBuildError> {
        if self.contains_domain(contribution.plan.store, contribution.plan.slot) {
            return Err(CommandBuildError::DuplicateDomain {
                domain: contribution.plan.domain,
            });
        }
        self.participants
            .push(DomainParticipant::Mutation(contribution));
        Ok(self)
    }

    /// Adds one sealed typed validation-only domain participant.
    pub fn add_validation(
        &mut self,
        validation: ValidationContribution,
    ) -> Result<&mut Self, CommandBuildError> {
        if self.contains_domain(validation.plan.store, validation.plan.slot) {
            return Err(CommandBuildError::DuplicateDomain {
                domain: validation.plan.domain,
            });
        }
        self.participants
            .push(DomainParticipant::Validation(validation));
        Ok(self)
    }

    fn contains_domain(&self, store: StoreInstanceId, slot: usize) -> bool {
        self.participants
            .iter()
            .any(|participant| participant.store() == store && participant.slot() == slot)
    }

    /// Retains one fully published sidecar through this command's metadata commit.
    pub fn require_sidecar(
        &mut self,
        sidecar: AdmittedSidecar,
    ) -> Result<&mut Self, CommandBuildError> {
        if self
            .sidecars
            .iter()
            .any(|existing| existing.address() == sidecar.address())
        {
            return Err(CommandBuildError::DuplicateSidecar);
        }
        self.sidecars.push(sidecar);
        Ok(self)
    }
}
