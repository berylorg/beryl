use beryl_model::{
    conversation::ConversationThreadId,
    threaded_decision::{
        ThreadedDecisionOperationId, ThreadedDecisionRecord, ThreadedDecisionRecordId,
        ThreadedDecisionState, ThreadedDecisionStatus,
    },
};

pub(crate) fn record_needs_child_archive(record: &ThreadedDecisionRecord) -> bool {
    matches!(
        record.status(),
        ThreadedDecisionStatus::ChecklistUpdated | ThreadedDecisionStatus::ArchivePending
    ) && record.child_thread_id().is_some()
}

pub(crate) fn record_blocks_child_writes(record: &ThreadedDecisionRecord) -> bool {
    record.child_thread_id().is_some()
        && matches!(
            record.status(),
            ThreadedDecisionStatus::ChecklistUpdated
                | ThreadedDecisionStatus::ArchivePending
                | ThreadedDecisionStatus::ArchiveFailed
                | ThreadedDecisionStatus::Closed
                | ThreadedDecisionStatus::Superseded
        )
}

pub(crate) fn child_thread_is_read_only_decision_branch(
    state: &ThreadedDecisionState,
    thread_id: &ConversationThreadId,
) -> bool {
    state.records().iter().any(|record| {
        record_blocks_child_writes(record) && record.child_thread_id() == Some(thread_id)
    })
}

pub(crate) fn normal_selector_hidden_decision_child_thread_ids(
    state: &ThreadedDecisionState,
) -> Vec<ConversationThreadId> {
    state
        .records()
        .iter()
        .filter(|record| record_blocks_child_writes(record))
        .filter_map(ThreadedDecisionRecord::child_thread_id)
        .cloned()
        .collect()
}

pub(crate) fn archive_operation_id_for_record(
    record_id: &ThreadedDecisionRecordId,
    timestamp_millis: u64,
) -> Option<ThreadedDecisionOperationId> {
    ThreadedDecisionOperationId::new(format!(
        "archive_decision_branch_{}_{}",
        sanitize_id_part(record_id.as_str()),
        timestamp_millis
    ))
    .ok()
}

fn sanitize_id_part(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            _ => '_',
        })
        .collect();
    if sanitized.is_empty() {
        "decision".to_string()
    } else {
        sanitized
    }
}
