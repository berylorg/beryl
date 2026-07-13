use beryl_model::{
    semantic_graph::{ChecklistItemKind, SemanticNode, SemanticNodeId, ThreadRef},
    threaded_decision::{
        ThreadedDecisionOutcome, ThreadedDecisionRecord, ThreadedDecisionState,
        ThreadedDecisionStatus,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecisionGraphBadgeTone {
    Neutral,
    Pending,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DecisionGraphBadge {
    label: &'static str,
    tone: DecisionGraphBadgeTone,
}

impl DecisionGraphBadge {
    pub(crate) fn label(self) -> &'static str {
        self.label
    }

    pub(crate) fn tone(self) -> DecisionGraphBadgeTone {
        self.tone
    }
}

pub(crate) fn decision_item_badges(
    node: &SemanticNode,
    decisions: &ThreadedDecisionState,
) -> Vec<DecisionGraphBadge> {
    let mut badges = Vec::new();
    let item_records = records_for_item(decisions, node.id());
    let is_decision_item = node.checklist_item_kind() == Some(ChecklistItemKind::Decision);

    if is_decision_item {
        badges.push(badge("decision", DecisionGraphBadgeTone::Neutral));
    }

    if let Some(record) = latest_blocking_record(item_records.iter().copied()) {
        badges.push(status_badge(record));
        return badges;
    }

    if let Some(record) = latest_terminal_record(item_records.iter().copied()) {
        badges.push(status_badge(record));
    }

    if item_records
        .iter()
        .any(|record| record.status() == ThreadedDecisionStatus::Superseded)
        && !badges.iter().any(|badge| badge.label == "superseded")
    {
        badges.push(badge("history", DecisionGraphBadgeTone::Neutral));
    }

    badges
}

pub(crate) fn decision_thread_ref_badge(
    decisions: &ThreadedDecisionState,
    thread_ref: &ThreadRef,
) -> Option<DecisionGraphBadge> {
    let record = records_for_item(decisions, thread_ref.node_id())
        .into_iter()
        .find(|record| record.child_thread_id() == Some(thread_ref.thread_id()))?;
    Some(status_badge(record))
}

pub(crate) fn active_decision_branch_record_for_item<'a>(
    decisions: &'a ThreadedDecisionState,
    node_id: &SemanticNodeId,
) -> Option<&'a ThreadedDecisionRecord> {
    records_for_item(decisions, node_id)
        .into_iter()
        .filter(|record| record.status() == ThreadedDecisionStatus::ActiveBranch)
        .max_by_key(|record| record.updated_at_millis())
}

pub(crate) fn latest_handoff_record_for_item<'a>(
    decisions: &'a ThreadedDecisionState,
    node_id: &SemanticNodeId,
) -> Option<&'a ThreadedDecisionRecord> {
    records_for_item(decisions, node_id)
        .into_iter()
        .filter(|record| record.handoff_turn_id().is_some())
        .max_by_key(|record| {
            record
                .resolved_at_millis()
                .unwrap_or_else(|| record.updated_at_millis())
        })
}

pub(crate) fn checklist_update_retry_record_for_item<'a>(
    decisions: &'a ThreadedDecisionState,
    node_id: &SemanticNodeId,
) -> Option<&'a ThreadedDecisionRecord> {
    records_for_item(decisions, node_id)
        .into_iter()
        .find(|record| {
            record.status() == ThreadedDecisionStatus::HandoffStarted
                && record.handoff_turn_id().is_some()
        })
}

pub(crate) fn archive_retry_record_for_item<'a>(
    decisions: &'a ThreadedDecisionState,
    node_id: &SemanticNodeId,
) -> Option<&'a ThreadedDecisionRecord> {
    records_for_item(decisions, node_id)
        .into_iter()
        .find(|record| record.status() == ThreadedDecisionStatus::ArchiveFailed)
}

pub(crate) fn decision_branch_start_label(
    decisions: &ThreadedDecisionState,
    node_id: &SemanticNodeId,
) -> &'static str {
    if records_for_item(decisions, node_id)
        .into_iter()
        .any(|record| {
            matches!(
                record.status(),
                ThreadedDecisionStatus::Closed | ThreadedDecisionStatus::Superseded
            )
        })
    {
        "Start Superseding Branch"
    } else {
        "Start Decision Branch"
    }
}

fn records_for_item<'a>(
    decisions: &'a ThreadedDecisionState,
    node_id: &SemanticNodeId,
) -> Vec<&'a ThreadedDecisionRecord> {
    decisions
        .records()
        .iter()
        .filter(|record| record.checklist_item_id() == node_id)
        .collect()
}

fn latest_blocking_record<'a>(
    records: impl Iterator<Item = &'a ThreadedDecisionRecord>,
) -> Option<&'a ThreadedDecisionRecord> {
    records
        .filter(|record| record.blocks_new_branch())
        .max_by_key(|record| record.updated_at_millis())
}

fn latest_terminal_record<'a>(
    records: impl Iterator<Item = &'a ThreadedDecisionRecord>,
) -> Option<&'a ThreadedDecisionRecord> {
    records
        .filter(|record| {
            matches!(
                record.status(),
                ThreadedDecisionStatus::Closed
                    | ThreadedDecisionStatus::Superseded
                    | ThreadedDecisionStatus::Invalidated
            )
        })
        .max_by_key(|record| {
            (
                terminal_status_priority(record.status()),
                record.updated_at_millis(),
            )
        })
}

fn terminal_status_priority(status: ThreadedDecisionStatus) -> u8 {
    match status {
        ThreadedDecisionStatus::Closed => 2,
        ThreadedDecisionStatus::Superseded => 1,
        ThreadedDecisionStatus::Invalidated => 0,
        ThreadedDecisionStatus::QueuedBranch
        | ThreadedDecisionStatus::ActiveBranch
        | ThreadedDecisionStatus::PendingResolution
        | ThreadedDecisionStatus::HandoffStarted
        | ThreadedDecisionStatus::ChecklistUpdated
        | ThreadedDecisionStatus::ArchivePending
        | ThreadedDecisionStatus::ArchiveFailed => 0,
    }
}

fn status_badge(record: &ThreadedDecisionRecord) -> DecisionGraphBadge {
    match record.status() {
        ThreadedDecisionStatus::QueuedBranch => badge("queued", DecisionGraphBadgeTone::Pending),
        ThreadedDecisionStatus::ActiveBranch => badge("active", DecisionGraphBadgeTone::Pending),
        ThreadedDecisionStatus::PendingResolution => {
            badge("handoff pending", DecisionGraphBadgeTone::Pending)
        }
        ThreadedDecisionStatus::HandoffStarted if record.handoff_turn_id().is_some() => {
            badge("checklist retry", DecisionGraphBadgeTone::Warning)
        }
        ThreadedDecisionStatus::HandoffStarted => {
            badge("handoff uncertain", DecisionGraphBadgeTone::Warning)
        }
        ThreadedDecisionStatus::ChecklistUpdated => {
            badge("close queued", DecisionGraphBadgeTone::Pending)
        }
        ThreadedDecisionStatus::ArchivePending => badge("closing", DecisionGraphBadgeTone::Pending),
        ThreadedDecisionStatus::ArchiveFailed => {
            badge("close failed", DecisionGraphBadgeTone::Error)
        }
        ThreadedDecisionStatus::Closed => match record.outcome() {
            Some(ThreadedDecisionOutcome::Accepted) => {
                badge("accepted", DecisionGraphBadgeTone::Neutral)
            }
            Some(ThreadedDecisionOutcome::Rejected) => {
                badge("rejected", DecisionGraphBadgeTone::Neutral)
            }
            None => badge("closed", DecisionGraphBadgeTone::Neutral),
        },
        ThreadedDecisionStatus::Superseded => badge("superseded", DecisionGraphBadgeTone::Warning),
        ThreadedDecisionStatus::Invalidated => badge("invalid", DecisionGraphBadgeTone::Error),
    }
}

fn badge(label: &'static str, tone: DecisionGraphBadgeTone) -> DecisionGraphBadge {
    DecisionGraphBadge { label, tone }
}
