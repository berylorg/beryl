mod facts;

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{
    BerylHomeId, BindingRevision, DiscussionContextOwnerId, DomainRevision, InputGateRevision,
    ProjectionRevision, SealedAssetReferenceSetProof, SyndicItemId, SyndicThreadId, SyndicTurnId,
};

use crate::{
    ContentReference, SelectedPathProof, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    SyndicTimestamp, TurnDispatchProvenance, TurnKind, TurnStateRevision,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingDispatchEvidence {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    source_revision: DomainRevision,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    turn_kind: TurnKind,
    selected_path: SelectedPathProof,
    context_owner_id: Option<DiscussionContextOwnerId>,
    binding_revision: BindingRevision,
    gate_revision: InputGateRevision,
    state_revision: TurnStateRevision,
    dispatch_provenance: TurnDispatchProvenance,
    item_id: SyndicItemId,
    item_revision: ProjectionRevision,
    input: ContentReference,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
    minimum_timestamp: SyndicTimestamp,
}

impl PendingDispatchEvidence {
    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }
    pub const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }
    pub const fn source_revision(self) -> DomainRevision {
        self.source_revision
    }
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn turn_id(self) -> SyndicTurnId {
        self.turn_id
    }
    pub const fn turn_kind(self) -> TurnKind {
        self.turn_kind
    }
    pub const fn selected_path(self) -> SelectedPathProof {
        self.selected_path
    }
    pub const fn context_owner_id(self) -> Option<DiscussionContextOwnerId> {
        self.context_owner_id
    }
    pub const fn binding_revision(self) -> BindingRevision {
        self.binding_revision
    }
    pub const fn gate_revision(self) -> InputGateRevision {
        self.gate_revision
    }
    pub const fn state_revision(self) -> TurnStateRevision {
        self.state_revision
    }
    pub const fn dispatch_provenance(self) -> TurnDispatchProvenance {
        self.dispatch_provenance
    }
    pub const fn item_id(self) -> SyndicItemId {
        self.item_id
    }
    pub const fn item_revision(self) -> ProjectionRevision {
        self.item_revision
    }
    pub const fn input(self) -> ContentReference {
        self.input
    }
    pub const fn asset_reference_set(self) -> Option<SealedAssetReferenceSetProof> {
        self.asset_reference_set
    }
    pub const fn minimum_timestamp(self) -> SyndicTimestamp {
        self.minimum_timestamp
    }
}

impl SyndicStorage {
    pub fn pending_dispatch_evidence(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<PendingDispatchEvidence>, SyndicReadError> {
        self.read_pending_dispatch_evidence(store, thread_id, limit, || {})
    }

    pub(crate) fn read_pending_dispatch_evidence(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
        before_confirmation: impl FnOnce(),
    ) -> Result<Option<PendingDispatchEvidence>, SyndicReadError> {
        let revision = self.revision(store)?;
        self.with_current_gate_source(store, thread_id, limit, || {
            let first = facts::read(self, store, thread_id, limit);
            before_confirmation();
            let second = facts::read(self, store, thread_id, limit);
            if self.revision(store)? != revision {
                return Err(SyndicReadError::ConcurrentChange {
                    operation: "pending dispatch evidence",
                });
            }
            let first = first?;
            if first != second? {
                return Err(SyndicReadError::ConcurrentChange {
                    operation: "pending dispatch evidence",
                });
            }
            first
                .map(|facts| facts.prove(store.home_id(), self.home_generation, revision))
                .transpose()
        })
    }
}
