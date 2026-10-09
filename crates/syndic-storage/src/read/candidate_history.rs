use super::access::ReadAccess;
use crate::codec::{
    CanonicalItemsFamily, HistorySummariesFamily, InputGatesFamily, ItemProjectionBuildsFamily,
    ItemProjectionHeadsFamily, ItemProjectionSetKey, ItemProjectionSetsFamily, ProjectionsFamily,
    ResourcesFamily, ThreadTranscriptBuildKey, ThreadsFamily, TranscriptBuildsFamily,
    TranscriptHeadsFamily, TurnStatesFamily, TurnsFamily,
};
use crate::{
    CanonicalItemRecord, ContentManifestRecord, HistorySummaryRecord, InputGateRecord,
    ItemProjectionBuildRecord, ItemProjectionGeneration, ItemProjectionHeadRecord,
    ItemProjectionSetRecord, ProjectionRecord, ResourceMetadataRecord, SyndicPointReadLimit,
    SyndicReadError, SyndicStorage, ThreadRecord, TranscriptBuildRecord, TranscriptGeneration,
    TranscriptViewHeadRecord, TurnRecord, TurnStateRecord,
};
use beryl_home_store::HomeCandidateRecoveryAccess;
use beryl_model::{
    SyndicContentId, SyndicItemId, SyndicProjectionId, SyndicResourceId, SyndicThreadId,
    SyndicTurnId,
};

impl SyndicStorage {
    pub fn projection_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicProjectionId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ProjectionRecord>, SyndicReadError> {
        self.point_with_access::<ProjectionsFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn history_summary_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<HistorySummaryRecord>, SyndicReadError> {
        self.point_with_access::<HistorySummariesFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn thread_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ThreadRecord>, SyndicReadError> {
        self.point_with_access::<ThreadsFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn turn_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TurnRecord>, SyndicReadError> {
        self.point_with_access::<TurnsFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn turn_state_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TurnStateRecord>, SyndicReadError> {
        self.point_with_access::<TurnStatesFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn input_gate_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<InputGateRecord>, SyndicReadError> {
        self.point_with_access::<InputGatesFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn canonical_item_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CanonicalItemRecord>, SyndicReadError> {
        self.point_with_access::<CanonicalItemsFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn content_manifest_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicContentId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ContentManifestRecord>, SyndicReadError> {
        self.content_manifest_with_access(ReadAccess::Candidate(store), id, limit)
    }

    pub fn resource_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicResourceId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ResourceMetadataRecord>, SyndicReadError> {
        self.point_with_access::<ResourcesFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn item_projection_head_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ItemProjectionHeadRecord>, SyndicReadError> {
        self.point_with_access::<ItemProjectionHeadsFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn transcript_view_head_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TranscriptViewHeadRecord>, SyndicReadError> {
        self.point_with_access::<TranscriptHeadsFamily>(ReadAccess::Candidate(store), id, limit)
    }

    pub fn item_projection_set_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        item: SyndicItemId,
        generation: ItemProjectionGeneration,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ItemProjectionSetRecord>, SyndicReadError> {
        self.point_with_access::<ItemProjectionSetsFamily>(
            ReadAccess::Candidate(store),
            ItemProjectionSetKey { item, generation },
            limit,
        )
    }

    pub fn item_projection_build_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        item: SyndicItemId,
        generation: ItemProjectionGeneration,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ItemProjectionBuildRecord>, SyndicReadError> {
        self.point_with_access::<ItemProjectionBuildsFamily>(
            ReadAccess::Candidate(store),
            ItemProjectionSetKey { item, generation },
            limit,
        )
    }

    pub fn transcript_build_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        thread: SyndicThreadId,
        generation: TranscriptGeneration,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TranscriptBuildRecord>, SyndicReadError> {
        self.point_with_access::<TranscriptBuildsFamily>(
            ReadAccess::Candidate(store),
            ThreadTranscriptBuildKey { thread, generation },
            limit,
        )
    }
}
