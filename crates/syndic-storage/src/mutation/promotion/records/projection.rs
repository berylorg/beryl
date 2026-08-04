use super::*;

pub(super) struct PromotionProjectionRecords {
    pub(super) transcript_head: TranscriptViewHeadRecord,
    pub(super) transcript_build: Option<TranscriptBuildRecord>,
    pub(super) summary: HistorySummaryRecord,
    pub(super) gate: InputGateRecord,
    pub(super) activity_head: ActivityQueryHeadRecord,
    pub(super) activity_source: ActivityQuerySourceRecord,
    pub(super) binding: BindingRecord,
    pub(super) binding_head: BindingHeadRecord,
    pub(super) thread_parent_index: Option<ThreadParentIndexRecord>,
}

pub(super) fn projection_records(
    reader: &DomainReader<'_, SyndicDomain>,
    basis: &AcceptedNextCandidateBasis,
    promotion: &PromoteAcceptedInput,
    thread: &ThreadRecord,
    selected_path: SelectedPathProof,
) -> Result<PromotionProjectionRecords, SyndicMutationError> {
    let current_head = basis.transcript_head();
    if current_head.thread_id() != basis.thread().id()
        || current_head.committed_tail() != basis.thread().committed_tail()
        || current_head.selected_path_digest() != basis.thread().selected_path_digest()
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let transcript_build =
        crate::mutation::transcript::supersede_active_transcript_build(reader, basis.thread())?;
    let transcript_head = TranscriptViewHeadRecord::new(
        thread.id(),
        current_head.generation().checked_next()?,
        current_head.revision().checked_next()?,
        0,
        Some(promotion.successor_turn_id()),
        selected_path.digest(),
        ProjectionLifecycle::Stale,
    );
    let summary = HistorySummaryRecord::new(
        thread.id(),
        basis.summary().revision().checked_next()?,
        thread.revision(),
        Some(promotion.successor_turn_id()),
        selected_path.digest(),
        false,
        promotion.promoted_at(),
    );

    let current_gate = basis.gate();
    let logical_bytes = basis.input().content().summary().logical_utf8_bytes();
    let gate = InputGateRecord::new(
        thread.id(),
        current_gate.revision().checked_next()?,
        InputGateState::PendingTurn(promotion.successor_turn_id()),
        current_gate.accepted_high_water(),
        current_gate.route_generation_high_water(),
        None,
        current_gate.live_steering_count(),
        current_gate
            .live_next_turn_count()
            .checked_sub(1)
            .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?,
        current_gate
            .live_logical_utf8_bytes()
            .checked_sub(logical_bytes)
            .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?,
    )?;

    let current_activity = basis.activity_head();
    if current_activity.thread_id() != thread.id()
        || current_activity.source_active()
        || current_activity.logical_row_count() != current_activity.completed_row_count()
    {
        return Err(SyndicMutationError::ActivityQueryConflict);
    }
    let work_period = if current_activity.source().is_none() {
        current_activity.work_period()
    } else {
        current_activity.work_period().checked_next()?
    };
    let activity_source_identity =
        ActivityQuerySource::new(thread.id(), promotion.successor_turn_id());
    let activity_head = ActivityQueryHeadRecord::new(
        thread.id(),
        work_period,
        Some(activity_source_identity),
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
        activity_source_identity,
        None,
        0,
        true,
        None,
    );

    let binding_revision = basis.binding_head().revision().checked_next()?;
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
        BindingState::unbound("promoted accepted input awaits an execution projection")?,
    );
    let binding_head = BindingHeadRecord::new(
        thread.id(),
        binding_revision,
        BindingLifecycle::Unbound,
        selected_path.digest(),
    );
    let thread_parent_index = crate::mutation::admission::thread_parent_index(thread);
    Ok(PromotionProjectionRecords {
        transcript_head,
        transcript_build,
        summary,
        gate,
        activity_head,
        activity_source,
        binding,
        binding_head,
        thread_parent_index,
    })
}
