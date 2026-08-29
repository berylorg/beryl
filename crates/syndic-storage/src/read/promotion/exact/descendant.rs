use super::*;

pub(super) fn accepted_admission_descendant_count(
    thread: &ThreadRecord,
    expected_thread: &ThreadRecord,
    gate: Option<&InputGateRecord>,
    expected_gate: &InputGateRecord,
) -> Option<u64> {
    let gate = gate?;
    let thread_advance = thread
        .revision()
        .get()
        .checked_sub(expected_thread.revision().get())?;
    let gate_advance = gate
        .revision()
        .get()
        .checked_sub(expected_gate.revision().get())?;
    let accepted_advance = gate
        .accepted_high_water()
        .checked_sub(expected_gate.accepted_high_water())?;
    let next_turn_advance = gate
        .live_next_turn_count()
        .checked_sub(expected_gate.live_next_turn_count())?;
    let generation_advance = match (
        gate.route_generation_high_water(),
        expected_gate.route_generation_high_water(),
    ) {
        (Some(current), Some(expected)) => current.get().checked_sub(expected.get())?,
        (None, None) => 0,
        _ => return None,
    };
    let draft_identity_agrees = if thread_advance == 0 {
        thread.current_draft_id() == expected_thread.current_draft_id()
    } else {
        thread.current_draft_id() != expected_thread.current_draft_id()
    };
    let bytes_agree = if thread_advance == 0 {
        gate.live_logical_utf8_bytes() == expected_gate.live_logical_utf8_bytes()
    } else {
        gate.live_logical_utf8_bytes() >= expected_gate.live_logical_utf8_bytes()
    };
    (thread.id() == expected_thread.id()
        && thread.committed_tail() == expected_thread.committed_tail()
        && thread.selected_path_digest() == expected_thread.selected_path_digest()
        && thread.lineage() == expected_thread.lineage()
        && thread.context_owner_id() == expected_thread.context_owner_id()
        && draft_identity_agrees
        && gate.thread_id() == expected_gate.thread_id()
        && gate.state() == expected_gate.state()
        && gate.selected_route() == expected_gate.selected_route()
        && gate.live_steering_count() == expected_gate.live_steering_count()
        && bytes_agree
        && gate_advance == thread_advance
        && accepted_advance == thread_advance
        && generation_advance == thread_advance
        && next_turn_advance == thread_advance)
        .then_some(thread_advance)
}

pub(super) fn draft_index_descendant(
    index: Option<&DraftByThreadRecord>,
    draft: Option<&DraftRecord>,
    thread: &ThreadRecord,
    expected: &DraftByThreadRecord,
    admission_count: u64,
) -> Option<u64> {
    let index = index?;
    let draft = draft?;
    if index.thread_id() != thread.id()
        || index.draft_id() != thread.current_draft_id()
        || index.thread_revision() != thread.revision()
        || draft.id() != index.draft_id()
        || draft.thread_id() != thread.id()
        || draft.revision() != index.draft_revision()
        || !matches!(
            draft.submission_intent(),
            crate::DraftSubmissionIntent::Ordinary
        )
        || draft.created_at() > draft.updated_at()
    {
        return None;
    }
    if admission_count == 0 {
        if index.draft_id() != expected.draft_id() {
            return None;
        }
        index
            .draft_revision()
            .get()
            .checked_sub(expected.draft_revision().get())
    } else {
        if index.draft_id() == expected.draft_id() {
            return None;
        }
        index.draft_revision().get().checked_sub(1)
    }
}

pub(super) fn summary_agrees(
    summary: Option<&HistorySummaryRecord>,
    thread: &ThreadRecord,
    draft: &DraftRecord,
    expected: &HistorySummaryRecord,
    admission_count: u64,
    draft_advance: u64,
) -> bool {
    summary.is_some_and(|summary| {
        let (activity_baseline, expected_activity) = if admission_count == 0 {
            (
                expected.last_activity_at(),
                expected.last_activity_at().max(draft.updated_at()),
            )
        } else {
            if draft.created_at() < expected.last_activity_at() {
                return false;
            }
            (draft.created_at(), draft.updated_at())
        };
        let activity_changed = expected_activity > activity_baseline;
        let minimum_revision_advance = admission_count.checked_add(u64::from(activity_changed));
        let maximum_revision_advance =
            admission_count.checked_add(if activity_changed { draft_advance } else { 0 });
        let revision_advance = summary
            .revision()
            .get()
            .checked_sub(expected.revision().get());
        summary.thread_id() == thread.id()
            && summary.thread_revision() == thread.revision()
            && summary.committed_tail() == thread.committed_tail()
            && summary.selected_path_digest() == thread.selected_path_digest()
            && !summary.complete()
            && summary.last_activity_at() == expected_activity
            && revision_advance.is_some_and(|advance| {
                minimum_revision_advance.is_some_and(|minimum| advance >= minimum)
                    && (admission_count != 0
                        || maximum_revision_advance.is_some_and(|maximum| advance <= maximum))
            })
    })
}

pub(super) fn activity_agrees(
    head: Option<&ActivityQueryHeadRecord>,
    source: Option<&ActivityQuerySourceRecord>,
    expected_head: &ActivityQueryHeadRecord,
    expected_source: &ActivityQuerySourceRecord,
) -> bool {
    let (Some(head), Some(source)) = (head, source) else {
        return false;
    };
    if source != expected_source {
        return false;
    }
    let Some(revision_advance) = head
        .revision()
        .get()
        .checked_sub(expected_head.revision().get())
    else {
        return false;
    };
    if revision_advance == 0 {
        return head == expected_head;
    }
    let Some(source_count) = expected_head.source_count().checked_add(revision_advance) else {
        return false;
    };
    let Some(minimum_frontier) = revision_advance
        .checked_mul(2)
        .and_then(|advance| expected_head.source_frontier().checked_add(advance))
    else {
        return false;
    };
    head.thread_id() == expected_head.thread_id()
        && head.work_period() == expected_head.work_period()
        && head.source() == expected_head.source()
        && head.source_active() == expected_head.source_active()
        && head.lifecycle() == ProjectionLifecycle::Current
        && head.source_count() == source_count
        && head.source_frontier() >= minimum_frontier
        && head.running_row_count() == 0
        && head.logical_row_count() == head.completed_row_count()
        && head.completed_row_count() != 0
        && head.completed_row_count() <= revision_advance
        && head.completed_stored_bytes() != 0
        && head.completed_retention_cutoff().is_some()
}

pub(super) fn transcript_agrees(
    head: Option<&TranscriptViewHeadRecord>,
    build: Option<&TranscriptBuildRecord>,
    expected_head: &TranscriptViewHeadRecord,
    thread: &ThreadRecord,
    expected_thread: &ThreadRecord,
) -> bool {
    let Some(head) = head else {
        return false;
    };
    let Some(generation_advance) = head
        .generation()
        .get()
        .checked_sub(expected_head.generation().get())
    else {
        return false;
    };
    let Some(revision_advance) = head
        .revision()
        .get()
        .checked_sub(expected_head.revision().get())
    else {
        return false;
    };
    if head.thread_id() != expected_head.thread_id()
        || head.committed_tail() != expected_head.committed_tail()
        || head.selected_path_digest() != expected_head.selected_path_digest()
        || generation_advance > revision_advance
    {
        return false;
    }
    let Some(build) = build else {
        return if head == expected_head {
            true
        } else {
            generation_advance > 0
                && revision_advance > 0
                && head.lifecycle() == ProjectionLifecycle::Stale
                && head.entry_count() == 0
        };
    };
    if build.thread_id() != head.thread_id()
        || build.generation() != head.generation()
        || build.revision() != head.revision()
        || build.committed_tail() != head.committed_tail()
        || build.selected_path_digest() != head.selected_path_digest()
        || build.source_thread_revision() < expected_thread.revision()
        || build.source_thread_revision() > thread.revision()
    {
        return false;
    }
    match build.phase() {
        crate::TranscriptBuildPhase::Collecting { .. }
        | crate::TranscriptBuildPhase::Publishing { .. } => {
            head.lifecycle() == ProjectionLifecycle::Stale && head.entry_count() == 0
        }
        crate::TranscriptBuildPhase::Complete => {
            !build.history_complete()
                && head.lifecycle() == ProjectionLifecycle::Current
                && head.entry_count() == build.entry_count()
        }
        crate::TranscriptBuildPhase::Superseded => false,
    }
}
