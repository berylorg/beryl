use beryl_model::{BindingRevision, SyndicExecutionSnapshotId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnDispatchAnchor {
    snapshot_id: SyndicExecutionSnapshotId,
    binding_revision: BindingRevision,
}

impl TurnDispatchAnchor {
    pub const fn new(
        snapshot_id: SyndicExecutionSnapshotId,
        binding_revision: BindingRevision,
    ) -> Self {
        Self {
            snapshot_id,
            binding_revision,
        }
    }

    pub const fn snapshot_id(self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }

    pub const fn binding_revision(self) -> BindingRevision {
        self.binding_revision
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnDispatchProvenance {
    Unattempted,
    Activated(TurnDispatchAnchor),
    Cancelled(TurnDispatchAnchor),
    ProviderOperation,
}
