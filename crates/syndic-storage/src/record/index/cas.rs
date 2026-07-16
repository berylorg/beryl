use beryl_model::{
    BindingRevision, CasItemId, CasNativeTurnCount, CasThreadId, CasTurnId, ProjectionRevision,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CasItemIndexRecord {
    pub(crate) cas_thread_id: CasThreadId,
    pub(crate) cas_turn_id: CasTurnId,
    pub(crate) cas_item_id: CasItemId,
    pub(crate) item_id: SyndicItemId,
    pub(crate) item_revision: ProjectionRevision,
}
impl CasItemIndexRecord {
    #[must_use]
    pub const fn new(
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
        cas_item_id: CasItemId,
        item_id: SyndicItemId,
        item_revision: ProjectionRevision,
    ) -> Self {
        Self {
            cas_thread_id,
            cas_turn_id,
            cas_item_id,
            item_id,
            item_revision,
        }
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
    #[must_use]
    pub const fn cas_turn_id(&self) -> &CasTurnId {
        &self.cas_turn_id
    }
    #[must_use]
    pub const fn cas_item_id(&self) -> &CasItemId {
        &self.cas_item_id
    }
    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn item_revision(&self) -> ProjectionRevision {
        self.item_revision
    }
}

/// Permanent owner and one-way retirement record for every CAS thread in binding history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CasThreadIndexRecord {
    pub(crate) cas_thread_id: CasThreadId,
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) first_binding_revision: BindingRevision,
    pub(crate) latest_binding_revision: BindingRevision,
    pub(crate) retired_binding_revision: Option<BindingRevision>,
}

/// One immutable reverse membership from a CAS thread to a Syndic binding revision.
///
/// The ordered family containing these records proves every historical reuse between
/// [`CasThreadIndexRecord::first_binding_revision`] and its latest occurrence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CasThreadBindingIndexRecord {
    pub(crate) cas_thread_id: CasThreadId,
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) binding_revision: BindingRevision,
}

impl CasThreadBindingIndexRecord {
    #[must_use]
    pub const fn new(
        cas_thread_id: CasThreadId,
        thread_id: SyndicThreadId,
        binding_revision: BindingRevision,
    ) -> Self {
        Self {
            cas_thread_id,
            thread_id,
            binding_revision,
        }
    }

    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }
}
impl CasThreadIndexRecord {
    #[must_use]
    pub const fn new(
        cas_thread_id: CasThreadId,
        thread_id: SyndicThreadId,
        first_binding_revision: BindingRevision,
    ) -> Self {
        Self {
            cas_thread_id,
            thread_id,
            first_binding_revision,
            latest_binding_revision: first_binding_revision,
            retired_binding_revision: None,
        }
    }

    #[must_use]
    pub const fn with_latest(
        cas_thread_id: CasThreadId,
        thread_id: SyndicThreadId,
        first_binding_revision: BindingRevision,
        latest_binding_revision: BindingRevision,
    ) -> Self {
        Self {
            cas_thread_id,
            thread_id,
            first_binding_revision,
            latest_binding_revision,
            retired_binding_revision: None,
        }
    }
    #[must_use]
    pub const fn retired(
        cas_thread_id: CasThreadId,
        thread_id: SyndicThreadId,
        first_binding_revision: BindingRevision,
        retired_binding_revision: BindingRevision,
    ) -> Self {
        Self::retired_with_latest(
            cas_thread_id,
            thread_id,
            first_binding_revision,
            retired_binding_revision,
            retired_binding_revision,
        )
    }

    #[must_use]
    pub const fn retired_with_latest(
        cas_thread_id: CasThreadId,
        thread_id: SyndicThreadId,
        first_binding_revision: BindingRevision,
        latest_binding_revision: BindingRevision,
        retired_binding_revision: BindingRevision,
    ) -> Self {
        Self {
            cas_thread_id,
            thread_id,
            first_binding_revision,
            latest_binding_revision,
            retired_binding_revision: Some(retired_binding_revision),
        }
    }

    #[must_use]
    pub fn advance(&self, latest_binding_revision: BindingRevision) -> Self {
        Self::with_latest(
            self.cas_thread_id.clone(),
            self.thread_id,
            self.first_binding_revision,
            latest_binding_revision,
        )
    }
    #[must_use]
    pub fn retire(&self, retired_binding_revision: BindingRevision) -> Self {
        Self::retired(
            self.cas_thread_id.clone(),
            self.thread_id,
            self.first_binding_revision,
            retired_binding_revision,
        )
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn first_binding_revision(&self) -> BindingRevision {
        self.first_binding_revision
    }
    #[must_use]
    pub const fn latest_binding_revision(&self) -> BindingRevision {
        self.latest_binding_revision
    }
    #[must_use]
    pub const fn retired_binding_revision(&self) -> Option<BindingRevision> {
        self.retired_binding_revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CasTurnIndexRecord {
    pub(crate) cas_thread_id: CasThreadId,
    pub(crate) cas_turn_id: CasTurnId,
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) turn_id: SyndicTurnId,
    pub(crate) binding_revision: BindingRevision,
    pub(crate) snapshot_id: SyndicExecutionSnapshotId,
    pub(crate) post_turn_native_count: CasNativeTurnCount,
}
impl CasTurnIndexRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        binding_revision: BindingRevision,
        snapshot_id: SyndicExecutionSnapshotId,
        post_turn_native_count: CasNativeTurnCount,
    ) -> Self {
        Self {
            cas_thread_id,
            cas_turn_id,
            thread_id,
            turn_id,
            binding_revision,
            snapshot_id,
            post_turn_native_count,
        }
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
    #[must_use]
    pub const fn cas_turn_id(&self) -> &CasTurnId {
        &self.cas_turn_id
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }
    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }
    /// Returns the exact native count after this correlated CAS turn completes.
    #[must_use]
    pub const fn post_turn_native_count(&self) -> CasNativeTurnCount {
        self.post_turn_native_count
    }
}
