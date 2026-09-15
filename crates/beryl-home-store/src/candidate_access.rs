use beryl_model::{BerylHomeId, DomainRevision, HomeRevision};

use crate::{
    CommandOutcome, CommitReceipt, CommitReceiptError, CurrentDomainCommand, CursorDirection,
    CursorPage, CursorRange, CursorReadLimits, DomainHandle, HomeCandidateError, HomeCommand,
    HomeGeneration, HomeHealthState, HomeStore, PointReadLimit, ReadError, ReconciliationFailure,
    ReconciliationHandle, ReconciliationResolution, RecordCodec, StorageDomain,
    health::{HealthAdmission, HealthGate, HealthGateError},
};

#[derive(Clone, Copy)]
pub(crate) enum StoreOperationAccess {
    Ordinary,
    Candidate {
        state: HomeHealthState,
        generation: HomeGeneration,
    },
}

impl StoreOperationAccess {
    pub(crate) fn admit(self, gate: &HealthGate) -> Result<HealthAdmission<'_>, HealthGateError> {
        match self {
            Self::Ordinary => gate.admit(),
            Self::Candidate { state, generation } => gate.admit_candidate(state, generation),
        }
    }
}

pub struct HomeCandidateRecoveryAccess<'a> {
    store: &'a HomeStore,
    access: StoreOperationAccess,
    generation: HomeGeneration,
}

impl<'a> HomeCandidateRecoveryAccess<'a> {
    pub(crate) fn new(
        store: &'a HomeStore,
        state: HomeHealthState,
        generation: HomeGeneration,
    ) -> Result<Self, HomeCandidateError> {
        let access = StoreOperationAccess::Candidate { state, generation };
        access.admit(&store.health)?.confirm()?;
        Ok(Self {
            store,
            access,
            generation,
        })
    }

    pub fn home_id(&self) -> BerylHomeId {
        self.store.home_id()
    }

    pub fn generation(&self) -> HomeGeneration {
        self.generation
    }

    pub fn home_revision(&self) -> Result<HomeRevision, ReadError> {
        self.store.home_revision_with_access(self.access)
    }

    pub fn domain_revision<D: StorageDomain>(
        &self,
        handle: &DomainHandle<D>,
    ) -> Result<DomainRevision, ReadError> {
        self.store.domain_revision_with_access(self.access, handle)
    }

    pub fn read_point<D: StorageDomain, R: RecordCodec<D>>(
        &self,
        handle: &DomainHandle<D>,
        key: &R::Key,
        limit: PointReadLimit,
    ) -> Result<Option<R::Value>, ReadError> {
        self.store
            .read_point_with_access::<D, R>(self.access, handle, key, limit)
    }

    pub fn read_cursor<D: StorageDomain, R: RecordCodec<D>>(
        &self,
        handle: &DomainHandle<D>,
        range: &CursorRange<R::Key>,
        direction: CursorDirection,
        limits: CursorReadLimits,
    ) -> Result<CursorPage<R::Key, R::Value>, ReadError> {
        self.store
            .read_cursor_with_access::<D, R>(self.access, handle, range, direction, limits)
    }

    pub fn execute(&self, command: HomeCommand) -> CommandOutcome {
        self.store.execute_with_access(self.access, command)
    }

    pub fn execute_current(&self, command: CurrentDomainCommand) -> CommandOutcome {
        self.store.execute_current_with_access(self.access, command)
    }

    pub fn receipt_domain_revision<D: StorageDomain>(
        &self,
        receipt: &CommitReceipt,
        handle: &DomainHandle<D>,
    ) -> Result<Option<DomainRevision>, CommitReceiptError> {
        self.store
            .receipt_domain_revision_with_access(self.access, receipt, handle)
    }

    pub fn pending_reconciliations(&self) -> Vec<ReconciliationHandle> {
        self.store.pending_reconciliations()
    }

    pub fn reconcile(
        &self,
        handle: &ReconciliationHandle,
    ) -> Result<ReconciliationResolution, ReconciliationFailure> {
        self.store.reconcile_with_access(self.access, handle)
    }

    pub fn retry_reconciliation(
        &self,
        handle: &ReconciliationHandle,
    ) -> Result<ReconciliationResolution, ReconciliationFailure> {
        self.store
            .retry_reconciliation_with_access(self.access, handle)
    }
}
