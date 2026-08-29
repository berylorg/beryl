use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, MutationContribution};
use beryl_model::{
    BindingRevision, DomainRevision, DraftRevision, ExecutionBinding, ProjectionRevision,
    SyndicDraftId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};

use crate::{
    BindingHeadRecord, BindingLifecycle, BindingRecord, BindingState, DraftByThreadRecord,
    DraftPieceRootRecordV1, DraftRecord, DraftSubmissionIntent, HistorySummaryRecord,
    ImageLabelAuthorityHeadV1, InputGateRecord, ProjectionLifecycle, SelectedPathProof,
    SyndicStorage, SyndicThreadTail, SyndicTimestamp, ThreadAttributesRecord,
    ThreadCatalogSummaryRecord, ThreadExecutionRecord, ThreadLineageProof, ThreadRecord,
    ThreadUsageRecord, TranscriptGeneration, TranscriptViewHeadRecord,
    canonical_empty_draft_piece_root_v1, codec::*, domain::SyndicDomain, draft_piece::*,
    empty_selected_path_digest, root_thread_lineage_digest,
};

mod accepted;
mod activity;
mod admission;
mod admission_helpers;
mod binding;
mod compaction;
mod content;
mod error;
mod live;
pub(crate) mod projection;
mod promotion;
mod provider_frame;
mod provider_observation;
mod stop;
mod thread_properties;
mod transcript;

pub use accepted::{
    BeginAcceptedInputDelivery, CompleteAcceptedInputDelivery, RetryAcceptedInputDelivery,
    SteeringRejection,
};
pub use activity::PublishActivityChildHandoff;
pub use admission::{FirstAcceptance, FirstAcceptanceKind, FirstAcceptanceStatus};
#[cfg(feature = "test-faults")]
pub(crate) use binding::active_cas_turn_fault_scope;
pub use binding::{
    AbandonActiveBinding, ActivateBinding, ActiveCasTurnPublicationStatus,
    BindingPublicationStatus, CancelBindingActivation, ExactRejectedInputDelivery,
    PublishActiveCasTurn, PublishStaleBinding, PublishUnboundBinding, PublishValidBinding,
};
pub use compaction::{
    AbandonCompactionOperation, AdmitCompactionOperation, ClaimCompactionDispatch,
    CompactionProviderEvent, PublishCompactionProviderEvent, PublishCompactionRequestDisposition,
    SealLifecycleContinuationContent, SettleCompactionOperation, SettleLifecycleCompaction,
};
pub use content::{CONTENT_APPEND_MAX_CHUNKS, ContentAppend, ContentBuild};
pub use error::{CreateThreadError, SyndicMutationError, ThreadCreationStatus};
#[cfg(feature = "test-faults")]
pub(crate) use live::live_source_event_fault_scope;
pub use live::{
    CompleteTerminalHistory, FinalizeNextTurnItem, FreezeNextTurnItem, LiveSourceEvent,
    LiveSourceEventStatus,
};
pub use projection::{AdvanceItemProjectionBuild, StartItemProjectionBuild};
pub use promotion::{AcceptedInputPromotionStatus, PromoteAcceptedInput};
pub use provider_frame::{
    PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS, PreparedProviderFrame,
    ProviderCompletionComparisonMutationError, ProviderFrameMutationError,
    ProviderFramePreparationError, ProviderFramePreparationPlan, ProviderFrameStageBatch,
    ProviderFrameStageBatchError, ProviderFrameStageBatchState, ProviderFrameStageCallback,
    ProviderFrameStageError, ProviderFrameStageOutcome, prepare_provider_frame,
    stage_provider_frame,
};
pub use provider_observation::ProviderObservationMutationError;
#[cfg(feature = "test-faults")]
pub(crate) use provider_observation::provider_observation_stage_fault_scope;
pub use stop::{
    AbandonStopOperation, AdmitStopOperation, ClaimStopDispatch, JoinStopCause,
    SafelyReopenStopOperation,
};
pub use thread_properties::{
    AcceptGeneratedThreadTitle, ArchiveBranchDiscussionThread, PublishThreadUsage,
};
pub use transcript::{AdvanceTranscriptBuild, StartTranscriptBuild};

/// Exact immutable inputs for one ordinary thread and its first durable draft.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateThread {
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    execution: ExecutionBinding,
    created_at: SyndicTimestamp,
    history_policy: DraftEditHistoryPolicyV1,
    source: Option<SyndicThreadTail>,
}

impl CreateThread {
    /// Creates one independent empty ordinary thread.
    #[must_use]
    pub const fn ordinary(
        thread_id: SyndicThreadId,
        draft_id: SyndicDraftId,
        execution: ExecutionBinding,
        created_at: SyndicTimestamp,
        history_policy: DraftEditHistoryPolicyV1,
    ) -> Self {
        Self {
            thread_id,
            draft_id,
            execution,
            created_at,
            history_policy,
            source: None,
        }
    }

    /// Creates one ordinary thread selected at an exact existing committed tail.
    pub fn from_tail(
        thread_id: SyndicThreadId,
        draft_id: SyndicDraftId,
        created_at: SyndicTimestamp,
        history_policy: DraftEditHistoryPolicyV1,
        source: SyndicThreadTail,
    ) -> Result<Self, CreateThreadError> {
        if source.selected_path().tail().is_none() {
            return Err(CreateThreadError::EmptySourceTail);
        }
        if !source.complete() {
            return Err(CreateThreadError::IncompleteSourceTail);
        }
        if created_at < source.last_activity_at() {
            return Err(CreateThreadError::TimestampPrecedesSourceActivity);
        }
        Ok(Self {
            thread_id,
            draft_id,
            execution: source.execution().clone(),
            created_at,
            history_policy,
            source: Some(source),
        })
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
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }

    #[must_use]
    pub const fn created_at(&self) -> SyndicTimestamp {
        self.created_at
    }

    #[must_use]
    pub const fn history_policy(&self) -> DraftEditHistoryPolicyV1 {
        self.history_policy
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InitialThreadRecords {
    pub(crate) thread: ThreadRecord,
    pub(crate) image_label_authority_head: ImageLabelAuthorityHeadV1,
    pub(crate) execution: ThreadExecutionRecord,
    pub(crate) attributes: ThreadAttributesRecord,
    pub(crate) usage: ThreadUsageRecord,
    pub(crate) catalog_summary: ThreadCatalogSummaryRecord,
    pub(crate) draft: DraftRecord,
    pub(crate) draft_piece_root: DraftPieceRootRecordV1,
    pub(crate) draft_edit_history: DraftEditHistoryFrontierV1,
    pub(crate) draft_index: DraftByThreadRecord,
    pub(crate) transcript_head: TranscriptViewHeadRecord,
    pub(crate) transcript_build: Option<crate::TranscriptBuildRecord>,
    pub(crate) summary: HistorySummaryRecord,
    pub(crate) input_gate: InputGateRecord,
    pub(crate) activity_head: crate::ActivityQueryHeadRecord,
    pub(crate) binding: BindingRecord,
    pub(crate) binding_head: BindingHeadRecord,
}

impl CreateThread {
    pub(crate) fn records(&self) -> InitialThreadRecords {
        let thread_revision = ThreadRevision::new(1).expect("initial revision is nonzero");
        let draft_revision = DraftRevision::new(1).expect("initial revision is nonzero");
        let projection_revision = ProjectionRevision::new(1).expect("initial revision is nonzero");
        let binding_revision = BindingRevision::new(1).expect("initial revision is nonzero");
        let selected_path = self.source.as_ref().map_or_else(
            || SelectedPathProof::new(None, thread_revision, empty_selected_path_digest()),
            |source| {
                SelectedPathProof::new(
                    source.selected_path().tail(),
                    thread_revision,
                    source.selected_path().digest(),
                )
            },
        );
        let thread = ThreadRecord::new(
            self.thread_id,
            selected_path,
            self.draft_id,
            ThreadLineageProof::new(
                None,
                None,
                crate::ThreadLineageDepth::FIRST,
                root_thread_lineage_digest(self.thread_id),
            ),
            None,
        );
        let draft_piece_root = canonical_empty_draft_piece_root_v1(
            self.draft_id,
            draft_revision,
            canonical_empty_draft_root_operation_id_v1(self.draft_id),
        );
        let draft_edit_history = canonical_empty_draft_edit_history_v1(
            draft_piece_root.reference(),
            self.history_policy,
        );
        let draft = DraftRecord::new(
            self.draft_id,
            self.thread_id,
            draft_revision,
            DraftSubmissionIntent::Ordinary,
            DraftRootHistoryPairV1::new(
                draft_piece_root.reference(),
                draft_edit_history.reference(),
            ),
            self.created_at,
            self.created_at,
        );
        let stale = self.source.is_some();
        let binding_state = BindingState::unbound("new thread has no CAS projection")
            .expect("static unbound reason is valid");
        let transcript_build = (!stale).then(|| {
            crate::TranscriptBuildRecord::new(
                self.thread_id,
                TranscriptGeneration::FIRST,
                projection_revision,
                thread_revision,
                None,
                selected_path.digest(),
                0,
                0,
                crate::projection::transcript_entry_digest_seed(),
                true,
                crate::TranscriptBuildPhase::Complete,
            )
        });
        let execution = ThreadExecutionRecord::new(self.thread_id, self.execution.clone());
        let attributes = ThreadAttributesRecord::ordinary(self.thread_id);
        let usage = ThreadUsageRecord::empty(self.thread_id);
        let summary = HistorySummaryRecord::new(
            self.thread_id,
            projection_revision,
            thread_revision,
            selected_path.tail(),
            selected_path.digest(),
            !stale,
            self.created_at,
        );
        let initial_title = self
            .source
            .as_ref()
            .and_then(|source| source.entire_selected_path_title().cloned());
        let catalog_summary = ThreadCatalogSummaryRecord::initial_with_history_title(
            &thread,
            &execution,
            &attributes,
            &summary,
            initial_title,
        );
        InitialThreadRecords {
            thread,
            image_label_authority_head: ImageLabelAuthorityHeadV1::new(
                self.thread_id,
                1,
                crate::ImageLabelFrontier::EMPTY,
                crate::ImageLabelFrontier::EMPTY,
            )
            .expect("initial image-label authority head is valid"),
            execution,
            attributes,
            usage,
            catalog_summary,
            draft,
            draft_piece_root,
            draft_edit_history,
            draft_index: DraftByThreadRecord::new(
                self.thread_id,
                self.draft_id,
                draft_revision,
                thread_revision,
            ),
            transcript_head: TranscriptViewHeadRecord::new(
                self.thread_id,
                TranscriptGeneration::FIRST,
                projection_revision,
                0,
                selected_path.tail(),
                selected_path.digest(),
                if stale {
                    ProjectionLifecycle::Stale
                } else {
                    ProjectionLifecycle::Current
                },
            ),
            transcript_build,
            summary,
            input_gate: InputGateRecord::idle(self.thread_id),
            activity_head: crate::ActivityQueryHeadRecord::empty(self.thread_id),
            binding: BindingRecord::new(
                self.thread_id,
                binding_revision,
                selected_path,
                binding_state,
            ),
            binding_head: BindingHeadRecord::new(
                self.thread_id,
                binding_revision,
                BindingLifecycle::Unbound,
                selected_path.digest(),
            ),
        }
    }
}

fn validate_source_tail(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &SyndicThreadTail,
    created_at: SyndicTimestamp,
) -> Result<(), SyndicMutationError> {
    if created_at < source.last_activity_at() {
        return Err(SyndicMutationError::TimestampPrecedesSourceActivity);
    }
    let thread = required::<ThreadsFamily>(reader, &source.thread_id())?;
    let selected = source.selected_path();
    if thread.revision() != selected.thread_revision()
        || thread.committed_tail() != selected.tail()
        || thread.selected_path_digest() != selected.digest()
    {
        return Err(SyndicMutationError::SourceTailConflict);
    }
    let summary = required::<HistorySummariesFamily>(reader, &source.thread_id())?;
    let execution = required::<ThreadExecutionsFamily>(reader, &source.thread_id())?;
    if summary.thread_revision() != selected.thread_revision()
        || summary.committed_tail() != selected.tail()
        || summary.selected_path_digest() != selected.digest()
        || summary.last_activity_at() != source.last_activity_at()
        || !summary.complete()
        || !source.complete()
        || execution.thread_id() != source.thread_id()
        || execution.execution() != source.execution()
    {
        return Err(SyndicMutationError::SourceTailConflict);
    }
    Ok(())
}

pub(super) fn current_draft(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
) -> Result<DraftRecord, SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &thread_id)?;
    let index = required::<DraftByThreadFamily>(reader, &thread_id)?;
    let draft = required::<DraftsFamily>(reader, &thread.current_draft_id())?;
    if index.thread_id() != thread.id()
        || index.draft_id() != draft.id()
        || index.draft_revision() != draft.revision()
        || index.thread_revision() != thread.revision()
        || draft.thread_id() != thread.id()
    {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    Ok(draft)
}

pub(super) fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, SyndicMutationError> {
    reader
        .point::<ExactCodec<F>>(key, crate::codec::family_point_limit::<F>())
        .map_err(Into::into)
}

pub(super) fn required<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<F::Value, SyndicMutationError> {
    point::<F>(reader, key)?.ok_or(SyndicMutationError::RequiredRecordMissing { family: F::NAME })
}

pub(super) fn turn_is_on_selected_path(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &ThreadRecord,
    candidate: &crate::TurnRecord,
) -> Result<bool, SyndicMutationError> {
    let Some(tail_id) = thread.committed_tail() else {
        return Ok(false);
    };
    let tail = required::<TurnsFamily>(reader, &tail_id)?;
    crate::selected_path::includes_turn(
        tail,
        candidate,
        |turn_id| required::<TurnsFamily>(reader, &turn_id),
        |_| SyndicMutationError::SourceTailConflict,
    )
}

impl SyndicStorage {
    /// Seals one atomic thread, draft, input-gate, and projection creation contribution.
    #[must_use]
    pub fn create_thread(
        &self,
        expected_domain_revision: DomainRevision,
        creation: CreateThread,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, CreateThreadMutation { creation })
    }
}

struct CreateThreadMutation {
    creation: CreateThread,
}

impl DomainMutation<SyndicDomain> for CreateThreadMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let records = self.creation.records();
        let transcript_build_collision = match &records.transcript_build {
            Some(build) => point::<TranscriptBuildsFamily>(
                reader,
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
            )?
            .is_some(),
            None => false,
        };
        if point::<ThreadsFamily>(reader, &records.thread.id())?.is_some()
            || point::<ImageLabelAuthorityHeadsFamily>(reader, &records.thread.id())?.is_some()
            || point::<ThreadExecutionsFamily>(reader, &records.thread.id())?.is_some()
            || point::<ThreadAttributesFamily>(reader, &records.thread.id())?.is_some()
            || point::<ThreadUsageFamily>(reader, &records.thread.id())?.is_some()
            || point::<ThreadCatalogSummariesFamily>(reader, &records.thread.id())?.is_some()
            || point::<DraftsFamily>(reader, &records.draft.id())?.is_some()
            || point::<DraftPieceRootsFamily>(reader, &records.draft_piece_root.reference().key())?
                .is_some()
            || point::<DraftEditHistoryFrontiersFamily>(
                reader,
                &records.draft_edit_history.reference().key(),
            )?
            .is_some()
            || point::<DraftByThreadFamily>(reader, &records.thread.id())?.is_some()
            || point::<TranscriptHeadsFamily>(reader, &records.thread.id())?.is_some()
            || transcript_build_collision
            || point::<HistorySummariesFamily>(reader, &records.thread.id())?.is_some()
            || point::<InputGatesFamily>(reader, &records.thread.id())?.is_some()
            || point::<ActivityQueryHeadsFamily>(reader, &records.thread.id())?.is_some()
            || point::<BindingHeadsFamily>(reader, &records.thread.id())?.is_some()
            || point::<BindingsFamily>(
                reader,
                &BindingKey {
                    thread: records.thread.id(),
                    revision: records.binding.revision(),
                },
            )?
            .is_some()
            || point::<TurnsFamily>(
                reader,
                &SyndicTurnId::from_bytes(*records.draft.id().as_bytes()),
            )?
            .is_some()
            || point::<AcceptedInputsFamily>(reader, &records.draft.id().accepted_input_id())?
                .is_some()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if let Some(source) = &self.creation.source {
            validate_source_tail(reader, source, self.creation.created_at)?;
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = self.creation.records();
        reservation.reserve_records::<ThreadsCodec>(1)?;
        reservation.reserve_records::<ImageLabelAuthorityHeadsCodec>(1)?;
        reservation.reserve_records::<ThreadExecutionsCodec>(1)?;
        reservation.reserve_records::<ThreadAttributesCodec>(1)?;
        reservation.reserve_records::<ThreadUsageCodec>(1)?;
        reservation.reserve_records::<ThreadCatalogSummariesCodec>(1)?;
        reservation.reserve_records::<DraftsCodec>(1)?;
        reservation.reserve_records::<DraftPieceRootsCodec>(1)?;
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        reservation.reserve_records::<DraftByThreadCodec>(1)?;
        reservation.reserve_records::<TranscriptHeadsCodec>(1)?;
        if records.transcript_build.is_some() {
            reservation.reserve_records::<TranscriptBuildsCodec>(1)?;
        }
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        reservation.reserve_records::<ActivityQueryHeadsCodec>(1)?;
        reservation.reserve_records::<BindingsCodec>(1)?;
        reservation.reserve_records::<BindingHeadsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = self.creation.records();
        mutations.put::<ThreadsCodec>(&records.thread.id(), &records.thread)?;
        mutations.put::<ImageLabelAuthorityHeadsCodec>(
            &records.thread.id(),
            &records.image_label_authority_head,
        )?;
        mutations.put::<ThreadExecutionsCodec>(&records.thread.id(), &records.execution)?;
        mutations.put::<ThreadAttributesCodec>(&records.thread.id(), &records.attributes)?;
        mutations.put::<ThreadUsageCodec>(&records.thread.id(), &records.usage)?;
        mutations
            .put::<ThreadCatalogSummariesCodec>(&records.thread.id(), &records.catalog_summary)?;
        mutations.put::<DraftsCodec>(&records.draft.id(), &records.draft)?;
        mutations.put::<DraftPieceRootsCodec>(
            &records.draft_piece_root.reference().key(),
            &records.draft_piece_root,
        )?;
        mutations.put::<DraftEditHistoryFrontiersCodec>(
            &records.draft_edit_history.reference().key(),
            &records.draft_edit_history,
        )?;
        mutations.put::<DraftByThreadCodec>(&records.thread.id(), &records.draft_index)?;
        mutations.put::<TranscriptHeadsCodec>(&records.thread.id(), &records.transcript_head)?;
        if let Some(build) = &records.transcript_build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        mutations.put::<HistorySummariesCodec>(&records.thread.id(), &records.summary)?;
        mutations.put::<InputGatesCodec>(&records.thread.id(), &records.input_gate)?;
        mutations.put::<ActivityQueryHeadsCodec>(&records.thread.id(), &records.activity_head)?;
        mutations.put::<BindingsCodec>(
            &BindingKey {
                thread: records.thread.id(),
                revision: records.binding.revision(),
            },
            &records.binding,
        )?;
        mutations.put::<BindingHeadsCodec>(&records.thread.id(), &records.binding_head)?;
        Ok(())
    }
}
