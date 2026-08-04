mod binding;
mod cas;

pub use binding::BindingHeadRecord;
pub use cas::*;

use beryl_model::{
    BindingRevision, DiscussionContextOwnerId, DraftRevision, ProjectionRevision,
    SyndicAcceptedInputId, SyndicDraftId, SyndicItemId, SyndicPathDigest, SyndicProjectionId,
    SyndicResourceId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};

use crate::{
    AcceptedInputOrdinal, AcceptedRouteGeneration, BindingLifecycle, ItemProjectionGeneration,
    ItemSourceEventOrdinal, ProjectionOrdinal, ResourceOrdinal, SourceEventSequence,
    TranscriptGeneration, TranscriptPosition, TurnDepth, TurnItemOrdinal,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftByThreadRecord {
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) draft_id: SyndicDraftId,
    pub(crate) draft_revision: DraftRevision,
    pub(crate) thread_revision: ThreadRevision,
}
impl DraftByThreadRecord {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        draft_id: SyndicDraftId,
        draft_revision: DraftRevision,
        thread_revision: ThreadRevision,
    ) -> Self {
        Self {
            thread_id,
            draft_id,
            draft_revision,
            thread_revision,
        }
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }
    #[must_use]
    pub const fn draft_revision(&self) -> DraftRevision {
        self.draft_revision
    }
    #[must_use]
    pub const fn thread_revision(&self) -> ThreadRevision {
        self.thread_revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadParentIndexRecord {
    pub(crate) parent_thread_id: SyndicThreadId,
    pub(crate) child_thread_id: SyndicThreadId,
    pub(crate) child_revision: ThreadRevision,
    pub(crate) context_owner_id: DiscussionContextOwnerId,
}
impl ThreadParentIndexRecord {
    #[must_use]
    pub const fn new(
        parent_thread_id: SyndicThreadId,
        child_thread_id: SyndicThreadId,
        child_revision: ThreadRevision,
        context_owner_id: DiscussionContextOwnerId,
    ) -> Self {
        Self {
            parent_thread_id,
            child_thread_id,
            child_revision,
            context_owner_id,
        }
    }
    #[must_use]
    pub const fn parent_thread_id(&self) -> SyndicThreadId {
        self.parent_thread_id
    }
    #[must_use]
    pub const fn child_thread_id(&self) -> SyndicThreadId {
        self.child_thread_id
    }
    #[must_use]
    pub const fn child_revision(&self) -> ThreadRevision {
        self.child_revision
    }
    #[must_use]
    pub const fn context_owner_id(&self) -> DiscussionContextOwnerId {
        self.context_owner_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnChildIndexRecord {
    pub(crate) parent_id: SyndicTurnId,
    pub(crate) child_id: SyndicTurnId,
    pub(crate) child_depth: TurnDepth,
    pub(crate) child_digest: SyndicPathDigest,
}
impl TurnChildIndexRecord {
    #[must_use]
    pub const fn new(
        parent_id: SyndicTurnId,
        child_id: SyndicTurnId,
        child_depth: TurnDepth,
        child_digest: SyndicPathDigest,
    ) -> Self {
        Self {
            parent_id,
            child_id,
            child_depth,
            child_digest,
        }
    }
    #[must_use]
    pub const fn parent_id(&self) -> SyndicTurnId {
        self.parent_id
    }
    #[must_use]
    pub const fn child_id(&self) -> SyndicTurnId {
        self.child_id
    }
    #[must_use]
    pub const fn child_depth(&self) -> TurnDepth {
        self.child_depth
    }
    #[must_use]
    pub const fn child_digest(&self) -> SyndicPathDigest {
        self.child_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedOrderIndexRecord {
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) ordinal: AcceptedInputOrdinal,
    pub(crate) input_id: SyndicAcceptedInputId,
    pub(crate) route_generation: AcceptedRouteGeneration,
}
impl AcceptedOrderIndexRecord {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        ordinal: AcceptedInputOrdinal,
        input_id: SyndicAcceptedInputId,
        route_generation: AcceptedRouteGeneration,
    ) -> Self {
        Self {
            thread_id,
            ordinal,
            input_id,
            route_generation,
        }
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn ordinal(&self) -> AcceptedInputOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn input_id(&self) -> SyndicAcceptedInputId {
        self.input_id
    }
    #[must_use]
    pub const fn route_generation(&self) -> AcceptedRouteGeneration {
        self.route_generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnItemIndexRecord {
    pub(crate) turn_id: SyndicTurnId,
    pub(crate) ordinal: TurnItemOrdinal,
    pub(crate) item_id: SyndicItemId,
    pub(crate) item_revision: ProjectionRevision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemSourceEventIndexRecord {
    pub(crate) item_id: SyndicItemId,
    pub(crate) ordinal: ItemSourceEventOrdinal,
    pub(crate) turn_id: SyndicTurnId,
    pub(crate) source_event: SourceEventSequence,
}

impl ItemSourceEventIndexRecord {
    #[must_use]
    pub const fn new(
        item_id: SyndicItemId,
        ordinal: ItemSourceEventOrdinal,
        turn_id: SyndicTurnId,
        source_event: SourceEventSequence,
    ) -> Self {
        Self {
            item_id,
            ordinal,
            turn_id,
            source_event,
        }
    }

    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn ordinal(&self) -> ItemSourceEventOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn source_event(&self) -> SourceEventSequence {
        self.source_event
    }
}
impl TurnItemIndexRecord {
    #[must_use]
    pub const fn new(
        turn_id: SyndicTurnId,
        ordinal: TurnItemOrdinal,
        item_id: SyndicItemId,
        item_revision: ProjectionRevision,
    ) -> Self {
        Self {
            turn_id,
            ordinal,
            item_id,
            item_revision,
        }
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn ordinal(&self) -> TurnItemOrdinal {
        self.ordinal
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptViewEntryRecord {
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) generation: TranscriptGeneration,
    pub(crate) position: TranscriptPosition,
    pub(crate) item_id: SyndicItemId,
    pub(crate) item_revision: ProjectionRevision,
    pub(crate) item_projection_generation: ItemProjectionGeneration,
    pub(crate) projection_id: SyndicProjectionId,
    pub(crate) projection_revision: ProjectionRevision,
}
impl TranscriptViewEntryRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        generation: TranscriptGeneration,
        position: TranscriptPosition,
        item_id: SyndicItemId,
        item_revision: ProjectionRevision,
        item_projection_generation: ItemProjectionGeneration,
        projection_id: SyndicProjectionId,
        projection_revision: ProjectionRevision,
    ) -> Self {
        Self {
            thread_id,
            generation,
            position,
            item_id,
            item_revision,
            item_projection_generation,
            projection_id,
            projection_revision,
        }
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn generation(&self) -> TranscriptGeneration {
        self.generation
    }
    #[must_use]
    pub const fn position(&self) -> TranscriptPosition {
        self.position
    }
    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn item_revision(&self) -> ProjectionRevision {
        self.item_revision
    }
    #[must_use]
    pub const fn item_projection_generation(&self) -> ItemProjectionGeneration {
        self.item_projection_generation
    }
    #[must_use]
    pub const fn projection_id(&self) -> SyndicProjectionId {
        self.projection_id
    }
    #[must_use]
    pub const fn projection_revision(&self) -> ProjectionRevision {
        self.projection_revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemProjectionIndexRecord {
    pub(crate) item_id: SyndicItemId,
    pub(crate) generation: ItemProjectionGeneration,
    pub(crate) ordinal: ProjectionOrdinal,
    pub(crate) projection_id: SyndicProjectionId,
    pub(crate) projection_revision: ProjectionRevision,
}

/// One generation-independent immutable projection in an item's closed prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableItemProjectionIndexRecord {
    pub(crate) item_id: SyndicItemId,
    pub(crate) ordinal: ProjectionOrdinal,
    pub(crate) projection_id: SyndicProjectionId,
    pub(crate) projection_revision: ProjectionRevision,
}

impl StableItemProjectionIndexRecord {
    #[must_use]
    pub const fn new(
        item_id: SyndicItemId,
        ordinal: ProjectionOrdinal,
        projection_id: SyndicProjectionId,
        projection_revision: ProjectionRevision,
    ) -> Self {
        Self {
            item_id,
            ordinal,
            projection_id,
            projection_revision,
        }
    }

    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }

    #[must_use]
    pub const fn ordinal(&self) -> ProjectionOrdinal {
        self.ordinal
    }

    #[must_use]
    pub const fn projection_id(&self) -> SyndicProjectionId {
        self.projection_id
    }

    #[must_use]
    pub const fn projection_revision(&self) -> ProjectionRevision {
        self.projection_revision
    }
}

impl ItemProjectionIndexRecord {
    #[must_use]
    pub const fn new(
        item_id: SyndicItemId,
        generation: ItemProjectionGeneration,
        ordinal: ProjectionOrdinal,
        projection_id: SyndicProjectionId,
        projection_revision: ProjectionRevision,
    ) -> Self {
        Self {
            item_id,
            generation,
            ordinal,
            projection_id,
            projection_revision,
        }
    }
    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn generation(&self) -> ItemProjectionGeneration {
        self.generation
    }
    #[must_use]
    pub const fn ordinal(&self) -> ProjectionOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn projection_id(&self) -> SyndicProjectionId {
        self.projection_id
    }
    #[must_use]
    pub const fn projection_revision(&self) -> ProjectionRevision {
        self.projection_revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionResourceIndexRecord {
    pub(crate) projection_id: SyndicProjectionId,
    pub(crate) ordinal: ResourceOrdinal,
    pub(crate) resource_id: SyndicResourceId,
    pub(crate) resource_revision: ProjectionRevision,
    pub(crate) resource_digest: [u8; 32],
}
impl ProjectionResourceIndexRecord {
    #[must_use]
    pub const fn new(
        projection_id: SyndicProjectionId,
        ordinal: ResourceOrdinal,
        resource_id: SyndicResourceId,
        resource_revision: ProjectionRevision,
        resource_digest: [u8; 32],
    ) -> Self {
        Self {
            projection_id,
            ordinal,
            resource_id,
            resource_revision,
            resource_digest,
        }
    }
    #[must_use]
    pub const fn projection_id(&self) -> SyndicProjectionId {
        self.projection_id
    }
    #[must_use]
    pub const fn ordinal(&self) -> ResourceOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn resource_id(&self) -> SyndicResourceId {
        self.resource_id
    }
    #[must_use]
    pub const fn resource_revision(&self) -> ProjectionRevision {
        self.resource_revision
    }
    #[must_use]
    pub const fn resource_digest(&self) -> [u8; 32] {
        self.resource_digest
    }
}
