use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::MutationProvenance,
    semantic_graph::{
        ChecklistItemKind, ChecklistItemStatus, SemanticGraph, SemanticGraphPatch,
        SemanticGraphPatchOp, SemanticNodeDraft, SemanticNodeFacets, SemanticNodeId,
        ThreadRefDraft, ThreadRefId,
    },
    threaded_decision::{ThreadedDecisionRecordId, ThreadedDecisionState, ThreadedDecisionStatus},
    workspace::WorkspaceId,
};

const UNTITLED_DECISION_BRANCH_LABEL: &str = "Decision branch";
pub(crate) const MAX_TOPIC_DECISION_ITEM_TITLE_CHARS: usize = 160;
pub(crate) const MAX_TOPIC_DECISION_ITEM_SUMMARY_CHARS: usize = 1200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecisionBranchStartBlocker {
    GraphWorkActive,
    BackendUnavailable,
    MissingChecklistItem,
    NotChecklistItem,
    MissingParentThread,
    MissingBranchPoint,
    ActiveBranchExists,
    ParentThreadBusyWithoutQueue,
    ParentCompacting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DecisionBranchStartGate {
    pub(crate) graph_work_active: bool,
    pub(crate) backend_available: bool,
    pub(crate) checklist_item_exists: bool,
    pub(crate) checklist_item: bool,
    pub(crate) parent_thread_registered: bool,
    pub(crate) branch_point_available: bool,
    pub(crate) active_branch_exists: bool,
    pub(crate) parent_thread_busy_without_queue: bool,
    pub(crate) parent_compacting: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueuedDecisionBranchRunBlocker {
    BranchWorkerActive,
    GraphWorkActive,
    ParentTurnActive,
    ParentCompacting,
    BackendUnavailable,
    MissingChecklistItem,
    MissingParentThread,
    MissingBranchPoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QueuedDecisionBranchRunGate {
    pub(crate) branch_worker_active: bool,
    pub(crate) graph_work_active: bool,
    pub(crate) parent_turn_active: bool,
    pub(crate) parent_compacting: bool,
    pub(crate) backend_available: bool,
    pub(crate) checklist_item_exists: bool,
    pub(crate) parent_thread_registered: bool,
    pub(crate) branch_point_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecisionChildProgressPatch {
    pub(crate) record_id: ThreadedDecisionRecordId,
    pub(crate) checklist_item_id: SemanticNodeId,
    pub(crate) patch: SemanticGraphPatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TopicDecisionItemPlan {
    pub(crate) checklist_item_id: SemanticNodeId,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) patch: Option<SemanticGraphPatch>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TopicDecisionItemPlanError {
    MissingTopic,
    NotTopic,
    EmptyTitle,
    TitleTooLong { char_count: usize, max_chars: usize },
    SummaryTooLong { char_count: usize, max_chars: usize },
    DuplicateSiblingTitle { existing_node_id: SemanticNodeId },
    InvalidGeneratedId,
}

pub(crate) fn decision_branch_start_blocker(
    gate: DecisionBranchStartGate,
) -> Option<DecisionBranchStartBlocker> {
    if gate.graph_work_active {
        return Some(DecisionBranchStartBlocker::GraphWorkActive);
    }
    if !gate.backend_available {
        return Some(DecisionBranchStartBlocker::BackendUnavailable);
    }
    if !gate.checklist_item_exists {
        return Some(DecisionBranchStartBlocker::MissingChecklistItem);
    }
    if !gate.checklist_item {
        return Some(DecisionBranchStartBlocker::NotChecklistItem);
    }
    if !gate.parent_thread_registered {
        return Some(DecisionBranchStartBlocker::MissingParentThread);
    }
    if !gate.branch_point_available {
        return Some(DecisionBranchStartBlocker::MissingBranchPoint);
    }
    if gate.active_branch_exists {
        return Some(DecisionBranchStartBlocker::ActiveBranchExists);
    }
    if gate.parent_thread_busy_without_queue {
        return Some(DecisionBranchStartBlocker::ParentThreadBusyWithoutQueue);
    }
    if gate.parent_compacting {
        return Some(DecisionBranchStartBlocker::ParentCompacting);
    }
    None
}

pub(crate) fn queued_decision_branch_run_blocker(
    gate: QueuedDecisionBranchRunGate,
) -> Option<QueuedDecisionBranchRunBlocker> {
    if gate.branch_worker_active {
        return Some(QueuedDecisionBranchRunBlocker::BranchWorkerActive);
    }
    if gate.graph_work_active {
        return Some(QueuedDecisionBranchRunBlocker::GraphWorkActive);
    }
    if gate.parent_turn_active {
        return Some(QueuedDecisionBranchRunBlocker::ParentTurnActive);
    }
    if gate.parent_compacting {
        return Some(QueuedDecisionBranchRunBlocker::ParentCompacting);
    }
    if !gate.backend_available {
        return Some(QueuedDecisionBranchRunBlocker::BackendUnavailable);
    }
    if !gate.checklist_item_exists {
        return Some(QueuedDecisionBranchRunBlocker::MissingChecklistItem);
    }
    if !gate.parent_thread_registered {
        return Some(QueuedDecisionBranchRunBlocker::MissingParentThread);
    }
    if !gate.branch_point_available {
        return Some(QueuedDecisionBranchRunBlocker::MissingBranchPoint);
    }
    None
}

impl DecisionBranchStartBlocker {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::GraphWorkActive => "Retry after the current semantic graph operation finishes.",
            Self::BackendUnavailable => {
                "Beryl does not have an active managed backend for the parent thread."
            }
            Self::MissingChecklistItem => "That checklist item is no longer available.",
            Self::NotChecklistItem => "Decision branches can only start from checklist items.",
            Self::MissingParentThread => {
                "Open a registered parent conversation thread before starting a decision branch."
            }
            Self::MissingBranchPoint => {
                "The parent conversation has no loaded turn to use as the branch point."
            }
            Self::ActiveBranchExists => {
                "This checklist item already has an active decision branch."
            }
            Self::ParentThreadBusyWithoutQueue => {
                "The parent thread is busy and Beryl cannot identify the active turn to queue after."
            }
            Self::ParentCompacting => "Wait for context compaction to finish before branching.",
        }
    }
}

pub(crate) fn decision_branch_graph_patch(
    graph: &SemanticGraph,
    checklist_item_id: &SemanticNodeId,
    child_thread_id: ConversationThreadId,
    execution_target: WorkspaceId,
    provenance: &MutationProvenance,
) -> Option<(ThreadRefDraft, SemanticGraphPatch)> {
    let node = graph.node(checklist_item_id)?;
    if !node.facets().has_checklist_item() {
        return None;
    }

    let thread_ref = ThreadRefDraft::new(
        next_thread_ref_id(graph, checklist_item_id, &child_thread_id),
        checklist_item_id.clone(),
        child_thread_id,
        execution_target,
        thread_ref_label(),
    );
    let patch = SemanticGraphPatch::new(vec![
        SemanticGraphPatchOp::SetChecklistItemKind {
            node_id: checklist_item_id.clone(),
            kind: ChecklistItemKind::Decision,
            provenance: provenance.clone(),
        },
        SemanticGraphPatchOp::UpsertThreadRef {
            thread_ref: thread_ref.clone(),
            provenance: provenance.clone(),
        },
    ]);

    Some((thread_ref, patch))
}

pub(crate) fn default_topic_decision_title(
    graph: &SemanticGraph,
    topic_id: &SemanticNodeId,
) -> Option<String> {
    let topic = graph.node(topic_id)?;
    let title = normalize_inline_text(format!("Decision: {}", topic.title()));
    Some(truncate_chars(&title, MAX_TOPIC_DECISION_ITEM_TITLE_CHARS))
}

pub(crate) fn topic_decision_item_plan(
    graph: &SemanticGraph,
    topic_id: &SemanticNodeId,
    title: impl Into<String>,
    summary: impl Into<String>,
    provenance: &MutationProvenance,
) -> Result<TopicDecisionItemPlan, TopicDecisionItemPlanError> {
    let topic = graph
        .node(topic_id)
        .ok_or(TopicDecisionItemPlanError::MissingTopic)?;
    if !topic.facets().has_topic() {
        return Err(TopicDecisionItemPlanError::NotTopic);
    }

    let title = normalize_inline_text(title.into());
    if title.is_empty() {
        return Err(TopicDecisionItemPlanError::EmptyTitle);
    }
    let title_char_count = title.chars().count();
    if title_char_count > MAX_TOPIC_DECISION_ITEM_TITLE_CHARS {
        return Err(TopicDecisionItemPlanError::TitleTooLong {
            char_count: title_char_count,
            max_chars: MAX_TOPIC_DECISION_ITEM_TITLE_CHARS,
        });
    }

    let summary = normalize_summary_text(summary.into(), topic.title());
    let summary_char_count = summary.chars().count();
    if summary_char_count > MAX_TOPIC_DECISION_ITEM_SUMMARY_CHARS {
        return Err(TopicDecisionItemPlanError::SummaryTooLong {
            char_count: summary_char_count,
            max_chars: MAX_TOPIC_DECISION_ITEM_SUMMARY_CHARS,
        });
    }

    for child in graph.child_nodes_of(topic_id) {
        if normalize_inline_text(child.title()) == title {
            if child.facets().has_checklist_item()
                && child.checklist_item_kind() == Some(ChecklistItemKind::Decision)
            {
                return Ok(TopicDecisionItemPlan {
                    checklist_item_id: child.id().clone(),
                    title,
                    summary,
                    patch: None,
                });
            }
            return Err(TopicDecisionItemPlanError::DuplicateSiblingTitle {
                existing_node_id: child.id().clone(),
            });
        }
    }

    let checklist_item_id = next_topic_decision_item_id(graph, topic_id, &title)
        .ok_or(TopicDecisionItemPlanError::InvalidGeneratedId)?;
    let patch = SemanticGraphPatch::new(vec![
        SemanticGraphPatchOp::UpsertNode {
            node: SemanticNodeDraft::new_with_checklist_item_kind(
                checklist_item_id.clone(),
                title.clone(),
                summary.clone(),
                SemanticNodeFacets::topic_and_checklist_item(),
                Some(ChecklistItemStatus::Todo),
                Some(ChecklistItemKind::Decision),
            ),
            provenance: provenance.clone(),
        },
        SemanticGraphPatchOp::SetHardParent {
            child_id: checklist_item_id.clone(),
            parent_id: Some(topic_id.clone()),
            index: None,
            provenance: provenance.clone(),
        },
    ]);

    Ok(TopicDecisionItemPlan {
        checklist_item_id,
        title,
        summary,
        patch: Some(patch),
    })
}

pub(crate) fn decision_child_progress_patch(
    graph: &SemanticGraph,
    decisions: &ThreadedDecisionState,
    child_thread_id: &ConversationThreadId,
    child_turn_id: &ConversationTurnId,
    provenance: &MutationProvenance,
) -> Option<DecisionChildProgressPatch> {
    let record = decisions.active_record_for_child_thread(child_thread_id)?;
    if record.status() != ThreadedDecisionStatus::ActiveBranch {
        return None;
    }
    if record.branch_point_turn_id() == Some(child_turn_id) {
        return None;
    }
    if record.bootstrap_turn_id() == Some(child_turn_id) {
        return None;
    }

    let node = graph.node(record.checklist_item_id())?;
    if node.checklist_item_status() != Some(ChecklistItemStatus::Todo) {
        return None;
    }

    let checklist_item_id = record.checklist_item_id().clone();
    Some(DecisionChildProgressPatch {
        record_id: record.record_id().clone(),
        checklist_item_id: checklist_item_id.clone(),
        patch: SemanticGraphPatch::from_operation(SemanticGraphPatchOp::SetChecklistItemStatus {
            node_id: checklist_item_id,
            status: ChecklistItemStatus::InProgress,
            provenance: provenance.clone(),
        }),
    })
}

impl TopicDecisionItemPlan {
    pub(crate) fn reused_existing_item(&self) -> bool {
        self.patch.is_none()
    }
}

impl TopicDecisionItemPlanError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::MissingTopic => "That topic is no longer available.".to_string(),
            Self::NotTopic => "Start Decision requires a topic-capable graph row.".to_string(),
            Self::EmptyTitle => "Decision title must not be empty.".to_string(),
            Self::TitleTooLong {
                char_count,
                max_chars,
            } => format!("Decision title is {char_count} characters; the maximum is {max_chars}."),
            Self::SummaryTooLong {
                char_count,
                max_chars,
            } => {
                format!("Decision summary is {char_count} characters; the maximum is {max_chars}.")
            }
            Self::DuplicateSiblingTitle { existing_node_id } => format!(
                "Another child item already uses that title: {}.",
                existing_node_id.as_str()
            ),
            Self::InvalidGeneratedId => {
                "Beryl could not generate a valid semantic node id for the decision item."
                    .to_string()
            }
        }
    }
}

impl std::fmt::Display for TopicDecisionItemPlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for TopicDecisionItemPlanError {}

fn thread_ref_label() -> String {
    UNTITLED_DECISION_BRANCH_LABEL.to_string()
}

fn next_thread_ref_id(
    graph: &SemanticGraph,
    node_id: &SemanticNodeId,
    thread_id: &ConversationThreadId,
) -> ThreadRefId {
    let base = format!(
        "decision_thread_ref_{}_{}",
        sanitize_id_part(node_id.as_str()),
        sanitize_id_part(thread_id.as_str())
    );
    for suffix in 0usize.. {
        let candidate = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}_{suffix}")
        };
        let Ok(thread_ref_id) = ThreadRefId::new(candidate) else {
            continue;
        };
        if graph.thread_ref(&thread_ref_id).is_none() {
            return thread_ref_id;
        }
    }

    unreachable!("usize suffix space is non-empty")
}

fn next_topic_decision_item_id(
    graph: &SemanticGraph,
    topic_id: &SemanticNodeId,
    title: &str,
) -> Option<SemanticNodeId> {
    let title_part = sanitize_id_part(title);
    let base = format!(
        "decision_{}_{}",
        sanitize_id_part(topic_id.as_str()),
        title_part
    );
    for suffix in 0usize.. {
        let candidate = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}_{suffix}")
        };
        let Ok(node_id) = SemanticNodeId::new(candidate) else {
            continue;
        };
        if graph.node(&node_id).is_none() {
            return Some(node_id);
        }
    }

    None
}

fn normalize_inline_text(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_summary_text(value: String, topic_title: &str) -> String {
    let summary = value.trim().to_string();
    if summary.is_empty() {
        format!(
            "Decision item created from topic \"{}\".",
            normalize_inline_text(topic_title)
        )
    } else {
        summary
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
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
        "untitled".to_string()
    } else {
        sanitized
    }
}
