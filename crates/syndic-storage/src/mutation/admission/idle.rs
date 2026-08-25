use super::*;
use beryl_home_store::{DomainReader, MutationBuilder};
use beryl_model::{DiscussionContextOwnerId, ProjectionRevision};

use crate::{
    ActivityQueryHeadRecord, ActivityQuerySource, ActivityQuerySourceRecord, BindingHeadRecord,
    BindingLifecycle, BindingRecord, BindingState, CanonicalItemRecord, ConversationParent,
    DraftRecord, DraftSubmissionIntent, HistorySummaryRecord, InputGateRecord, InputGateState,
    ProjectionLifecycle, SelectedPathProof, ThreadRecord, TranscriptViewHeadRecord,
    TurnChildIndexRecord, TurnItemIndexRecord, TurnItemOrdinal, TurnKind, TurnLifecycle,
    TurnRecord, TurnStateRecord, TurnStateRevision, codec::Family, domain::SyndicDomain,
};

use super::shared::{AcceptanceBase, CommonRecords, IdleSpecificRecords};

pub(super) struct IdleRecords {
    common: CommonRecords,
    specific: IdleSpecificRecords,
}

pub(super) fn records(
    reader: &DomainReader<'_, SyndicDomain>,
    acceptance: &FirstAcceptance,
    base: AcceptanceBase,
) -> Result<IdleRecords, SyndicMutationError> {
    if base.gate.live_count() != 0 {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let turn_id = acceptance.submitted_turn_id();
    if point::<TurnsFamily>(reader, &turn_id)?.is_some()
        || point::<AcceptedInputsFamily>(reader, &acceptance.accepted_input_id())?.is_some()
        || point::<CanonicalItemsFamily>(reader, &acceptance.idle_user_item_id())?.is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let (context_move, context_owner, context_parent) = context_move(reader, &base.draft, turn_id)?;
    let parent = match base.draft.submission_intent() {
        DraftSubmissionIntent::Ordinary => {
            ConversationParent::from_turn(base.thread.committed_tail())
        }
        DraftSubmissionIntent::DiscussionContext(_) => {
            context_parent.ok_or(SyndicMutationError::CurrentDraftConflict)?
        }
        DraftSubmissionIntent::Replacement(intent) => {
            super::shared::validate_replacement_intent(reader, &base.thread, intent)?
                .0
                .parent()
        }
    };
    let (depth, digest, ancestor_skip) =
        crate::mutation::admission_helpers::turn_shape(reader, turn_id, parent)?;
    let (image_label_frontiers, origin_span) = super::shared::advance_image_label_authority(
        reader,
        &base.thread,
        crate::ImageLabelOriginOwner::CanonicalItem(acceptance.idle_user_item_id()),
        acceptance.asset_reference_set(),
    )?;
    let thread_revision = base.thread.revision().checked_next()?;
    let selected_path = SelectedPathProof::new(Some(turn_id), thread_revision, digest);
    let thread = ThreadRecord::new(
        base.thread.id(),
        selected_path,
        acceptance.next_draft_id(),
        base.thread.lineage(),
        image_label_frontiers,
        context_owner.or(base.thread.context_owner_id()),
    );
    let (draft, draft_index) = super::shared::fresh_draft(acceptance, &base, &thread)?;
    let turn = TurnRecord::new(
        turn_id,
        thread.id(),
        TurnKind::OrdinaryUser,
        parent,
        ancestor_skip,
        depth,
        digest,
        acceptance.admitted_at(),
    );
    let turn_state = TurnStateRecord::with_capture_frontiers(
        turn_id,
        TurnStateRevision::FIRST,
        TurnLifecycle::Pending,
        0,
        1,
        0,
        1,
        0,
        None,
        acceptance.admitted_at(),
    )?;
    let child_index = parent
        .turn()
        .map(|parent_id| TurnChildIndexRecord::new(parent_id, turn_id, depth, digest));
    let item_revision = ProjectionRevision::new(1)?;
    let item = CanonicalItemRecord::local_user_input(
        acceptance.idle_user_item_id(),
        turn_id,
        TurnItemOrdinal::FIRST,
        item_revision,
        acceptance.materialization().content(),
        acceptance.asset_reference_set(),
    );
    let item_index = TurnItemIndexRecord::new(
        turn_id,
        TurnItemOrdinal::FIRST,
        acceptance.idle_user_item_id(),
        item_revision,
    );
    let current_head = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
    let transcript_build =
        crate::mutation::transcript::supersede_active_transcript_build(reader, &base.thread)?;
    let transcript_head = TranscriptViewHeadRecord::new(
        thread.id(),
        current_head.generation().checked_next()?,
        current_head.revision().checked_next()?,
        0,
        Some(turn_id),
        digest,
        ProjectionLifecycle::Stale,
    );
    let summary = HistorySummaryRecord::new(
        thread.id(),
        base.summary.revision().checked_next()?,
        thread_revision,
        Some(turn_id),
        digest,
        false,
        acceptance.admitted_at(),
    );
    let gate = InputGateRecord::new(
        thread.id(),
        base.gate.revision().checked_next()?,
        InputGateState::PendingTurn(turn_id),
        base.gate.accepted_high_water(),
        base.gate.route_generation_high_water(),
        None,
        base.gate.live_steering_count(),
        base.gate.live_next_turn_count(),
        base.gate.live_logical_utf8_bytes(),
    )?;
    let current_activity = required::<ActivityQueryHeadsFamily>(reader, &thread.id())?;
    if current_activity.source_active()
        || current_activity.logical_row_count() != current_activity.completed_row_count()
    {
        return Err(SyndicMutationError::ActivityQueryConflict);
    }
    let work_period = if current_activity.source().is_none() {
        current_activity.work_period()
    } else {
        current_activity.work_period().checked_next()?
    };
    let activity_source_key = ActivityQuerySource::new(thread.id(), turn_id);
    let activity_head = ActivityQueryHeadRecord::new(
        thread.id(),
        work_period,
        Some(activity_source_key),
        true,
        0,
        current_activity.revision().checked_next()?,
        1,
        0,
        0,
        0,
        0,
        None,
        ProjectionLifecycle::Current,
    )?;
    let activity_source = ActivityQuerySourceRecord::new(
        thread.id(),
        work_period,
        activity_source_key,
        None,
        0,
        true,
        None,
    );
    let binding_head = required::<BindingHeadsFamily>(reader, &thread.id())?;
    let binding_revision = binding_head.revision().checked_next()?;
    if point::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread.id(),
            revision: binding_revision,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let binding = BindingRecord::new(
        thread.id(),
        binding_revision,
        selected_path,
        BindingState::unbound("submitted turn awaits an execution projection")?,
    );
    let binding_head = BindingHeadRecord::new(
        thread.id(),
        binding_revision,
        BindingLifecycle::Unbound,
        digest,
    );
    let thread_parent_index = super::shared::thread_parent_index(&thread);
    Ok(IdleRecords {
        common: CommonRecords {
            old_draft_id: base.draft.id(),
            thread,
            draft,
            draft_index,
            fresh_root: base.fresh_root,
            fresh_history: base.fresh_history,
            disposed_session: base.disposed_session,
            origin_span,
            summary,
            gate,
            thread_parent_index,
        },
        specific: IdleSpecificRecords {
            turn,
            turn_state,
            child_index,
            item,
            item_index,
            transcript_head,
            transcript_build,
            activity_head,
            activity_source,
            binding,
            binding_head,
            context_move,
        },
    })
}

impl IdleRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        self.common.contribute(mutations)?;
        let records = self.specific;
        mutations.put::<TurnsCodec>(&records.turn.id(), &records.turn)?;
        mutations.put::<TurnStatesCodec>(&records.turn.id(), &records.turn_state)?;
        if let Some(index) = &records.child_index {
            mutations.put::<TurnChildrenCodec>(
                &TurnPairKey {
                    parent: index.parent_id(),
                    child: index.child_id(),
                },
                index,
            )?;
        }
        mutations.put::<CanonicalItemsCodec>(&records.item.id(), &records.item)?;
        mutations.put::<TurnItemsCodec>(
            &TurnItemKey {
                owner: records.turn.id(),
                ordinal: TurnItemOrdinal::FIRST,
            },
            &records.item_index,
        )?;
        mutations
            .put::<TranscriptHeadsCodec>(&self.common.thread.id(), &records.transcript_head)?;
        if let Some(build) = &records.transcript_build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        mutations.put::<ActivityQueryHeadsCodec>(
            &records.activity_head.thread_id(),
            &records.activity_head,
        )?;
        mutations.put::<ActivityQuerySourcesCodec>(
            &ActivityQuerySourceKey {
                thread: records.activity_source.thread_id(),
                work_period: records.activity_source.work_period(),
                source_thread: records.activity_source.source().thread_id(),
                source_turn: records.activity_source.source().turn_id(),
            },
            &records.activity_source,
        )?;
        mutations.put::<BindingsCodec>(
            &BindingKey {
                thread: records.binding.thread_id(),
                revision: records.binding.revision(),
            },
            &records.binding,
        )?;
        mutations
            .put::<BindingHeadsCodec>(&records.binding_head.thread_id(), &records.binding_head)?;
        if let Some((old_owner, new_record)) = &records.context_move {
            mutations.delete::<ContextEnvelopesCodec>(&ContextOwnerKey::from(*old_owner))?;
            mutations.put::<ContextEnvelopesCodec>(
                &ContextOwnerKey::from(new_record.owner()),
                new_record,
            )?;
        }
        Ok(())
    }
}

fn context_move(
    reader: &DomainReader<'_, SyndicDomain>,
    draft: &DraftRecord,
    turn_id: beryl_model::SyndicTurnId,
) -> Result<
    (
        Option<(DiscussionContextOwnerId, crate::ContextEnvelopeRecord)>,
        Option<DiscussionContextOwnerId>,
        Option<ConversationParent>,
    ),
    SyndicMutationError,
> {
    let DraftSubmissionIntent::DiscussionContext(owner) = draft.submission_intent() else {
        return Ok((None, None, None));
    };
    if owner != DiscussionContextOwnerId::Draft(draft.id()) {
        return Err(SyndicMutationError::CurrentDraftConflict);
    }
    let record = required::<ContextEnvelopesFamily>(reader, &ContextOwnerKey::from(owner))?;
    let submitted = DiscussionContextOwnerId::SubmittedTurn(turn_id);
    if point::<ContextEnvelopesFamily>(reader, &ContextOwnerKey::from(submitted))?.is_some() {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    Ok((
        Some((
            owner,
            crate::ContextEnvelopeRecord::new(
                submitted,
                record.revision(),
                record.envelope().clone(),
            ),
        )),
        Some(submitted),
        Some(ConversationParent::Turn(
            record.envelope().descriptor().source().turn_id(),
        )),
    ))
}

fn required<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<F::Value, SyndicMutationError>
where
    F::Key: std::fmt::Debug,
{
    crate::mutation::required::<F>(reader, key)
}

fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, SyndicMutationError> {
    crate::mutation::point::<F>(reader, key)
}
