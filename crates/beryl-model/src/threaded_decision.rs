use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    conversation::{ConversationThreadId, ConversationTurnId, WorkspaceConversationState},
    provenance::{ElementProvenance, MutationProvenance},
    semantic_graph::{SemanticGraph, SemanticNodeId},
};

mod archive;
mod errors;
mod ids;
mod record;
mod state;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadedDecisionIdError {
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadedDecisionStateError {
    DuplicateRecordId {
        record_id: ThreadedDecisionRecordId,
    },
    ActiveBranchExists {
        checklist_item_id: SemanticNodeId,
        existing_record_id: ThreadedDecisionRecordId,
    },
    MissingRecord {
        record_id: ThreadedDecisionRecordId,
    },
    InvalidTransition {
        record_id: ThreadedDecisionRecordId,
        from: ThreadedDecisionStatus,
        to: ThreadedDecisionStatus,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadedDecisionRecordId(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadedDecisionOperationId(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadedDecisionStatus {
    QueuedBranch,
    ActiveBranch,
    PendingResolution,
    HandoffStarted,
    ChecklistUpdated,
    ArchivePending,
    ArchiveFailed,
    Closed,
    Superseded,
    Invalidated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadedDecisionOutcome {
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadedDecisionArchiveState {
    #[default]
    NotStarted,
    Pending,
    Archived,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadedDecisionArchiveStatus {
    #[serde(default)]
    state: ThreadedDecisionArchiveState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operation_id: Option<ThreadedDecisionOperationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failure_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    updated_at_millis: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadedDecisionInvalidationReason {
    MissingChecklistItem,
    MissingParentThread,
    MissingChildThread,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadedDecisionInvalidation {
    reason: ThreadedDecisionInvalidationReason,
    invalidated_at_millis: u64,
    provenance: MutationProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadedDecisionSupersession {
    superseded_by_record_id: ThreadedDecisionRecordId,
    superseded_at_millis: u64,
    provenance: MutationProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadedDecisionRecord {
    record_id: ThreadedDecisionRecordId,
    checklist_item_id: SemanticNodeId,
    parent_thread_id: ConversationThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    child_thread_id: Option<ConversationThreadId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bootstrap_turn_id: Option<ConversationTurnId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    branch_point_turn_id: Option<ConversationTurnId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handoff_turn_id: Option<ConversationTurnId>,
    status: ThreadedDecisionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    outcome: Option<ThreadedDecisionOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolution_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handoff_message: Option<String>,
    archive_status: ThreadedDecisionArchiveStatus,
    branch_operation_id: ThreadedDecisionOperationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolution_operation_id: Option<ThreadedDecisionOperationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    archive_operation_id: Option<ThreadedDecisionOperationId>,
    created_at_millis: u64,
    updated_at_millis: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolved_at_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    supersession: Option<ThreadedDecisionSupersession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    invalidation: Option<ThreadedDecisionInvalidation>,
    provenance: ElementProvenance,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadedDecisionState {
    #[serde(default)]
    records: Vec<ThreadedDecisionRecord>,
}

fn normalize_optional_text(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}
