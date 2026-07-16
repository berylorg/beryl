mod admission;
mod binding;
mod capture;
mod capture_text;
mod content_text;
mod current;
mod pages;
mod range;

pub use capture::SyndicCaptureItem;
pub use content_text::SyndicContentTextRangeRead;
pub use current::{SyndicCurrentDraft, SyndicThreadTail};
pub use range::SyndicResourceRangeRead;

use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore, ReadLimitError};
use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, DiscussionContextOwnerId, SyndicAcceptedInputId,
    SyndicContentId, SyndicDraftId, SyndicExecutionSnapshotId, SyndicItemId, SyndicProjectionId,
    SyndicResourceId, SyndicThreadId, SyndicTurnId,
};

use crate::{AcceptedInputRecord, SyndicReadError};
use crate::{
    ActiveCasTurnRecord, BindingHeadRecord, BindingRecord, CanonicalItemRecord,
    CasThreadIndexRecord, CasTurnIndexRecord, ContentManifestRecord, ContextEnvelopeRecord,
    DraftRecord, ExecutionSnapshotRecord, HistorySummaryRecord, InputGateRecord,
    ItemProjectionBuildRecord, ItemProjectionHeadRecord, ItemProjectionSetRecord, ProjectionRecord,
    ResourceMetadataRecord, SourceEventRecord, TranscriptBuildRecord, TranscriptViewHeadRecord,
    TurnRecord, TurnStateRecord, codec::*, domain::SyndicStorage,
};

/// Nonzero total stored-byte bound for one typed Syndic primary read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyndicPointReadLimit {
    max_stored_bytes: usize,
}
impl SyndicPointReadLimit {
    pub fn new(max_stored_bytes: usize) -> Result<Self, ReadLimitError> {
        beryl_home_store::PointReadLimit::new(max_stored_bytes)?;
        Ok(Self { max_stored_bytes })
    }
    #[must_use]
    pub const fn max_stored_bytes(self) -> usize {
        self.max_stored_bytes
    }
}

/// One bounded point result with exact key-and-value stored-byte accounting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicStoredRecord<T> {
    record: T,
    stored_bytes: usize,
}

/// One head-stabilized current binding, never an arbitrary historical revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicCurrentBinding {
    head: BindingHeadRecord,
    binding: BindingRecord,
    stored_bytes: usize,
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

    /// Combined bytes read for the head/binding/head stability proof.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
}
impl<T> SyndicStoredRecord<T> {
    #[must_use]
    pub const fn record(&self) -> &T {
        &self.record
    }
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
}

/// One bounded ordered page returned without a raw storage iterator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicPage<T> {
    records: Vec<T>,
    stored_bytes: usize,
    has_more: bool,
}
impl<T> SyndicPage<T> {
    #[must_use]
    pub fn records(&self) -> &[T] {
        &self.records
    }
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }
}

impl SyndicStorage {
    pub fn thread(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<crate::ThreadRecord>>, SyndicReadError> {
        self.point::<ThreadsFamily>(store, id, limit)
    }
    pub fn draft(
        &self,
        store: &HomeStore,
        id: SyndicDraftId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<DraftRecord>>, SyndicReadError> {
        self.point::<DraftsFamily>(store, id, limit)
    }
    pub fn content_manifest(
        &self,
        store: &HomeStore,
        id: SyndicContentId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<ContentManifestRecord>>, SyndicReadError> {
        self.point::<ContentManifestsFamily>(store, id, limit)
    }
    pub fn context_envelope(
        &self,
        store: &HomeStore,
        owner: DiscussionContextOwnerId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<ContextEnvelopeRecord>>, SyndicReadError> {
        self.point::<ContextEnvelopesFamily>(store, ContextOwnerKey::from(owner), limit)
    }
    pub fn turn(
        &self,
        store: &HomeStore,
        id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<TurnRecord>>, SyndicReadError> {
        self.point::<TurnsFamily>(store, id, limit)
    }
    pub fn turn_state(
        &self,
        store: &HomeStore,
        id: SyndicTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<TurnStateRecord>>, SyndicReadError> {
        self.point::<TurnStatesFamily>(store, id, limit)
    }
    pub fn input_gate(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<InputGateRecord>>, SyndicReadError> {
        self.point::<InputGatesFamily>(store, id, limit)
    }
    pub fn accepted_input(
        &self,
        store: &HomeStore,
        id: SyndicAcceptedInputId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<AcceptedInputRecord>>, SyndicReadError> {
        self.point::<AcceptedInputsFamily>(store, id, limit)
    }
    pub fn canonical_item(
        &self,
        store: &HomeStore,
        id: SyndicItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<CanonicalItemRecord>>, SyndicReadError> {
        self.point::<CanonicalItemsFamily>(store, id, limit)
    }
    pub fn transcript_view_head(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<TranscriptViewHeadRecord>>, SyndicReadError> {
        self.point::<TranscriptHeadsFamily>(store, id, limit)
    }
    pub fn item_projection_head(
        &self,
        store: &HomeStore,
        id: SyndicItemId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<ItemProjectionHeadRecord>>, SyndicReadError> {
        self.point::<ItemProjectionHeadsFamily>(store, id, limit)
    }
    pub fn item_projection_set(
        &self,
        store: &HomeStore,
        item: SyndicItemId,
        generation: crate::ItemProjectionGeneration,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<ItemProjectionSetRecord>>, SyndicReadError> {
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
    ) -> Result<Option<SyndicStoredRecord<ItemProjectionBuildRecord>>, SyndicReadError> {
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
    ) -> Result<Option<SyndicStoredRecord<TranscriptBuildRecord>>, SyndicReadError> {
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
    ) -> Result<Option<SyndicStoredRecord<ProjectionRecord>>, SyndicReadError> {
        self.point::<ProjectionsFamily>(store, id, limit)
    }
    pub fn resource(
        &self,
        store: &HomeStore,
        id: SyndicResourceId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<ResourceMetadataRecord>>, SyndicReadError> {
        self.point::<ResourcesFamily>(store, id, limit)
    }
    pub fn history_summary(
        &self,
        store: &HomeStore,
        id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<HistorySummaryRecord>>, SyndicReadError> {
        self.point::<HistorySummariesFamily>(store, id, limit)
    }
    pub fn binding(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        revision: BindingRevision,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<BindingRecord>>, SyndicReadError> {
        self.point::<BindingsFamily>(store, BindingKey { thread, revision }, limit)
    }

    /// Reads the selected binding through a bounded head/binding/head stability proof.
    ///
    /// `limit` applies independently to each of the three point reads. The returned byte count is
    /// their checked sum and is therefore bounded by three times the per-read ceiling.
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
        let head = first.record().clone();
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
        if second.record() != &head {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "current-binding read",
            });
        }
        let record = binding.record();
        if record.thread_id() != head.thread_id()
            || record.revision() != head.revision()
            || record.state().lifecycle() != head.lifecycle()
            || record.selected_path().digest() != head.selected_path_digest()
        {
            return Err(SyndicReadError::Invariant(
                "current binding head and selected record disagree",
            ));
        }
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_current_binding_read(
            first.stored_bytes(),
            binding.stored_bytes(),
            second.stored_bytes(),
        );
        let stored_bytes = first
            .stored_bytes()
            .checked_add(binding.stored_bytes())
            .and_then(|bytes| bytes.checked_add(second.stored_bytes()))
            .ok_or(SyndicReadError::Invariant(
                "current binding stored-byte accounting overflowed",
            ))?;
        Ok(Some(SyndicCurrentBinding {
            head,
            binding: binding.record,
            stored_bytes,
        }))
    }
    pub fn execution_snapshot(
        &self,
        store: &HomeStore,
        id: SyndicExecutionSnapshotId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<ExecutionSnapshotRecord>>, SyndicReadError> {
        self.point::<ExecutionSnapshotsFamily>(store, id, limit)
    }

    /// Reads the one-way CAS-turn publication for an immutable execution snapshot.
    pub fn active_cas_turn(
        &self,
        store: &HomeStore,
        snapshot: SyndicExecutionSnapshotId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<ActiveCasTurnRecord>>, SyndicReadError> {
        self.point::<ActiveCasTurnsFamily>(store, snapshot, limit)
    }

    /// Reads the permanent Syndic owner and one-way retirement state of one CAS thread.
    pub fn cas_thread_owner(
        &self,
        store: &HomeStore,
        cas_thread: CasThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<CasThreadIndexRecord>>, SyndicReadError> {
        self.point::<CasThreadIndexFamily>(store, CasThreadKey::Record(cas_thread), limit)
    }

    /// Reads the permanent owner correlation for one published CAS turn.
    pub fn cas_turn_owner(
        &self,
        store: &HomeStore,
        cas_thread: CasThreadId,
        cas_turn: CasTurnId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<CasTurnIndexRecord>>, SyndicReadError> {
        self.point::<CasTurnIndexFamily>(store, CasTurnKey::Record(cas_thread, cas_turn), limit)
    }

    pub fn source_event(
        &self,
        store: &HomeStore,
        turn: SyndicTurnId,
        sequence: crate::SourceEventSequence,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicStoredRecord<SourceEventRecord>>, SyndicReadError> {
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
                if stored.record() == &expected
                    && state.as_ref().is_some_and(|state| {
                        state.record().source_event_count() >= request.sequence().get()
                    }) =>
            {
                crate::LiveSourceEventStatus::Exact
            }
            Some(_) => crate::LiveSourceEventStatus::Collision,
            None if state.as_ref().is_some_and(|state| {
                state.record().revision() == request.expected_state_revision()
                    && state.record().source_event_count().checked_add(1)
                        == Some(request.sequence().get())
            }) && gate.as_ref().is_some_and(|gate| {
                gate.record().revision() == request.expected_gate_revision()
            }) =>
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
    ) -> Result<Option<SyndicStoredRecord<F::Value>>, SyndicReadError>
    where
        F::Key: Eq,
    {
        let page = store.read_cursor::<crate::domain::SyndicDomain, ExactCodec<F>>(
            self.handle,
            &CursorRange::closed(key.clone(), key),
            CursorDirection::Forward,
            CursorReadLimits::new(1, limit.max_stored_bytes()).expect("point bound is nonzero"),
        )?;
        if page.has_more() || page.records().len() > 1 {
            return Err(SyndicReadError::Invariant(
                "Syndic point range returned more than one record",
            ));
        }
        let stored_bytes = page.stored_bytes();
        Ok(page
            .into_records()
            .into_iter()
            .next()
            .map(|record| SyndicStoredRecord {
                record: record.into_parts().1,
                stored_bytes,
            }))
    }
}
