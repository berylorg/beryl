use beryl_home_store::{
    CommandOutcome, CurrentDomainCommand, CursorReadLimits, HomeCandidateRecoveryAccess, HomeStore,
};
use beryl_model::{
    BerylHomeId, SyndicContentId, SyndicItemId, SyndicResourceId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    CanonicalItemRecord, ContentManifestRecord, InputGateRecord, ItemProjectionBuildRecord,
    ItemProjectionGeneration, ItemProjectionHeadRecord, ItemProjectionSetRecord,
    ResourceMetadataRecord, SyndicPage, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    ThreadRecord, TranscriptBuildRecord, TranscriptGeneration, TranscriptViewHeadRecord,
    TurnItemIndexRecord, TurnItemOrdinal, TurnRecord, TurnStateRecord,
};

#[derive(Clone, Copy)]
pub(super) enum HistoryAccess<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

macro_rules! point_read {
    ($name:ident, $candidate:ident, ($($key:ident: $key_type:ty),+), $value:ty) => {
        pub(super) fn $name(self, storage: &SyndicStorage, $($key: $key_type,)+ limit: SyndicPointReadLimit) -> Result<Option<$value>, SyndicReadError> {
            match self {
                Self::Ordinary(store) => storage.$name(store, $($key,)+ limit),
                Self::Candidate(store) => storage.$candidate(store, $($key,)+ limit),
            }
        }
    };
}

impl HistoryAccess<'_> {
    pub(super) fn home_id(self) -> BerylHomeId {
        match self {
            Self::Ordinary(store) => store.home_id(),
            Self::Candidate(store) => store.home_id(),
        }
    }
    pub(super) fn execute_current(self, command: CurrentDomainCommand) -> CommandOutcome {
        match self {
            Self::Ordinary(store) => store.execute_current(command),
            Self::Candidate(store) => store.execute_current(command),
        }
    }
    pub(super) fn turn_items(
        self,
        storage: &SyndicStorage,
        turn: SyndicTurnId,
        after: Option<TurnItemOrdinal>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<TurnItemIndexRecord>, SyndicReadError> {
        match self {
            Self::Ordinary(store) => storage.turn_items(store, turn, after, limits),
            Self::Candidate(store) => storage.turn_items_candidate(store, turn, after, limits),
        }
    }
    point_read!(thread, thread_candidate, (id: SyndicThreadId), ThreadRecord);
    point_read!(turn, turn_candidate, (id: SyndicTurnId), TurnRecord);
    point_read!(turn_state, turn_state_candidate, (id: SyndicTurnId), TurnStateRecord);
    point_read!(input_gate, input_gate_candidate, (id: SyndicThreadId), InputGateRecord);
    point_read!(canonical_item, canonical_item_candidate, (id: SyndicItemId), CanonicalItemRecord);
    point_read!(content_manifest, content_manifest_candidate, (id: SyndicContentId), ContentManifestRecord);
    point_read!(resource, resource_candidate, (id: SyndicResourceId), ResourceMetadataRecord);
    point_read!(item_projection_head, item_projection_head_candidate, (id: SyndicItemId), ItemProjectionHeadRecord);
    point_read!(transcript_view_head, transcript_view_head_candidate, (id: SyndicThreadId), TranscriptViewHeadRecord);
    point_read!(item_projection_set, item_projection_set_candidate, (item: SyndicItemId, generation: ItemProjectionGeneration), ItemProjectionSetRecord);
    point_read!(item_projection_build, item_projection_build_candidate, (item: SyndicItemId, generation: ItemProjectionGeneration), ItemProjectionBuildRecord);
    point_read!(transcript_build, transcript_build_candidate, (thread: SyndicThreadId, generation: TranscriptGeneration), TranscriptBuildRecord);
}
