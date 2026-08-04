use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, MutationContribution};
use beryl_model::{
    DomainRevision, DraftRevision, InputGateRevision, SealedAssetReferenceSetProof, SyndicDraftId,
    SyndicItemId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};

use crate::{
    CurrentTranscriptEntryProof, DraftByThreadRecord, DraftRecord, DraftSubmissionIntent,
    HistorySummaryRecord, InputGateState, ReplacementEditIntent, SelectedPathProof,
    SyndicMutationError, SyndicStorage, SyndicTimestamp, codec::*, domain::SyndicDomain,
};

use super::{current_draft, required};

/// Exact proof and revisions for entering durable replacement-edit mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartReplacementEdit {
    thread_id: SyndicThreadId,
    expected_thread_revision: ThreadRevision,
    draft_id: SyndicDraftId,
    expected_draft_revision: DraftRevision,
    expected_gate_revision: InputGateRevision,
    target_turn_id: SyndicTurnId,
    target_item_id: SyndicItemId,
    selected_path: SelectedPathProof,
    transcript_entry: CurrentTranscriptEntryProof,
    asset_reference_set: Option<SealedAssetReferenceSetProof>,
    started_at: SyndicTimestamp,
}

impl StartReplacementEdit {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_thread_revision: ThreadRevision,
        draft_id: SyndicDraftId,
        expected_draft_revision: DraftRevision,
        expected_gate_revision: InputGateRevision,
        target_turn_id: SyndicTurnId,
        target_item_id: SyndicItemId,
        selected_path: SelectedPathProof,
        transcript_entry: CurrentTranscriptEntryProof,
        asset_reference_set: Option<SealedAssetReferenceSetProof>,
        started_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            expected_thread_revision,
            draft_id,
            expected_draft_revision,
            expected_gate_revision,
            target_turn_id,
            target_item_id,
            selected_path,
            transcript_entry,
            asset_reference_set,
            started_at,
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
    pub const fn target_item_id(&self) -> SyndicItemId {
        self.target_item_id
    }

    #[must_use]
    pub const fn asset_reference_set(&self) -> Option<SealedAssetReferenceSetProof> {
        self.asset_reference_set
    }

    #[must_use]
    pub const fn started_at(&self) -> SyndicTimestamp {
        self.started_at
    }
}

/// Exact revisions for leaving replacement-edit mode without changing its payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancelReplacementEdit {
    thread_id: SyndicThreadId,
    expected_thread_revision: ThreadRevision,
    draft_id: SyndicDraftId,
    expected_draft_revision: DraftRevision,
    expected_gate_revision: InputGateRevision,
    cancelled_at: SyndicTimestamp,
}

impl CancelReplacementEdit {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_thread_revision: ThreadRevision,
        draft_id: SyndicDraftId,
        expected_draft_revision: DraftRevision,
        expected_gate_revision: InputGateRevision,
        cancelled_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            expected_thread_revision,
            draft_id,
            expected_draft_revision,
            expected_gate_revision,
            cancelled_at,
        }
    }
}

impl SyndicStorage {
    /// Atomically enters replacement-edit mode against one current transcript item.
    #[must_use]
    pub fn start_replacement_edit(
        &self,
        expected_domain_revision: DomainRevision,
        edit: StartReplacementEdit,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            StartReplacementEditMutation { edit },
        )
    }

    /// Atomically clears replacement intent while preserving the mutable payload.
    #[must_use]
    pub fn cancel_replacement_edit(
        &self,
        expected_domain_revision: DomainRevision,
        cancellation: CancelReplacementEdit,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            CancelReplacementEditMutation { cancellation },
        )
    }
}

struct StartReplacementEditMutation {
    edit: StartReplacementEdit,
}

struct CancelReplacementEditMutation {
    cancellation: CancelReplacementEdit,
}

struct ReplacementRecords {
    draft: DraftRecord,
    draft_index: DraftByThreadRecord,
    summary: Option<HistorySummaryRecord>,
}

struct ReplacementBase {
    thread: crate::ThreadRecord,
    draft: DraftRecord,
    summary: HistorySummaryRecord,
}

impl DomainMutation<SyndicDomain> for StartReplacementEditMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

impl DomainMutation<SyndicDomain> for CancelReplacementEditMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

impl ReplacementRecords {
    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<DraftsCodec>(&self.draft.id(), &self.draft)?;
        mutations.put::<DraftByThreadCodec>(&self.draft.thread_id(), &self.draft_index)?;
        if let Some(summary) = &self.summary {
            mutations.put::<HistorySummariesCodec>(&self.draft.thread_id(), summary)?;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn load_base(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    expected_thread_revision: ThreadRevision,
    draft_id: SyndicDraftId,
    expected_draft_revision: DraftRevision,
    expected_gate_revision: InputGateRevision,
    changed_at: SyndicTimestamp,
) -> Result<ReplacementBase, SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &thread_id)?;
    if thread.revision() != expected_thread_revision {
        return Err(SyndicMutationError::ThreadRevisionConflict {
            expected: expected_thread_revision,
            current: thread.revision(),
        });
    }
    let draft = current_draft(reader, thread_id)?;
    if draft.id() != draft_id {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    if draft.revision() != expected_draft_revision {
        return Err(SyndicMutationError::DraftRevisionConflict {
            expected: expected_draft_revision,
            current: draft.revision(),
        });
    }
    let gate = required::<InputGatesFamily>(reader, &thread_id)?;
    if gate.revision() != expected_gate_revision {
        return Err(SyndicMutationError::InputGateRevisionConflict {
            expected: expected_gate_revision,
            current: gate.revision(),
        });
    }
    if !matches!(gate.state(), InputGateState::Idle) || gate.live_count() != 0 {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let summary = required::<HistorySummariesFamily>(reader, &thread_id)?;
    if changed_at < draft.updated_at() || changed_at < summary.last_activity_at() {
        return Err(SyndicMutationError::TimestampRegressed);
    }
    Ok(ReplacementBase {
        thread,
        draft,
        summary,
    })
}

impl StartReplacementEditMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<ReplacementRecords, SyndicMutationError> {
        let edit = &self.edit;
        let base = load_base(
            reader,
            edit.thread_id,
            edit.expected_thread_revision,
            edit.draft_id,
            edit.expected_draft_revision,
            edit.expected_gate_revision,
            edit.started_at,
        )?;
        match base.draft.submission_intent() {
            DraftSubmissionIntent::Ordinary => {}
            DraftSubmissionIntent::Replacement(_) => {
                return Err(SyndicMutationError::ReplacementEditAlreadyActive);
            }
            DraftSubmissionIntent::DiscussionContext(_) => {
                return Err(SyndicMutationError::ReplacementTargetConflict);
            }
        }
        let empty = super::admission::canonical_empty_content(reader)?;
        if base.draft.content() != empty {
            return Err(SyndicMutationError::ReplacementDraftNotEmpty);
        }

        let intent = ReplacementEditIntent::new(
            edit.target_turn_id,
            edit.selected_path,
            edit.transcript_entry,
        );
        let (_target, item) =
            super::admission::validate_replacement_intent(reader, &base.thread, intent)?;
        if item.id() != edit.target_item_id {
            return Err(SyndicMutationError::ReplacementTargetConflict);
        }
        let content = item
            .presentation_content()
            .ok_or(SyndicMutationError::ReplacementTargetConflict)?;
        super::admission::require_sealed_composer(reader, content)?;
        super::admission::validate_asset_reference_set(content, edit.asset_reference_set)?;
        if item.presentation().asset_reference_set() != edit.asset_reference_set {
            return Err(SyndicMutationError::AssetReferenceSetConflict);
        }

        let revision = base.draft.revision().checked_next()?;
        let draft = DraftRecord::new(
            base.draft.id(),
            base.draft.thread_id(),
            revision,
            DraftSubmissionIntent::Replacement(intent),
            content,
            base.draft.created_at(),
            edit.started_at,
        );
        replacement_records(base, draft)
    }
}

impl CancelReplacementEditMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<ReplacementRecords, SyndicMutationError> {
        let cancellation = self.cancellation;
        let base = load_base(
            reader,
            cancellation.thread_id,
            cancellation.expected_thread_revision,
            cancellation.draft_id,
            cancellation.expected_draft_revision,
            cancellation.expected_gate_revision,
            cancellation.cancelled_at,
        )?;
        if !matches!(
            base.draft.submission_intent(),
            DraftSubmissionIntent::Replacement(_)
        ) {
            return Err(SyndicMutationError::ReplacementEditNotActive);
        }
        let revision = base.draft.revision().checked_next()?;
        let draft = DraftRecord::new(
            base.draft.id(),
            base.draft.thread_id(),
            revision,
            DraftSubmissionIntent::Ordinary,
            base.draft.content(),
            base.draft.created_at(),
            cancellation.cancelled_at,
        );
        replacement_records(base, draft)
    }
}

fn replacement_records(
    base: ReplacementBase,
    draft: DraftRecord,
) -> Result<ReplacementRecords, SyndicMutationError> {
    let draft_index = DraftByThreadRecord::new(
        base.thread.id(),
        draft.id(),
        draft.revision(),
        base.thread.revision(),
    );
    let next_activity = base.summary.last_activity_at().max(draft.updated_at());
    let summary = if next_activity != base.summary.last_activity_at() {
        Some(HistorySummaryRecord::new(
            base.summary.thread_id(),
            base.summary.revision().checked_next()?,
            base.summary.thread_revision(),
            base.summary.committed_tail(),
            base.summary.selected_path_digest(),
            base.summary.complete(),
            next_activity,
        ))
    } else {
        None
    };
    Ok(ReplacementRecords {
        draft,
        draft_index,
        summary,
    })
}
