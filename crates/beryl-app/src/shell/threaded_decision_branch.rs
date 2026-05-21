use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_backend::{
    DynamicToolCallRequest, DynamicToolCallResponse, ManagedBackendClientConnector, ThreadStatus,
    ThreadSummary,
};
use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{SemanticGraph, SemanticGraphPatch, SemanticNodeId},
    threaded_decision::{
        ThreadedDecisionOperationId, ThreadedDecisionRecord, ThreadedDecisionRecordId,
        ThreadedDecisionStateError, ThreadedDecisionStatus,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use gpui::{Context, Window};
use tracing::warn;

use crate::{
    GraphPatchWriteRequest, WorkspaceGraphRevision, WorkspaceGraphToolService,
    branch_bootstrap_core::{
        BranchBootstrapMessageInput, branch_bootstrap_message, start_branch_bootstrap_turn,
    },
    threaded_decision_branch_core::{
        DecisionBranchStartGate, QueuedDecisionBranchRunBlocker, QueuedDecisionBranchRunGate,
        TopicDecisionItemPlanError, decision_branch_graph_patch, decision_branch_start_blocker,
        default_topic_decision_title, queued_decision_branch_run_blocker, topic_decision_item_plan,
    },
    threaded_decision_child_thread::start_empty_decision_child_thread,
    threaded_decision_context::{
        ThreadedDecisionBootstrapContextInput, threaded_decision_bootstrap_context,
    },
    threaded_decision_dynamic_tools::{
        DecisionBranchToolItemResult, START_DECISION_BRANCH_TOOL, START_TOPIC_DECISION_TOOL,
        ThreadedDecisionDynamicToolRequest, TopicDecisionToolResult,
        decision_branch_tool_success_response, parse_beryl_threaded_decision_dynamic_tool_request,
        threaded_decision_tool_failure_response, topic_decision_tool_success_response,
    },
};

use super::{
    ShellState, ShellView, SurfaceNotice,
    execution_detail::{
        ExecutionItem, TurnExecutionRecord, TurnExecutionStatus, TurnNarrativeEntry,
    },
    graph::{GraphMutationCommitUpdate, GraphMutationFailureUpdate, GraphMutationUpdate},
    graph::{GraphOptimisticMutation, OptimisticGraphMutationId},
    thread_title::ThreadTitleCandidate,
    token_usage_snapshot,
    transcript_branch_core::register_transcript_branch_thread,
};

mod actions;
mod completion;
mod planning;
mod queue;
mod support;
mod worker;

use support::{
    dynamic_tool_provenance, next_decision_operation_id, next_decision_record_id,
    parent_context_source_for_turn, title_seed_for_turn_or_node, workspace_action_provenance,
};
use worker::spawn_decision_branch_start_worker;

const DECISION_BRANCH_ACTION: &str = "start_decision_branch";
const DECISION_PARENT_CONTEXT_SOURCE_MAX_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DecisionBranchActivation {
    SwitchTo,
    Background,
}

pub(super) struct QueuedDecisionBranchJob {
    workspace_id: BerylWorkspaceId,
    record_id: ThreadedDecisionRecordId,
    checklist_item_id: SemanticNodeId,
    topic_item_creation: Option<TopicDecisionItemSpec>,
    parent_thread_id: ConversationThreadId,
    parent_thread_title: Option<String>,
    parent_thread_summary: Option<String>,
    branch_point_turn_id: ConversationTurnId,
    parent_context_source: Option<String>,
    execution_target: WorkspaceId,
    title_seed: String,
    provenance: MutationProvenance,
    activation: DecisionBranchActivation,
}

#[derive(Clone)]
struct TopicDecisionItemSpec {
    topic_id: SemanticNodeId,
    title: String,
    summary: String,
}

pub(super) struct DecisionBranchStartTask {
    workspace_id: BerylWorkspaceId,
    record_id: ThreadedDecisionRecordId,
    parent_thread_id: ConversationThreadId,
    optimistic_mutation_id: OptimisticGraphMutationId,
    receiver: Receiver<DecisionBranchStartUpdate>,
}

enum DecisionBranchStartUpdate {
    Finished(DecisionBranchStartOutcome),
}

enum DecisionBranchStartOutcome {
    Started {
        job: QueuedDecisionBranchJob,
        thread_summary: ThreadSummary,
        bootstrap_turn_id: Option<ConversationTurnId>,
        graph_patch: SemanticGraphPatch,
        graph_revision: WorkspaceGraphRevision,
        optimistic_mutation_id: OptimisticGraphMutationId,
    },
    Failed {
        job: QueuedDecisionBranchJob,
        message: String,
        graph_failure: GraphMutationFailureUpdate,
    },
}

struct DecisionBranchPoint {
    parent_thread_id: ConversationThreadId,
    parent_thread_title: Option<String>,
    parent_thread_summary: Option<String>,
    branch_point_turn_id: ConversationTurnId,
    parent_context_source: Option<String>,
    execution_target: WorkspaceId,
    title_seed: String,
}

struct DecisionBranchPointResolution {
    title_seed: String,
    parent_context_source: Option<String>,
}

impl DecisionBranchStartTask {
    fn new(
        workspace_id: BerylWorkspaceId,
        record_id: ThreadedDecisionRecordId,
        parent_thread_id: ConversationThreadId,
        optimistic_mutation_id: OptimisticGraphMutationId,
        receiver: Receiver<DecisionBranchStartUpdate>,
    ) -> Self {
        Self {
            workspace_id,
            record_id,
            parent_thread_id,
            optimistic_mutation_id,
            receiver,
        }
    }

    fn try_recv(&self) -> Result<DecisionBranchStartUpdate, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }

    fn disconnected_failure(&self, message: &'static str) -> GraphMutationFailureUpdate {
        GraphMutationFailureUpdate::new(self.workspace_id.clone(), message)
            .with_optimistic_mutation_id(self.optimistic_mutation_id)
    }
}
