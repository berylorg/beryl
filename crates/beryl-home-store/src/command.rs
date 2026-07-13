use std::{
    collections::HashSet,
    error::Error,
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use beryl_model::{DomainRevision, HomeRevision};
use thiserror::Error;

use crate::{
    AdmittedSidecar, DomainHandle, DomainReader, ReadError, RecordCodec, StorageDomain,
    domain::{RegisteredDomain, StoreInstanceId},
    read::{encode_key, encode_value},
};

mod result;

pub use result::{CommandError, CommitReceipt, RevisionConflict};

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
    type Error: Error + Send + Sync + 'static;

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
    _typed: PhantomData<fn() -> D>,
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
        let family_slot = self.family_slot(R::FAMILY)?;
        let key = encode_key::<D, R>(key)?;
        let value = encode_value::<D, R>(value)?;
        self.push(family_slot, key, PendingAction::Put(value))
    }

    /// Adds one typed deletion.
    pub fn delete<R: RecordCodec<D>>(&mut self, key: &R::Key) -> Result<(), MutationBuildError> {
        let family_slot = self.family_slot(R::FAMILY)?;
        let key = encode_key::<D, R>(key)?;
        self.push(family_slot, key, PendingAction::Delete)
    }

    fn family_slot(&self, name: &'static str) -> Result<usize, MutationBuildError> {
        self.domain
            .family_slot(name)
            .ok_or(MutationBuildError::UnknownFamily {
                domain: D::NAME,
                family: name,
            })
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

pub(crate) trait ErasedContribution: Send {
    fn validate(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn assemble(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<Vec<PendingMutation>, Box<dyn Error + Send + Sync>>;
}

struct TypedContribution<D: StorageDomain, M: DomainMutation<D>> {
    mutation: M,
    _typed: PhantomData<fn() -> D>,
}

impl<D: StorageDomain, M: DomainMutation<D>> ErasedContribution for TypedContribution<D, M> {
    fn validate(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.mutation
            .validate(&DomainReader::new(snapshot, domain))
            .map_err(|source| Box::new(source) as Box<dyn Error + Send + Sync>)
    }

    fn assemble(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<Vec<PendingMutation>, Box<dyn Error + Send + Sync>> {
        let reader = DomainReader::new(snapshot, domain);
        let mut builder = MutationBuilder::<D>::new(domain);
        self.mutation
            .contribute(&reader, &mut builder)
            .map_err(|source| Box::new(source) as Box<dyn Error + Send + Sync>)?;
        Ok(builder.into_pending())
    }
}

/// Opaque, typed domain contribution accepted by a home command.
pub struct MutationContribution {
    pub(crate) store: StoreInstanceId,
    pub(crate) slot: usize,
    pub(crate) domain: &'static str,
    pub(crate) expected_revision: DomainRevision,
    pub(crate) mutation: Box<dyn ErasedContribution>,
}

impl std::fmt::Debug for MutationContribution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MutationContribution")
            .field("domain", &self.domain)
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
            store: self.store,
            slot: self.slot,
            domain: D::NAME,
            expected_revision,
            mutation: Box::new(TypedContribution::<D, M> {
                mutation,
                _typed: PhantomData,
            }),
        }
    }
}

/// Failure while constructing a heterogeneous home command.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CommandBuildError {
    /// One command may advance a logical domain at most once.
    #[error("command contains duplicate contribution for domain `{domain}`")]
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
    pub(crate) contributions: Vec<MutationContribution>,
    pub(crate) sidecars: Vec<AdmittedSidecar>,
}

impl HomeCommand {
    /// Constructs an empty command against one exact home revision.
    #[must_use]
    pub fn new(expected_home_revision: HomeRevision) -> Self {
        Self {
            expected_home_revision,
            cancellation: CommandCancellation::new(),
            contributions: Vec::new(),
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
        if self.contributions.iter().any(|existing| {
            existing.store == contribution.store && existing.slot == contribution.slot
        }) {
            return Err(CommandBuildError::DuplicateDomain {
                domain: contribution.domain,
            });
        }
        self.contributions.push(contribution);
        Ok(self)
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
