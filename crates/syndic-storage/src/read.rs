mod accepted_delivery;
mod accepted_next;
mod accepted_ready;
mod admission;
mod binding;
mod capture;
mod capture_text;
mod catalog_summary;
mod compaction;
mod content_text;
mod current;
mod delivering_steering;
mod delivery_recovery;
mod pages;
mod promotion;
mod queries;
mod range;
mod routes;
mod stop;
mod stop_admission;

pub use accepted_delivery::{AcceptedInputDeliveryTransitionStatus, SyndicReadySteeringInput};
pub(crate) use accepted_next::AcceptedNextCandidateBasis;
pub use accepted_next::{
    ACCEPTED_NEXT_PAGE_MAX_BYTES, ACCEPTED_NEXT_PAGE_MAX_RECORDS, AcceptedNextCandidate,
    AcceptedNextCandidateCursor, AcceptedNextCandidatePage, AcceptedNextSource,
    AcceptedNextSourceCursor, AcceptedNextSourcePage,
};
pub use accepted_ready::{
    ACCEPTED_READY_PAGE_MAX_BYTES, ACCEPTED_READY_PAGE_MAX_RECORDS, AcceptedReadyCandidate,
    AcceptedReadyCandidateCursor, AcceptedReadyCandidatePage, AcceptedReadySourceCursor,
    AcceptedReadySourcePage,
};
pub use capture::SyndicCaptureItem;
pub use capture_text::SyndicCaptureTextRangeRead;
pub use catalog_summary::{
    ExactThreadCatalogSummary, PreparedThreadCatalogSummaryReplacement,
    ThreadCatalogSummaryPreparation,
};
pub use compaction::{
    CompactionAdmissionCandidate, CompactionAdmissionIneligibility, CompactionAdmissionRead,
    CompactionRecoveryCase, CompactionRequestTransitionStatus,
};
pub use content_text::{
    SyndicContentTextRangeRead, SyndicContentTextSegment, SyndicContentTextSegmentBoundary,
    SyndicContentTextSegmentRangeRead,
};
pub use current::{SyndicCurrentDraft, SyndicThreadTail};
pub use delivering_steering::SyndicDeliveringSteeringInput;
pub use delivery_recovery::{
    ActiveDeliveryRecovery, DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
    DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS, DeliveryRecoveryCase,
    DeliveryRecoveryClassificationError, DeliveryRecoverySource, DeliveryRecoveryStartupCursor,
    DeliveryRecoveryStartupPage, RecoveredPendingCursor, RecoveredPendingPage,
    RecoveredPendingSource, SyndicLiveStopOperation,
};
pub use queries::*;
pub use range::SyndicResourceRangeRead;
pub use routes::*;
pub use stop::StopOperationTransitionStatus;
pub use stop_admission::{StopAdmissionCandidate, StopAdmissionIneligibility, StopAdmissionRead};

use beryl_home_store::{HomeStore, PointReadLimit, ReadLimitError};
use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, DiscussionContextOwnerId, SyndicAcceptedInputId,
    SyndicContentId, SyndicDraftId, SyndicExecutionSnapshotId, SyndicItemId, SyndicProjectionId,
    SyndicResourceId, SyndicThreadId, SyndicTurnId,
};

use crate::{AcceptedInputRecord, ProjectionTextSource, SyndicReadError};
use crate::{
    ActiveCasTurnRecord, BindingHeadRecord, BindingRecord, CanonicalItemRecord,
    CasThreadIndexRecord, CasTurnIndexRecord, ContentManifestRecord, ContextEnvelopeRecord,
    DraftRecord, ExecutionSnapshotRecord, HistorySummaryRecord, InputGateRecord,
    ItemProjectionBuildRecord, ItemProjectionHeadRecord, ItemProjectionSetRecord, ProjectionRecord,
    ResourceMetadataRecord, SourceEventRecord, StopOperationId, StopOperationRecord,
    ThreadAttributesRecord, ThreadCatalogSummaryRecord, ThreadExecutionRecord, ThreadUsageRecord,
    TranscriptBuildRecord, TranscriptViewHeadRecord, TurnRecord, TurnStateRecord, codec::*,
    domain::SyndicStorage,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ReadByteTotals {
    pub(crate) stored: usize,
    pub(crate) decoded: usize,
}

impl ReadByteTotals {
    pub(crate) const fn new(stored: usize, decoded: usize) -> Self {
        Self { stored, decoded }
    }

    pub(crate) fn add(
        &mut self,
        stored: usize,
        decoded: usize,
        overflow: &'static str,
    ) -> Result<(), SyndicReadError> {
        self.stored = self
            .stored
            .checked_add(stored)
            .ok_or(SyndicReadError::Invariant(overflow))?;
        self.decoded = self
            .decoded
            .checked_add(decoded)
            .ok_or(SyndicReadError::Invariant(overflow))?;
        Ok(())
    }
}

pub(crate) fn read_projection_text_source_range_into(
    storage: &SyndicStorage,
    store: &HomeStore,
    source: ProjectionTextSource,
    start: u64,
    end: u64,
    output: &mut [u8],
) -> Result<ReadByteTotals, SyndicReadError> {
    range::read_projection_text_source_range_into(storage, store, source, start, end, output)
}

/// Nonzero stored-value and practical decoded-value ceiling for one typed Syndic point read.
///
/// The encoded key is independently schema-bounded and is not charged to this caller limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyndicPointReadLimit {
    max_bytes: usize,
}
impl SyndicPointReadLimit {
    /// Constructs one ceiling applied independently before value acquisition and publication.
    pub fn new(max_bytes: usize) -> Result<Self, ReadLimitError> {
        PointReadLimit::new(max_bytes)?;
        Ok(Self { max_bytes })
    }

    /// Returns the maximum stored value and practical decoded value.
    #[must_use]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }
}

/// One head-stabilized current binding, never an arbitrary historical revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicCurrentBinding {
    head: BindingHeadRecord,
    binding: BindingRecord,
}

impl SyndicCurrentBinding {
    #[must_use]
    pub const fn head(&self) -> &BindingHeadRecord {
        &self.head
    }

    #[must_use]
    pub const fn binding(&self) -> &BindingRecord {
        &self.binding
    }
}

/// One bounded ordered page returned without a raw storage iterator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicPage<T> {
    records: Vec<T>,
    stored_bytes: usize,
    decoded_bytes: usize,
    has_more: bool,
}
impl<T> SyndicPage<T> {
    /// Returns the typed records published by this page.
    #[must_use]
    pub fn records(&self) -> &[T] {
        &self.records
    }

    /// Returns the actual aggregate stored bytes acquired by this cursor page.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    /// Returns the actual aggregate practical decoded bytes published by this cursor page.
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }

    #[must_use]
    pub fn into_records(self) -> Vec<T> {
        self.records
    }
}

impl SyndicStorage {
    pub fn thread(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<crate::ThreadRecord>, SyndicReadError> {
        self.point::<ThreadsFamily>(store, id, limit)
    }
    pub fn thread_execution(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ThreadExecutionRecord>, SyndicReadError> {
        self.point::<ThreadExecutionsFamily>(store, id, limit)
    }
    pub fn thread_attributes(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ThreadAttributesRecord>, SyndicReadError> {
        self.point::<ThreadAttributesFamily>(store, id, limit)
    }
    pub fn thread_usage(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ThreadUsageRecord>, SyndicReadError> {
        self.point::<ThreadUsageFamily>(store, id, limit)
    }
    pub fn thread_catalog_summary(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ThreadCatalogSummaryRecord>, SyndicReadError> {
        self.point::<ThreadCatalogSummariesFamily>(store, id, limit)
    }
    pub fn draft(
        &self,
        store: &HomeStore,
        id: SyndicDraftId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<DraftRecord>, SyndicReadError> {
        self.point::<DraftsFamily>(store, id, limit)
    }
    pub fn content_manifest(
        &self,
        store: &HomeStore,
        id: SyndicContentId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ContentManifestRecord>, SyndicReadError> {
        self.point::<ContentManifestsFamily>(store, id, limit)
    }
    pub fn context_envelope(
        &self,
        store: &HomeStore,
        owner: DiscussionContextOwnerId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ContextEnvelopeRecord>, SyndicReadError> {
        self.point::<ContextEnvelopesFamily>(store, ContextOwnerKey::from(owner), limit)
    }
    pub fn turn(
        &self,
        store: &HomeStore,
        id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TurnRecord>, SyndicReadError> {
        self.point::<TurnsFamily>(store, id, limit)
    }
    pub fn turn_state(
        &self,
        store: &HomeStore,
        id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TurnStateRecord>, SyndicReadError> {
        self.point::<TurnStatesFamily>(store, id, limit)
    }
    pub fn input_gate(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<InputGateRecord>, SyndicReadError> {
        self.point::<InputGatesFamily>(store, id, limit)
    }
    pub fn accepted_input(
        &self,
        store: &HomeStore,
        id: SyndicAcceptedInputId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<AcceptedInputRecord>, SyndicReadError> {
        self.point::<AcceptedInputsFamily>(store, id, limit)
    }
    /// Reads one retained live-or-consumed stop-operation receipt by its exact natural identity.
    pub fn stop_operation(
        &self,
        store: &HomeStore,
        id: StopOperationId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<StopOperationRecord>, SyndicReadError> {
        self.point::<StopOperationsFamily>(store, id, limit)
    }
    pub fn canonical_item(
        &self,
        store: &HomeStore,
        id: SyndicItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CanonicalItemRecord>, SyndicReadError> {
        self.point::<CanonicalItemsFamily>(store, id, limit)
    }
    pub fn transcript_view_head(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TranscriptViewHeadRecord>, SyndicReadError> {
        self.point::<TranscriptHeadsFamily>(store, id, limit)
    }
    pub fn item_projection_head(
        &self,
        store: &HomeStore,
        id: SyndicItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ItemProjectionHeadRecord>, SyndicReadError> {
        self.point::<ItemProjectionHeadsFamily>(store, id, limit)
    }
    pub fn item_projection_set(
        &self,
        store: &HomeStore,
        item: SyndicItemId,
        generation: crate::ItemProjectionGeneration,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ItemProjectionSetRecord>, SyndicReadError> {
        self.point::<ItemProjectionSetsFamily>(
            store,
            ItemProjectionSetKey { item, generation },
            limit,
        )
    }
    pub fn item_projection_build(
        &self,
        store: &HomeStore,
        item: SyndicItemId,
        generation: crate::ItemProjectionGeneration,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ItemProjectionBuildRecord>, SyndicReadError> {
        self.point::<ItemProjectionBuildsFamily>(
            store,
            ItemProjectionSetKey { item, generation },
            limit,
        )
    }
    pub fn transcript_build(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        generation: crate::TranscriptGeneration,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<TranscriptBuildRecord>, SyndicReadError> {
        self.point::<TranscriptBuildsFamily>(
            store,
            ThreadTranscriptBuildKey { thread, generation },
            limit,
        )
    }
    pub fn projection(
        &self,
        store: &HomeStore,
        id: SyndicProjectionId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ProjectionRecord>, SyndicReadError> {
        self.point::<ProjectionsFamily>(store, id, limit)
    }
    pub fn resource(
        &self,
        store: &HomeStore,
        id: SyndicResourceId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ResourceMetadataRecord>, SyndicReadError> {
        self.point::<ResourcesFamily>(store, id, limit)
    }
    pub fn history_summary(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<HistorySummaryRecord>, SyndicReadError> {
        self.point::<HistorySummariesFamily>(store, id, limit)
    }
    pub fn binding(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        revision: BindingRevision,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<BindingRecord>, SyndicReadError> {
        self.point::<BindingsFamily>(store, BindingKey { thread, revision }, limit)
    }

    /// Reads the selected binding through a bounded head/binding/head stability proof.
    ///
    /// `limit` applies independently to each of the three point reads.
    pub fn current_binding(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicCurrentBinding>, SyndicReadError> {
        let Some(first) = self.point::<BindingHeadsFamily>(store, thread, limit)? else {
            return match self.point::<BindingHeadsFamily>(store, thread, limit)? {
                None => Ok(None),
                Some(_) => Err(SyndicReadError::ConcurrentChange {
                    operation: "current-binding read",
                }),
            };
        };
        let head = first.clone();
        let binding = self
            .point::<BindingsFamily>(
                store,
                BindingKey {
                    thread,
                    revision: head.revision(),
                },
                limit,
            )?
            .ok_or(SyndicReadError::Invariant(
                "current binding head selects a missing binding",
            ))?;
        let second = self
            .point::<BindingHeadsFamily>(store, thread, limit)?
            .ok_or(SyndicReadError::ConcurrentChange {
                operation: "current-binding read",
            })?;
        if second != head {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "current-binding read",
            });
        }
        if binding.thread_id() != head.thread_id()
            || binding.revision() != head.revision()
            || binding.state().lifecycle() != head.lifecycle()
            || binding.selected_path().digest() != head.selected_path_digest()
        {
            return Err(SyndicReadError::Invariant(
                "current binding head and selected record disagree",
            ));
        }
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_current_binding_read();
        Ok(Some(SyndicCurrentBinding { head, binding }))
    }
    pub fn execution_snapshot(
        &self,
        store: &HomeStore,
        id: SyndicExecutionSnapshotId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ExecutionSnapshotRecord>, SyndicReadError> {
        self.point::<ExecutionSnapshotsFamily>(store, id, limit)
    }

    /// Reads the one-way CAS-turn publication for an immutable execution snapshot.
    pub fn active_cas_turn(
        &self,
        store: &HomeStore,
        snapshot: SyndicExecutionSnapshotId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ActiveCasTurnRecord>, SyndicReadError> {
        self.point::<ActiveCasTurnsFamily>(store, snapshot, limit)
    }

    /// Reads the permanent Syndic owner and one-way retirement state of one CAS thread.
    pub fn cas_thread_owner(
        &self,
        store: &HomeStore,
        cas_thread: CasThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CasThreadIndexRecord>, SyndicReadError> {
        self.point::<CasThreadIndexFamily>(store, CasThreadKey::Record(cas_thread), limit)
    }

    /// Reads the permanent owner correlation for one published CAS turn.
    pub fn cas_turn_owner(
        &self,
        store: &HomeStore,
        cas_thread: CasThreadId,
        cas_turn: CasTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CasTurnIndexRecord>, SyndicReadError> {
        self.point::<CasTurnIndexFamily>(store, CasTurnKey::Record(cas_thread, cas_turn), limit)
    }

    pub fn source_event(
        &self,
        store: &HomeStore,
        turn: SyndicTurnId,
        sequence: crate::SourceEventSequence,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SourceEventRecord>, SyndicReadError> {
        self.point::<SourceEventsFamily>(
            store,
            TurnEventKey {
                owner: turn,
                ordinal: sequence,
            },
            limit,
        )
    }

    /// Reconciles one immutable normalized source event under a stable domain revision.
    pub fn live_source_event_status(
        &self,
        store: &HomeStore,
        request: &crate::LiveSourceEvent,
        limit: SyndicPointReadLimit,
    ) -> Result<crate::LiveSourceEventStatus, SyndicReadError> {
        let stored = self.source_event(store, request.turn_id(), request.sequence(), limit)?;
        let state = self.turn_state(store, request.turn_id(), limit)?;
        let gate = self.input_gate(store, request.thread_id(), limit)?;
        let confirmed_state = self.turn_state(store, request.turn_id(), limit)?;
        let confirmed_gate = self.input_gate(store, request.thread_id(), limit)?;
        if confirmed_state != state || confirmed_gate != gate {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "live-source-event reconciliation",
            });
        }
        let expected = SourceEventRecord::new(
            request.turn_id(),
            request.sequence(),
            request.source().cloned(),
            request.payload().clone(),
        )
        .expect("LiveSourceEvent construction already validates its source record");
        Ok(match stored {
            Some(stored)
                if stored == expected
                    && state.as_ref().is_some_and(|state| {
                        state.source_event_count() >= request.sequence().get()
                    }) =>
            {
                crate::LiveSourceEventStatus::Exact
            }
            Some(_) => crate::LiveSourceEventStatus::Collision,
            None if state.as_ref().is_some_and(|state| {
                state.revision() == request.expected_state_revision()
                    && state.source_event_count().checked_add(1) == Some(request.sequence().get())
            }) && gate
                .as_ref()
                .is_some_and(|gate| gate.revision() == request.expected_gate_revision()) =>
            {
                crate::LiveSourceEventStatus::Absent
            }
            None => crate::LiveSourceEventStatus::Collision,
        })
    }

    pub(crate) fn point<F: Family>(
        &self,
        store: &HomeStore,
        key: F::Key,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<F::Value>, SyndicReadError> {
        store
            .read_point::<crate::domain::SyndicDomain, ExactCodec<F>>(
                self.handle,
                &key,
                PointReadLimit::new(limit.max_bytes()).expect("point bound is nonzero"),
            )
            .map_err(Into::into)
    }
}
