use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::Path,
};

use beryl_backend::{
    AgentMessageItem, ProtocolPhase, ThreadItem, ThreadReadMetadata, ThreadSessionMetadata,
    ThreadSummary, ToolActivityCollabAgentSpawnMetadata, ToolActivityEvent,
    ToolActivityFileChangeSummary, ToolActivityLifecycle, ToolActivitySource, TurnStatus,
    TurnStreamEvent,
};
use beryl_model::workspace::{RuntimeMode, WorkspaceId};
use gpui::SharedString;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::activity_lifecycle_diagnostics::{
    ActivityLifecycleDiagnosticInput, ActivityLifecycleDiagnosticObserver,
    ActivityLifecycleDiagnosticSnapshot, ActivityLifecycleDiagnostics,
};
use crate::activity_presentation_diagnostics::ActivityProjectionDiagnosticState;

const MULTI_AGENT_V2_DIAGNOSTIC_RECORD_LIMIT: usize = 64;
const MULTI_AGENT_V2_DIAGNOSTIC_IDENTITY_BYTE_LIMIT: usize = 64 * 1024;
const MULTI_AGENT_V2_DIAGNOSTIC_IDENTITY_FIELD_BYTE_LIMIT: usize = 512;

#[derive(Clone, Debug)]
pub(super) struct ToolActivityProjection {
    records: Vec<ToolActivityRecord>,
    rows: Vec<ToolActivityRow>,
    agent_labels_by_thread: HashMap<String, String>,
    runtime_metadata_by_subagent_thread: HashMap<String, SubagentRuntimeMetadata>,
    multi_agent_v2_child_threads: HashSet<String>,
    multi_agent_v2_agent_path_by_child: HashMap<String, String>,
    parent_thread_by_child: HashMap<String, String>,
    root_turn_by_child_thread: HashMap<String, ToolActivityRootTurnKey>,
    visible_row_indexes_by_thread: HashMap<String, Vec<usize>>,
    last_selected_thread_id: Option<String>,
    next_start_order: u64,
    lifecycle_diagnostics: ActivityLifecycleDiagnostics,
    projection_revision: u64,
    newest_lifecycle_sequence_in_projection: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ToolActivityRow {
    key: ToolActivityKey,
    stable_identity: SharedString,
    pub(super) agent_label: String,
    pub(super) tool_display_value: String,
    pub(super) status: ToolActivityRowStatus,
}

impl ToolActivityRow {
    pub(super) fn thread_id(&self) -> &str {
        self.key.thread_id.as_str()
    }

    pub(super) fn turn_id(&self) -> &str {
        self.key.turn_id.as_str()
    }

    pub(super) fn item_id(&self) -> &str {
        self.key.item_id.as_str()
    }

    pub(super) fn stable_identity(&self) -> &SharedString {
        &self.stable_identity
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolActivityRowStatus {
    Running,
    FinishedOk,
    FinishedError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolActivityRecord {
    key: ToolActivityKey,
    activity_subject_thread_id: Option<String>,
    source: ToolActivityRecordSource,
    explicit_agent_label: Option<String>,
    tool_display_value: String,
    status: ToolActivityRowStatus,
    start_order: u64,
    active_epoch: Option<u64>,
    completion_order: Option<u64>,
    reasoning_summary_parts: Vec<String>,
    receiver_thread_ids: Vec<String>,
    multi_agent_v2_lifecycle_kind: Option<MultiAgentV2LifecycleKind>,
    multi_agent_v2_agent_path: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MultiAgentV2LifecycleKind {
    Started,
    Interacted,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MultiAgentV2ActivityDiagnostic {
    pub(super) parent_thread_id: String,
    pub(super) parent_turn_id: String,
    pub(super) parent_item_id: String,
    pub(super) child_thread_id: Option<String>,
    pub(super) lifecycle_kind: &'static str,
    pub(super) row_status: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct MultiAgentV2ActivityDiagnosticSample {
    pub(super) records: Vec<MultiAgentV2ActivityDiagnostic>,
    pub(super) truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolActivityRecordSource {
    Backend(ToolActivitySource),
    SubagentHandoff,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolActivityKey {
    thread_id: String,
    turn_id: String,
    item_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolActivityRootTurnKey {
    thread_id: String,
    turn_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SubagentRuntimeMetadata {
    model: Option<String>,
    reasoning_effort: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ReceiverThreadOwnershipChange {
    changed: bool,
    requires_row_rebuild: bool,
}

#[derive(Clone, Debug, Default)]
struct ChargedMultiAgentV2PresentationIdentity {
    child_threads: HashSet<String>,
    agent_paths: HashSet<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ToolActivityRetainedCounts {
    pub(super) records: usize,
    pub(super) rows: usize,
    pub(super) label_count: usize,
    pub(super) label_payload_bytes: usize,
    pub(super) reasoning_summary_parts: usize,
    pub(super) reasoning_summary_bytes: usize,
    pub(super) subagent_metadata_count: usize,
    pub(super) subagent_metadata_bytes: usize,
    pub(super) multi_agent_v2_child_threads: usize,
    pub(super) multi_agent_v2_child_thread_bytes: usize,
    pub(super) multi_agent_v2_path_associations: usize,
    pub(super) multi_agent_v2_path_association_bytes: usize,
    pub(super) parent_thread_links: usize,
    pub(super) parent_thread_link_bytes: usize,
    pub(super) root_turn_links: usize,
    pub(super) root_turn_link_bytes: usize,
    pub(super) visible_thread_index_maps: usize,
    pub(super) visible_thread_indexes: usize,
    pub(super) visible_thread_index_key_bytes: usize,
    pub(super) visible_thread_index_bytes: usize,
    pub(super) record_payload_bytes: usize,
    pub(super) row_payload_bytes: usize,
    pub(super) payload_bytes: usize,
}

impl Default for ToolActivityProjection {
    fn default() -> Self {
        Self::with_activity_lifecycle_diagnostic_observer(None)
    }
}

impl ToolActivityProjection {
    pub(super) fn with_activity_lifecycle_diagnostic_observer(
        observer: Option<ActivityLifecycleDiagnosticObserver>,
    ) -> Self {
        Self {
            records: Vec::new(),
            rows: Vec::new(),
            agent_labels_by_thread: HashMap::new(),
            runtime_metadata_by_subagent_thread: HashMap::new(),
            multi_agent_v2_child_threads: HashSet::new(),
            multi_agent_v2_agent_path_by_child: HashMap::new(),
            parent_thread_by_child: HashMap::new(),
            root_turn_by_child_thread: HashMap::new(),
            visible_row_indexes_by_thread: HashMap::new(),
            last_selected_thread_id: None,
            next_start_order: 0,
            lifecycle_diagnostics: ActivityLifecycleDiagnostics::with_observer(observer),
            projection_revision: 0,
            newest_lifecycle_sequence_in_projection: None,
        }
    }

    pub(super) fn lifecycle_diagnostic_snapshot(&self) -> ActivityLifecycleDiagnosticSnapshot {
        self.lifecycle_diagnostics.snapshot()
    }

    pub(super) fn presentation_diagnostic_state(&self) -> ActivityProjectionDiagnosticState {
        let mut state = ActivityProjectionDiagnosticState {
            revision: self.projection_revision,
            newest_lifecycle_sequence: self.newest_lifecycle_sequence_in_projection,
            total_row_count: self.rows.len(),
            running_row_count: 0,
            finished_ok_row_count: 0,
            finished_error_row_count: 0,
        };
        for row in &self.rows {
            match row.status {
                ToolActivityRowStatus::Running => state.running_row_count += 1,
                ToolActivityRowStatus::FinishedOk => state.finished_ok_row_count += 1,
                ToolActivityRowStatus::FinishedError => state.finished_error_row_count += 1,
            }
        }
        state
    }

    #[allow(dead_code)]
    pub(super) fn rows(&self) -> &[ToolActivityRow] {
        &self.rows
    }

    pub(super) fn rows_for_selected_thread(
        &self,
        selected_thread_id: Option<&str>,
    ) -> Vec<&ToolActivityRow> {
        self.visible_row_indexes_for_selected_thread(selected_thread_id)
            .map(|row_indexes| {
                row_indexes
                    .iter()
                    .filter_map(|row_index| self.rows.get(*row_index))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn row_count_for_selected_thread(&self, selected_thread_id: Option<&str>) -> usize {
        self.visible_row_indexes_for_selected_thread(selected_thread_id)
            .map_or(0, <[usize]>::len)
    }

    pub(super) fn multi_agent_v2_activity_diagnostic_sample(
        &self,
        selected_thread_id: Option<&str>,
    ) -> MultiAgentV2ActivityDiagnosticSample {
        let Some(selected_thread_id) = selected_thread_id.and_then(non_blank_str) else {
            return MultiAgentV2ActivityDiagnosticSample::default();
        };

        let mut records = self
            .records
            .iter()
            .filter(|record| {
                record.multi_agent_v2_lifecycle_kind.is_some()
                    && self.record_is_visible_for_thread(record, selected_thread_id)
            })
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.status
                .sort_rank()
                .cmp(&right.status.sort_rank())
                .then_with(|| right.completion_order.cmp(&left.completion_order))
                .then_with(|| left.key.thread_id.cmp(&right.key.thread_id))
                .then_with(|| left.key.turn_id.cmp(&right.key.turn_id))
                .then_with(|| left.key.item_id.cmp(&right.key.item_id))
        });

        let mut sample = MultiAgentV2ActivityDiagnosticSample::default();
        let mut identity_bytes = 0usize;
        for record in records {
            if sample.records.len() == MULTI_AGENT_V2_DIAGNOSTIC_RECORD_LIMIT {
                sample.truncated = true;
                break;
            }
            let Some(kind) = record.multi_agent_v2_lifecycle_kind else {
                continue;
            };
            let identities = [
                Some(record.key.thread_id.as_str()),
                Some(record.key.turn_id.as_str()),
                Some(record.key.item_id.as_str()),
                record.activity_subject_thread_id.as_deref(),
            ];
            if identities.iter().flatten().any(|identity| {
                identity.trim().is_empty()
                    || identity.len() > MULTI_AGENT_V2_DIAGNOSTIC_IDENTITY_FIELD_BYTE_LIMIT
            }) || identities[..3].iter().any(Option::is_none)
            {
                sample.truncated = true;
                continue;
            }
            let record_identity_bytes = identities
                .iter()
                .flatten()
                .map(|identity| identity.len())
                .sum::<usize>();
            if identity_bytes.saturating_add(record_identity_bytes)
                > MULTI_AGENT_V2_DIAGNOSTIC_IDENTITY_BYTE_LIMIT
            {
                sample.truncated = true;
                break;
            }
            identity_bytes = identity_bytes.saturating_add(record_identity_bytes);
            sample.records.push(MultiAgentV2ActivityDiagnostic {
                parent_thread_id: record.key.thread_id.clone(),
                parent_turn_id: record.key.turn_id.clone(),
                parent_item_id: record.key.item_id.clone(),
                child_thread_id: record.activity_subject_thread_id.clone(),
                lifecycle_kind: kind.diagnostic_label(),
                row_status: record.status.diagnostic_label(),
            });
        }
        sample
    }

    pub(super) fn retained_counts(&self) -> ToolActivityRetainedCounts {
        let reasoning_summary_parts = self
            .records
            .iter()
            .map(|record| record.reasoning_summary_parts.len())
            .sum::<usize>();
        let reasoning_summary_bytes = self
            .records
            .iter()
            .flat_map(|record| record.reasoning_summary_parts.iter())
            .map(String::len)
            .sum::<usize>();
        let record_payload_bytes = self
            .records
            .iter()
            .map(|record| {
                record.key.thread_id.len()
                    + record.key.turn_id.len()
                    + record.key.item_id.len()
                    + record
                        .activity_subject_thread_id
                        .as_ref()
                        .map_or(0, String::len)
                    + record.explicit_agent_label.as_ref().map_or(0, String::len)
                    + record.tool_display_value.len()
                    + record
                        .reasoning_summary_parts
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
                    + record
                        .receiver_thread_ids
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
                    + record
                        .multi_agent_v2_agent_path
                        .as_ref()
                        .map_or(0, String::len)
            })
            .sum::<usize>();
        let row_payload_bytes = self
            .rows
            .iter()
            .map(|row| {
                row.key.thread_id.len()
                    + row.key.turn_id.len()
                    + row.key.item_id.len()
                    + row.agent_label.len()
                    + row.tool_display_value.len()
            })
            .sum::<usize>();
        let label_payload_bytes = self
            .agent_labels_by_thread
            .iter()
            .map(|(thread_id, label)| thread_id.len() + label.len())
            .sum::<usize>();
        let metadata_payload_bytes = self
            .runtime_metadata_by_subagent_thread
            .iter()
            .map(|(thread_id, metadata)| {
                thread_id.len()
                    + metadata.model.as_ref().map_or(0, String::len)
                    + metadata.reasoning_effort.as_ref().map_or(0, String::len)
            })
            .sum::<usize>();
        let multi_agent_v2_child_thread_bytes = self
            .multi_agent_v2_child_threads
            .iter()
            .map(String::len)
            .sum::<usize>();
        let multi_agent_v2_path_association_bytes = self
            .multi_agent_v2_agent_path_by_child
            .iter()
            .map(|(thread_id, agent_path)| thread_id.len() + agent_path.len())
            .sum::<usize>();
        let parent_payload_bytes = self
            .parent_thread_by_child
            .iter()
            .map(|(child, parent)| child.len() + parent.len())
            .sum::<usize>();
        let root_turn_payload_bytes = self
            .root_turn_by_child_thread
            .iter()
            .map(|(child, root_turn)| {
                child.len() + root_turn.thread_id.len() + root_turn.turn_id.len()
            })
            .sum::<usize>();
        let visible_thread_index_key_bytes = self
            .visible_row_indexes_by_thread
            .keys()
            .map(String::len)
            .sum::<usize>();
        let visible_thread_indexes = self
            .visible_row_indexes_by_thread
            .values()
            .map(Vec::len)
            .sum::<usize>();
        let visible_thread_index_bytes =
            visible_thread_indexes.saturating_mul(std::mem::size_of::<usize>());
        let selected_thread_id_bytes = self.last_selected_thread_id.as_ref().map_or(0, String::len);

        ToolActivityRetainedCounts {
            records: self.records.len(),
            rows: self.rows.len(),
            label_count: self.agent_labels_by_thread.len(),
            label_payload_bytes,
            reasoning_summary_parts,
            reasoning_summary_bytes,
            subagent_metadata_count: self.runtime_metadata_by_subagent_thread.len(),
            subagent_metadata_bytes: metadata_payload_bytes,
            multi_agent_v2_child_threads: self.multi_agent_v2_child_threads.len(),
            multi_agent_v2_child_thread_bytes,
            multi_agent_v2_path_associations: self.multi_agent_v2_agent_path_by_child.len(),
            multi_agent_v2_path_association_bytes,
            parent_thread_links: self.parent_thread_by_child.len(),
            parent_thread_link_bytes: parent_payload_bytes,
            root_turn_links: self.root_turn_by_child_thread.len(),
            root_turn_link_bytes: root_turn_payload_bytes,
            visible_thread_index_maps: self.visible_row_indexes_by_thread.len(),
            visible_thread_indexes,
            visible_thread_index_key_bytes,
            visible_thread_index_bytes,
            record_payload_bytes,
            row_payload_bytes,
            payload_bytes: record_payload_bytes
                .saturating_add(row_payload_bytes)
                .saturating_add(label_payload_bytes)
                .saturating_add(metadata_payload_bytes)
                .saturating_add(multi_agent_v2_child_thread_bytes)
                .saturating_add(multi_agent_v2_path_association_bytes)
                .saturating_add(parent_payload_bytes)
                .saturating_add(root_turn_payload_bytes)
                .saturating_add(visible_thread_index_key_bytes)
                .saturating_add(visible_thread_index_bytes)
                .saturating_add(selected_thread_id_bytes),
        }
    }

    pub(super) fn rows_for_selected_thread_window(
        &self,
        selected_thread_id: Option<&str>,
        range: Range<usize>,
    ) -> Vec<(usize, &ToolActivityRow)> {
        let Some(row_indexes) = self.visible_row_indexes_for_selected_thread(selected_thread_id)
        else {
            return Vec::new();
        };
        let start = range.start.min(row_indexes.len());
        let end = range.end.min(row_indexes.len()).max(start);

        row_indexes[start..end]
            .iter()
            .enumerate()
            .filter_map(|(offset, row_index)| {
                self.rows
                    .get(*row_index)
                    .map(|row| (start.saturating_add(offset), row))
            })
            .collect()
    }

    pub(super) fn set_selected_thread_id(&mut self, selected_thread_id: Option<&str>) -> bool {
        let selected_thread_id = selected_thread_id
            .and_then(non_blank_str)
            .map(str::to_string);
        if self.last_selected_thread_id == selected_thread_id {
            return false;
        }
        self.last_selected_thread_id = selected_thread_id;
        self.rebuild_rows();
        true
    }

    #[allow(dead_code)]
    pub(super) fn apply_stream_event(
        &mut self,
        event: &TurnStreamEvent,
        agent_label: Option<String>,
    ) -> bool {
        self.apply_stream_event_with_execution_target(event, agent_label, None)
    }

    pub(super) fn apply_stream_event_with_execution_target(
        &mut self,
        event: &TurnStreamEvent,
        agent_label: Option<String>,
        execution_target: Option<&WorkspaceId>,
    ) -> bool {
        if let Some(activity) = event.activity() {
            return self.apply_tool_activity(activity, agent_label, execution_target);
        }

        match event {
            TurnStreamEvent::ItemCompleted {
                thread_id,
                turn_id,
                item: ThreadItem::AgentMessage(item),
            } => self.apply_subagent_handoff_activity(thread_id, turn_id, item),
            TurnStreamEvent::ItemCompleted {
                item:
                    ThreadItem::UserMessage(_)
                    | ThreadItem::Reasoning(_)
                    | ThreadItem::CommandExecution(_)
                    | ThreadItem::FileChange(_)
                    | ThreadItem::ImageGeneration(_)
                    | ThreadItem::Generic(_),
                ..
            } => false,
            TurnStreamEvent::ThreadStarted { thread } => {
                let changed = self.note_thread_summary_agent_labels(thread);
                if changed {
                    self.rebuild_rows();
                }
                changed
            }
            TurnStreamEvent::AgentLabelUpdated { .. } => false,
            TurnStreamEvent::TurnCompleted { thread_id, turn } => {
                match final_status_from_turn_status(turn.status) {
                    Some(status) => {
                        let affected =
                            self.finish_running_for_turn_count(thread_id, &turn.id, status);
                        self.record_fallback(
                            "turn_completed",
                            Some(thread_id),
                            Some(turn.id.as_str()),
                            status,
                            affected,
                        );
                        affected > 0
                    }
                    None => false,
                }
            }
            TurnStreamEvent::ThreadClosed { thread_id } => {
                self.finish_thread_fallback("thread_closed", thread_id)
            }
            TurnStreamEvent::ThreadArchived { thread_id } => {
                self.finish_thread_fallback("thread_archived", thread_id)
            }
            TurnStreamEvent::ThreadDeleted { thread_id } => {
                self.finish_thread_fallback("thread_deleted", thread_id)
            }
            TurnStreamEvent::ThreadUnarchived { .. } => false,
            TurnStreamEvent::ProtocolError { .. } => {
                let status = ToolActivityRowStatus::FinishedError;
                let affected = self.finish_all_running_count(status);
                self.record_fallback("protocol_error", None, None, status, affected);
                affected > 0
            }
            TurnStreamEvent::TurnStarted { .. }
            | TurnStreamEvent::ThreadStatusChanged { .. }
            | TurnStreamEvent::ItemStarted { .. }
            | TurnStreamEvent::AgentMessageDelta { .. }
            | TurnStreamEvent::ReasoningSummaryPartAdded { .. }
            | TurnStreamEvent::ReasoningSummaryTextDelta { .. }
            | TurnStreamEvent::ReasoningTextDelta { .. }
            | TurnStreamEvent::CommandExecutionOutputDelta { .. }
            | TurnStreamEvent::FileChangeOutputDelta { .. }
            | TurnStreamEvent::TokenUsageUpdated { .. }
            | TurnStreamEvent::AccountRateLimitsUpdated { .. }
            | TurnStreamEvent::ThreadNameUpdated { .. }
            | TurnStreamEvent::TurnError { .. }
            | TurnStreamEvent::ApprovalRequested(..)
            | TurnStreamEvent::DynamicToolCallRequested(..) => false,
        }
    }

    pub(super) fn apply_thread_summary_agent_labels<'a>(
        &mut self,
        threads: impl IntoIterator<Item = &'a ThreadSummary>,
    ) -> bool {
        let mut changed = false;
        for thread in threads {
            changed |= self.note_thread_summary_agent_labels(thread);
        }
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    pub(super) fn apply_thread_read_metadata<'a>(
        &mut self,
        metadata: impl IntoIterator<Item = &'a ThreadReadMetadata>,
    ) -> bool {
        let mut changed = false;
        for metadata in metadata {
            changed |= self.note_thread_summary_agent_labels(&metadata.thread);
            if self.is_observed_subagent_thread(&metadata.thread.id) {
                changed |= self.note_subagent_runtime_metadata(
                    metadata.thread.id.as_str(),
                    &metadata.session_metadata,
                );
            }
        }
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    #[allow(dead_code)]
    pub(super) fn clear_thread(&mut self, thread_id: &str) -> bool {
        let before = self.records.len();
        self.records.retain(|row| row.key.thread_id != thread_id);
        self.rebuild_rows_if_len_changed(before)
    }

    #[allow(dead_code)]
    pub(super) fn clear_all(&mut self) -> bool {
        let changed = !self.records.is_empty()
            || !self.agent_labels_by_thread.is_empty()
            || !self.runtime_metadata_by_subagent_thread.is_empty()
            || !self.multi_agent_v2_child_threads.is_empty()
            || !self.multi_agent_v2_agent_path_by_child.is_empty()
            || !self.parent_thread_by_child.is_empty()
            || !self.root_turn_by_child_thread.is_empty()
            || !self.visible_row_indexes_by_thread.is_empty()
            || self.last_selected_thread_id.is_some()
            || self.projection_revision != 0
            || self.newest_lifecycle_sequence_in_projection.is_some();
        self.records.clear();
        self.rows.clear();
        self.agent_labels_by_thread.clear();
        self.runtime_metadata_by_subagent_thread.clear();
        self.multi_agent_v2_child_threads.clear();
        self.multi_agent_v2_agent_path_by_child.clear();
        self.parent_thread_by_child.clear();
        self.root_turn_by_child_thread.clear();
        self.visible_row_indexes_by_thread.clear();
        self.last_selected_thread_id.take();
        self.lifecycle_diagnostics.clear();
        self.projection_revision = 0;
        self.newest_lifecycle_sequence_in_projection = None;
        changed
    }

    pub(super) fn finish_running_for_thread(
        &mut self,
        thread_id: &str,
        status: ToolActivityRowStatus,
    ) -> bool {
        self.finish_running_for_thread_count(thread_id, status) > 0
    }

    pub(super) fn finish_running_for_thread_stream_failure(&mut self, thread_id: &str) -> bool {
        let status = ToolActivityRowStatus::FinishedError;
        let affected = self.finish_running_for_thread_count(thread_id, status);
        self.lifecycle_diagnostics
            .record(ActivityLifecycleDiagnosticInput {
                stage: "stream_failure",
                category: "stream_failure",
                kind: "local_turn_failure",
                thread_id: Some(thread_id),
                turn_id: None,
                item_id: None,
                item_type: None,
                item_status: None,
                projection_outcome: fallback_outcome(affected),
                before_row_status: (affected > 0).then_some("running"),
                after_row_status: (affected > 0).then_some(status.diagnostic_label()),
                affected_row_count: affected,
            });
        if affected > 0 {
            self.refresh_projection_lifecycle_sequence();
        }
        affected > 0
    }

    fn finish_thread_fallback(&mut self, kind: &'static str, thread_id: &str) -> bool {
        let status = ToolActivityRowStatus::FinishedOk;
        let affected = self.finish_running_for_thread_count(thread_id, status);
        self.record_fallback(kind, Some(thread_id), None, status, affected);
        affected > 0
    }

    fn record_fallback(
        &mut self,
        kind: &'static str,
        thread_id: Option<&str>,
        turn_id: Option<&str>,
        status: ToolActivityRowStatus,
        affected: usize,
    ) {
        self.lifecycle_diagnostics
            .record(ActivityLifecycleDiagnosticInput {
                stage: "fallback",
                category: "fallback",
                kind,
                thread_id,
                turn_id,
                item_id: None,
                item_type: None,
                item_status: None,
                projection_outcome: fallback_outcome(affected),
                before_row_status: (affected > 0).then_some("running"),
                after_row_status: (affected > 0).then_some(status.diagnostic_label()),
                affected_row_count: affected,
            });
        if affected > 0 {
            self.refresh_projection_lifecycle_sequence();
        }
    }

    fn finish_running_for_thread_count(
        &mut self,
        thread_id: &str,
        status: ToolActivityRowStatus,
    ) -> usize {
        let completion_order = self
            .records
            .iter()
            .any(|record| {
                record.key.thread_id == thread_id && record.status == ToolActivityRowStatus::Running
            })
            .then(|| self.next_start_order());
        let mut affected = 0usize;
        for record in &mut self.records {
            if record.key.thread_id == thread_id && record.status == ToolActivityRowStatus::Running
            {
                record.status = status;
                record.completion_order = completion_order;
                affected = affected.saturating_add(1);
            }
        }
        if affected > 0 {
            self.rebuild_rows();
        }
        affected
    }

    fn apply_tool_activity(
        &mut self,
        activity: ToolActivityEvent,
        agent_label: Option<String>,
        execution_target: Option<&WorkspaceId>,
    ) -> bool {
        let diagnostic_thread_id = activity.thread_id.clone();
        let diagnostic_turn_id = activity.turn_id.clone();
        let diagnostic_item_id = activity.item_id.clone();
        let diagnostic_item_type = activity.item_type.clone();
        let diagnostic_item_status = activity.raw_item_status.clone();
        let lifecycle = activity.lifecycle;
        let key = ToolActivityKey::from_activity(&activity);
        let before_status = self
            .records
            .iter()
            .find(|existing| existing.key == key)
            .map(|record| record.status);
        let ownership_changed = self.apply_receiver_thread_ownership_updates(&activity);
        let explicit_agent_label = explicit_agent_label_for_activity(&activity, agent_label);
        let activity_changed = match lifecycle {
            ToolActivityLifecycle::Started => {
                self.start_activity(key, activity, explicit_agent_label, execution_target)
            }
            ToolActivityLifecycle::Updated => {
                self.update_activity(key, activity, explicit_agent_label, execution_target)
            }
            ToolActivityLifecycle::Completed => {
                let status = final_status_from_item_status(activity.raw_item_status.as_deref());
                self.finish_or_insert_completed(
                    key,
                    activity,
                    explicit_agent_label,
                    status,
                    execution_target,
                )
            }
        };
        let after_status = self
            .records
            .iter()
            .find(|existing| {
                existing.key.thread_id == diagnostic_thread_id
                    && existing.key.turn_id == diagnostic_turn_id
                    && existing.key.item_id == diagnostic_item_id
            })
            .map(|record| record.status);
        self.lifecycle_diagnostics
            .record(ActivityLifecycleDiagnosticInput {
                stage: "activity_ingress",
                category: "lifecycle",
                kind: lifecycle_diagnostic_kind(lifecycle),
                thread_id: Some(diagnostic_thread_id.as_str()),
                turn_id: Some(diagnostic_turn_id.as_str()),
                item_id: Some(diagnostic_item_id.as_str()),
                item_type: Some(diagnostic_item_type.as_str()),
                item_status: diagnostic_item_status.as_deref(),
                projection_outcome: lifecycle_projection_outcome(lifecycle, before_status),
                before_row_status: before_status.map(ToolActivityRowStatus::diagnostic_label),
                after_row_status: after_status.map(ToolActivityRowStatus::diagnostic_label),
                affected_row_count: usize::from(after_status.is_some()),
            });
        if activity_changed {
            self.refresh_projection_lifecycle_sequence();
        }
        if ownership_changed.changed && !activity_changed {
            if ownership_changed.requires_row_rebuild {
                self.rebuild_rows();
            } else {
                self.prune_derived_state();
                self.rebuild_visible_row_indexes();
            }
        }
        ownership_changed.changed || activity_changed
    }

    fn start_activity(
        &mut self,
        key: ToolActivityKey,
        activity: ToolActivityEvent,
        explicit_agent_label: Option<String>,
        execution_target: Option<&WorkspaceId>,
    ) -> bool {
        if let Some(index) = self.records.iter().position(|existing| existing.key == key) {
            let reactivated = self.records[index].status != ToolActivityRowStatus::Running;
            let reactivation_order = reactivated.then(|| {
                let start_order = self.next_start_order();
                (
                    start_order,
                    self.active_epoch_for_thread(activity.thread_id.as_str(), start_order),
                )
            });
            let existing = &mut self.records[index];
            let mut changed = false;
            let source = ToolActivityRecordSource::from(activity.source);
            if existing.source != source {
                existing.source = source;
                changed = true;
            }
            if existing.status != ToolActivityRowStatus::Running {
                existing.status = ToolActivityRowStatus::Running;
                let (start_order, active_epoch) =
                    reactivation_order.expect("reactivated records receive an epoch");
                existing.start_order = start_order;
                existing.active_epoch = Some(active_epoch);
                existing.completion_order = None;
                changed = true;
            }
            changed |= merge_activity_subject_thread_id(existing, &activity);
            changed |= merge_receiver_thread_ids(existing, &activity);
            changed |= merge_multi_agent_v2_lifecycle_kind(existing, &activity);
            changed |= merge_multi_agent_v2_agent_path(existing, &activity);
            changed |= apply_reasoning_summary_detail(existing, &activity);
            let display_value =
                activity_display_value_for_record(existing, &activity, execution_target);
            if existing.tool_display_value != display_value {
                existing.tool_display_value = display_value;
                changed = true;
            }
            if explicit_agent_label.is_some()
                && existing.explicit_agent_label != explicit_agent_label
            {
                existing.explicit_agent_label = explicit_agent_label;
                changed = true;
            }
            if changed {
                self.rebuild_rows();
            }
            return changed;
        }

        let record = self.new_record(
            key,
            activity,
            explicit_agent_label,
            ToolActivityRowStatus::Running,
            execution_target,
        );
        self.records.push(record);
        self.rebuild_rows();
        true
    }

    fn update_activity(
        &mut self,
        key: ToolActivityKey,
        activity: ToolActivityEvent,
        explicit_agent_label: Option<String>,
        execution_target: Option<&WorkspaceId>,
    ) -> bool {
        if let Some(existing) = self.records.iter_mut().find(|existing| existing.key == key) {
            let mut changed = false;
            let source = ToolActivityRecordSource::from(activity.source);
            if existing.source != source {
                existing.source = source;
                changed = true;
            }
            changed |= merge_activity_subject_thread_id(existing, &activity);
            changed |= merge_receiver_thread_ids(existing, &activity);
            changed |= merge_multi_agent_v2_lifecycle_kind(existing, &activity);
            changed |= merge_multi_agent_v2_agent_path(existing, &activity);
            changed |= apply_reasoning_summary_detail(existing, &activity);
            let display_value =
                activity_display_value_for_record(existing, &activity, execution_target);
            if existing.tool_display_value != display_value {
                existing.tool_display_value = display_value;
                changed = true;
            }
            if explicit_agent_label.is_some()
                && existing.explicit_agent_label != explicit_agent_label
            {
                existing.explicit_agent_label = explicit_agent_label;
                changed = true;
            }
            if changed {
                self.rebuild_rows();
            }
            return changed;
        }

        let record = self.new_record(
            key,
            activity,
            explicit_agent_label,
            ToolActivityRowStatus::Running,
            execution_target,
        );
        self.records.push(record);
        self.rebuild_rows();
        true
    }

    fn finish_running_for_turn_count(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        status: ToolActivityRowStatus,
    ) -> usize {
        let completion_order = self
            .records
            .iter()
            .any(|record| {
                record.key.thread_id == thread_id
                    && record.key.turn_id == turn_id
                    && record.status == ToolActivityRowStatus::Running
            })
            .then(|| self.next_start_order());
        let mut affected = 0usize;
        for record in &mut self.records {
            if record.key.thread_id == thread_id
                && record.key.turn_id == turn_id
                && record.status == ToolActivityRowStatus::Running
            {
                record.status = status;
                record.completion_order = completion_order;
                affected = affected.saturating_add(1);
            }
        }
        if affected > 0 {
            self.rebuild_rows();
        }
        affected
    }

    fn finish_all_running_count(&mut self, status: ToolActivityRowStatus) -> usize {
        let completion_order = self
            .records
            .iter()
            .any(|record| record.status == ToolActivityRowStatus::Running)
            .then(|| self.next_start_order());
        let mut affected = 0usize;
        for record in &mut self.records {
            if record.status == ToolActivityRowStatus::Running {
                record.status = status;
                record.completion_order = completion_order;
                affected = affected.saturating_add(1);
            }
        }
        if affected > 0 {
            self.rebuild_rows();
        }
        affected
    }

    fn finish_or_insert_completed(
        &mut self,
        key: ToolActivityKey,
        activity: ToolActivityEvent,
        explicit_agent_label: Option<String>,
        status: ToolActivityRowStatus,
        execution_target: Option<&WorkspaceId>,
    ) -> bool {
        if let Some(index) = self.records.iter().position(|existing| existing.key == key) {
            let completion_order = (self.records[index].status == ToolActivityRowStatus::Running)
                .then(|| self.next_start_order());
            let existing = &mut self.records[index];
            let mut changed = false;
            let source = ToolActivityRecordSource::from(activity.source);
            if existing.source != source {
                existing.source = source;
                changed = true;
            }
            if existing.status != status {
                existing.status = status;
                existing.completion_order = completion_order.or(existing.completion_order);
                changed = true;
            }
            changed |= merge_activity_subject_thread_id(existing, &activity);
            changed |= merge_receiver_thread_ids(existing, &activity);
            changed |= merge_multi_agent_v2_lifecycle_kind(existing, &activity);
            changed |= merge_multi_agent_v2_agent_path(existing, &activity);
            changed |= apply_reasoning_summary_detail(existing, &activity);
            let tool_display_value =
                activity_display_value_for_record(existing, &activity, execution_target);
            if existing.tool_display_value != tool_display_value {
                existing.tool_display_value = tool_display_value;
                changed = true;
            }
            if explicit_agent_label.is_some()
                && existing.explicit_agent_label != explicit_agent_label
            {
                existing.explicit_agent_label = explicit_agent_label;
                changed = true;
            }
            if changed {
                self.rebuild_rows();
            }
            return changed;
        }

        let record = self.new_record(
            key,
            activity,
            explicit_agent_label,
            status,
            execution_target,
        );
        self.records.push(record);
        self.rebuild_rows();
        true
    }

    fn apply_subagent_handoff_activity(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        item: &AgentMessageItem,
    ) -> bool {
        if item.phase != Some(ProtocolPhase::FinalAnswer)
            || !self.is_observed_subagent_thread(thread_id)
        {
            return false;
        }

        let key = ToolActivityKey {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            item_id: item.id.clone(),
        };
        let tool_display_value = subagent_handoff_display_value(item.text.as_bytes().len());

        if let Some(index) = self.records.iter().position(|existing| existing.key == key) {
            let completion_order = (self.records[index].status == ToolActivityRowStatus::Running)
                .then(|| self.next_start_order());
            let existing = &mut self.records[index];
            let mut changed = false;
            if existing.source != ToolActivityRecordSource::SubagentHandoff {
                existing.source = ToolActivityRecordSource::SubagentHandoff;
                changed = true;
            }
            if existing.explicit_agent_label.take().is_some() {
                changed = true;
            }
            if existing.tool_display_value != tool_display_value {
                existing.tool_display_value = tool_display_value;
                changed = true;
            }
            if existing.status != ToolActivityRowStatus::FinishedOk {
                existing.status = ToolActivityRowStatus::FinishedOk;
                existing.completion_order = completion_order.or(existing.completion_order);
                changed = true;
            }
            if !existing.reasoning_summary_parts.is_empty() {
                existing.reasoning_summary_parts.clear();
                changed = true;
            }
            if changed {
                self.rebuild_rows();
            }
            return changed;
        }

        let completion_order = self.next_start_order();
        let record = ToolActivityRecord {
            key,
            source: ToolActivityRecordSource::SubagentHandoff,
            activity_subject_thread_id: None,
            explicit_agent_label: None,
            tool_display_value,
            status: ToolActivityRowStatus::FinishedOk,
            start_order: completion_order,
            active_epoch: None,
            completion_order: Some(completion_order),
            reasoning_summary_parts: Vec::new(),
            receiver_thread_ids: Vec::new(),
            multi_agent_v2_lifecycle_kind: None,
            multi_agent_v2_agent_path: None,
        };
        self.records.push(record);
        self.rebuild_rows();
        true
    }

    fn new_record(
        &mut self,
        key: ToolActivityKey,
        activity: ToolActivityEvent,
        explicit_agent_label: Option<String>,
        status: ToolActivityRowStatus,
        execution_target: Option<&WorkspaceId>,
    ) -> ToolActivityRecord {
        let start_order = self.next_start_order();
        let active_epoch = (status == ToolActivityRowStatus::Running)
            .then(|| self.active_epoch_for_thread(activity.thread_id.as_str(), start_order));
        let mut record = ToolActivityRecord {
            source: ToolActivityRecordSource::from(activity.source),
            activity_subject_thread_id: activity_subject_thread_id_for_activity(&activity),
            explicit_agent_label,
            tool_display_value: tool_activity_display_value(&activity, execution_target),
            status,
            start_order,
            active_epoch,
            completion_order: (status != ToolActivityRowStatus::Running).then_some(start_order),
            reasoning_summary_parts: Vec::new(),
            receiver_thread_ids: receiver_thread_ids_for_activity(&activity),
            multi_agent_v2_lifecycle_kind: multi_agent_v2_lifecycle_kind_for_activity(&activity),
            multi_agent_v2_agent_path: multi_agent_v2_agent_path_for_activity(&activity),
            key,
        };
        apply_reasoning_summary_detail(&mut record, &activity);
        record.tool_display_value =
            activity_display_value_for_record(&record, &activity, execution_target);
        record
    }

    fn apply_receiver_thread_ownership_updates(
        &mut self,
        activity: &ToolActivityEvent,
    ) -> ReceiverThreadOwnershipChange {
        if activity.source != ToolActivitySource::CollabAgentToolCall {
            return ReceiverThreadOwnershipChange::default();
        }

        let Some(parent_thread_id) = non_blank_str(activity.thread_id.as_str()) else {
            return ReceiverThreadOwnershipChange::default();
        };
        let root_turn = self
            .root_turn_by_child_thread
            .get(parent_thread_id)
            .cloned()
            .unwrap_or_else(|| ToolActivityRootTurnKey {
                thread_id: parent_thread_id.to_string(),
                turn_id: activity.turn_id.clone(),
            });
        let is_multi_agent_v2_lifecycle =
            multi_agent_v2_lifecycle_kind_for_activity(activity).is_some();
        let multi_agent_v2_agent_path = multi_agent_v2_agent_path_for_activity(activity);

        let mut change = ReceiverThreadOwnershipChange::default();
        for receiver_thread_id in receiver_thread_ids_for_activity(activity) {
            let previous = self
                .parent_thread_by_child
                .insert(receiver_thread_id.clone(), parent_thread_id.to_string());
            if previous.as_deref() != Some(parent_thread_id) {
                change.changed = true;
            }
            let previous_root_turn = self
                .root_turn_by_child_thread
                .insert(receiver_thread_id.clone(), root_turn.clone());
            if previous_root_turn.as_ref() != Some(&root_turn) {
                change.changed = true;
            }
            if is_multi_agent_v2_lifecycle {
                if self
                    .multi_agent_v2_child_threads
                    .insert(receiver_thread_id.clone())
                {
                    change.changed = true;
                    change.requires_row_rebuild = true;
                }
                if let Some(agent_path) = multi_agent_v2_agent_path.as_ref() {
                    let previous_path = self
                        .multi_agent_v2_agent_path_by_child
                        .insert(receiver_thread_id.clone(), agent_path.clone());
                    if previous_path.as_ref() != Some(agent_path) {
                        change.changed = true;
                        change.requires_row_rebuild = true;
                    }
                }
            }
            if self.note_activity_subagent_runtime_metadata(
                receiver_thread_id.as_str(),
                activity.collab_agent_spawn_metadata.as_ref(),
            ) {
                change.changed = true;
                change.requires_row_rebuild = true;
            }
        }
        change
    }

    fn note_thread_summary_agent_labels(&mut self, thread: &ThreadSummary) -> bool {
        (!self.is_observed_subagent_thread(&thread.id))
            .then(|| {
                self.note_thread_display_label(
                    thread.id.as_str(),
                    thread.name.as_deref().or(Some(thread.preview.as_str())),
                )
            })
            .unwrap_or(false)
    }

    fn note_subagent_runtime_metadata(
        &mut self,
        thread_id: &str,
        metadata: &ThreadSessionMetadata,
    ) -> bool {
        self.note_subagent_runtime_metadata_values(
            thread_id,
            metadata.model.as_deref(),
            metadata.reasoning_effort.as_deref(),
            true,
        )
    }

    fn note_activity_subagent_runtime_metadata(
        &mut self,
        thread_id: &str,
        metadata: Option<&ToolActivityCollabAgentSpawnMetadata>,
    ) -> bool {
        let Some(metadata) = metadata else {
            return false;
        };
        if normalized_optional_metadata_value(metadata.model.as_deref()).is_none() {
            return false;
        }
        self.note_subagent_runtime_metadata_values(
            thread_id,
            metadata.model.as_deref(),
            metadata.reasoning_effort.as_deref(),
            false,
        )
    }

    fn note_subagent_runtime_metadata_values(
        &mut self,
        thread_id: &str,
        model: Option<&str>,
        reasoning_effort: Option<&str>,
        insert_empty_marker: bool,
    ) -> bool {
        let Some(thread_id) = non_blank_str(thread_id) else {
            return false;
        };
        let model = normalized_optional_metadata_value(model);
        let reasoning_effort = normalized_optional_metadata_value(reasoning_effort);

        if model.is_none() && reasoning_effort.is_none() && !insert_empty_marker {
            return false;
        }

        if let Some(existing) = self.runtime_metadata_by_subagent_thread.get_mut(thread_id) {
            let mut changed = false;
            if let Some(model) = model
                && existing.model.as_deref() != Some(model.as_str())
            {
                existing.model = Some(model);
                changed = true;
            }
            if let Some(reasoning_effort) = reasoning_effort
                && existing.reasoning_effort.as_deref() != Some(reasoning_effort.as_str())
            {
                existing.reasoning_effort = Some(reasoning_effort);
                changed = true;
            }
            return changed;
        }

        self.runtime_metadata_by_subagent_thread.insert(
            thread_id.to_string(),
            SubagentRuntimeMetadata {
                model,
                reasoning_effort,
            },
        );
        true
    }

    fn note_thread_display_label(&mut self, thread_id: &str, label: Option<&str>) -> bool {
        let Some(thread_id) = non_blank_str(thread_id) else {
            return false;
        };
        let Some(label) = label else {
            return false;
        };
        let label = label.trim();
        if thread_id.is_empty() || label.is_empty() {
            return false;
        }
        if is_fallback_agent_label_for_thread(label, thread_id) {
            return false;
        }
        let label = truncate_label_payload(label);

        if self.agent_labels_by_thread.get(thread_id) == Some(&label) {
            return false;
        }

        self.agent_labels_by_thread
            .insert(thread_id.to_string(), label);
        true
    }

    fn rebuild_rows_if_len_changed(&mut self, before_len: usize) -> bool {
        let changed = self.records.len() != before_len;
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    fn prune_retained_records(&mut self) {
        let completed_count = self
            .records
            .iter()
            .filter(|record| record.status != ToolActivityRowStatus::Running)
            .count();
        if completed_count <= ACTIVITY_COMPLETED_ROW_BUDGET
            && self.completed_display_payload_bytes() <= ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET
        {
            return;
        }

        let protected_indexes = self.protected_selected_completed_record_indexes();
        let mut keep = vec![false; self.records.len()];
        let mut retained_completed_rows = 0usize;
        let mut retained_completed_bytes = 0usize;
        let mut retained_identity = self.running_multi_agent_v2_presentation_identity();

        for (index, record) in self.records.iter().enumerate() {
            if record.status == ToolActivityRowStatus::Running {
                keep[index] = true;
            }
        }

        let mut protected_indexes = protected_indexes.into_iter().collect::<Vec<_>>();
        protected_indexes.sort_by(|left, right| {
            self.records[*right]
                .completion_order
                .cmp(&self.records[*left].completion_order)
                .then_with(|| {
                    self.records[*left]
                        .key
                        .thread_id
                        .cmp(&self.records[*right].key.thread_id)
                })
                .then_with(|| {
                    self.records[*left]
                        .key
                        .turn_id
                        .cmp(&self.records[*right].key.turn_id)
                })
                .then_with(|| {
                    self.records[*left]
                        .key
                        .item_id
                        .cmp(&self.records[*right].key.item_id)
                })
        });
        for index in protected_indexes {
            let record = &self.records[index];
            let mut candidate_identity = retained_identity.clone();
            let record_bytes =
                self.completed_record_display_payload_bytes(record, &mut candidate_identity);
            if retained_completed_rows < ACTIVITY_COMPLETED_ROW_BUDGET
                && retained_completed_bytes.saturating_add(record_bytes)
                    <= ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET
            {
                keep[index] = true;
                retained_completed_rows = retained_completed_rows.saturating_add(1);
                retained_completed_bytes = retained_completed_bytes.saturating_add(record_bytes);
                retained_identity = candidate_identity;
            }
        }

        let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (index, record) in self.records.iter().enumerate() {
            if record.status == ToolActivityRowStatus::Running || keep[index] {
                continue;
            }
            groups
                .entry(self.root_turn_key_for_record(record))
                .or_default()
                .push(index);
        }

        let mut groups = groups.into_iter().collect::<Vec<_>>();
        groups.sort_by(|(left_key, left_indexes), (right_key, right_indexes)| {
            let left_latest = latest_completion_order(&self.records, left_indexes);
            let right_latest = latest_completion_order(&self.records, right_indexes);
            right_latest
                .cmp(&left_latest)
                .then_with(|| left_key.cmp(right_key))
        });

        for (_, mut indexes) in groups {
            indexes.sort_by(|left, right| {
                self.records[*right]
                    .completion_order
                    .cmp(&self.records[*left].completion_order)
                    .then_with(|| {
                        self.records[*left]
                            .key
                            .thread_id
                            .cmp(&self.records[*right].key.thread_id)
                    })
                    .then_with(|| {
                        self.records[*left]
                            .key
                            .turn_id
                            .cmp(&self.records[*right].key.turn_id)
                    })
                    .then_with(|| {
                        self.records[*left]
                            .key
                            .item_id
                            .cmp(&self.records[*right].key.item_id)
                    })
            });

            let group_rows = indexes.len();
            let mut group_identity = retained_identity.clone();
            let group_bytes = indexes.iter().fold(0usize, |bytes, index| {
                bytes.saturating_add(self.completed_record_display_payload_bytes(
                    &self.records[*index],
                    &mut group_identity,
                ))
            });
            if retained_completed_rows.saturating_add(group_rows) <= ACTIVITY_COMPLETED_ROW_BUDGET
                && retained_completed_bytes.saturating_add(group_bytes)
                    <= ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET
            {
                for index in indexes {
                    keep[index] = true;
                }
                retained_completed_rows = retained_completed_rows.saturating_add(group_rows);
                retained_completed_bytes = retained_completed_bytes.saturating_add(group_bytes);
                retained_identity = group_identity;
                continue;
            }

            for index in indexes {
                if retained_completed_rows >= ACTIVITY_COMPLETED_ROW_BUDGET {
                    break;
                }
                let mut candidate_identity = retained_identity.clone();
                let record_bytes = self.completed_record_display_payload_bytes(
                    &self.records[index],
                    &mut candidate_identity,
                );
                if retained_completed_bytes.saturating_add(record_bytes)
                    > ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET
                {
                    continue;
                }
                keep[index] = true;
                retained_completed_rows = retained_completed_rows.saturating_add(1);
                retained_completed_bytes = retained_completed_bytes.saturating_add(record_bytes);
                retained_identity = candidate_identity;
            }
        }

        let mut index = 0usize;
        self.records.retain(|_| {
            let retain = keep[index];
            index = index.saturating_add(1);
            retain
        });
    }

    fn completed_display_payload_bytes(&self) -> usize {
        let mut charged_identity = self.running_multi_agent_v2_presentation_identity();
        self.records
            .iter()
            .filter(|record| record.status != ToolActivityRowStatus::Running)
            .map(|record| {
                self.completed_record_display_payload_bytes(record, &mut charged_identity)
            })
            .sum()
    }

    fn completed_record_display_payload_bytes(
        &self,
        record: &ToolActivityRecord,
        charged_identity: &mut ChargedMultiAgentV2PresentationIdentity,
    ) -> usize {
        completed_record_display_payload_bytes(record).saturating_add(
            self.charge_multi_agent_v2_presentation_identity(record, charged_identity),
        )
    }

    fn running_multi_agent_v2_presentation_identity(
        &self,
    ) -> ChargedMultiAgentV2PresentationIdentity {
        let mut charged_identity = ChargedMultiAgentV2PresentationIdentity::default();
        for record in self
            .records
            .iter()
            .filter(|record| record.status == ToolActivityRowStatus::Running)
        {
            self.charge_multi_agent_v2_presentation_identity(record, &mut charged_identity);
        }
        charged_identity
    }

    fn charge_multi_agent_v2_presentation_identity(
        &self,
        record: &ToolActivityRecord,
        charged_identity: &mut ChargedMultiAgentV2PresentationIdentity,
    ) -> usize {
        let child_thread_id = record
            .multi_agent_v2_lifecycle_kind
            .map(|_| record.activity_subject_thread_id.as_deref())
            .unwrap_or_else(|| Some(record.key.thread_id.as_str()));
        let Some(child_thread_id) = child_thread_id
            .filter(|thread_id| self.multi_agent_v2_child_threads.contains(*thread_id))
        else {
            return 0;
        };

        let mut bytes = 0usize;
        if charged_identity
            .child_threads
            .insert(child_thread_id.to_string())
        {
            bytes = bytes.saturating_add(child_thread_id.len());
        }
        let supports_agent_path = record.multi_agent_v2_lifecycle_kind.is_none()
            || record.multi_agent_v2_agent_path.is_some();
        if supports_agent_path
            && charged_identity
                .agent_paths
                .insert(child_thread_id.to_string())
            && let Some(agent_path) = self.multi_agent_v2_agent_path_by_child.get(child_thread_id)
        {
            bytes = bytes.saturating_add(child_thread_id.len() + agent_path.len());
        }
        bytes
    }

    fn protected_selected_completed_record_indexes(&self) -> HashSet<usize> {
        let selected_thread_id = self.last_selected_thread_id.clone();
        let Some(selected_thread_id) = selected_thread_id.as_deref() else {
            return HashSet::new();
        };

        let mut indexes = self
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| {
                record.status != ToolActivityRowStatus::Running
                    && self.record_is_visible_for_thread(record, selected_thread_id)
            })
            .map(|(index, record)| (index, record.completion_order.unwrap_or_default()))
            .collect::<Vec<_>>();
        indexes.sort_by(|(left_index, left_order), (right_index, right_order)| {
            right_order
                .cmp(left_order)
                .then_with(|| {
                    self.records[*left_index]
                        .key
                        .thread_id
                        .cmp(&self.records[*right_index].key.thread_id)
                })
                .then_with(|| {
                    self.records[*left_index]
                        .key
                        .turn_id
                        .cmp(&self.records[*right_index].key.turn_id)
                })
                .then_with(|| {
                    self.records[*left_index]
                        .key
                        .item_id
                        .cmp(&self.records[*right_index].key.item_id)
                })
        });
        indexes
            .into_iter()
            .take(ACTIVITY_SELECTED_COMPLETED_ROW_WINDOW)
            .map(|(index, _)| index)
            .collect()
    }

    fn root_turn_key_for_record(&self, record: &ToolActivityRecord) -> (String, String) {
        if let Some(root_turn) = self.root_turn_by_child_thread.get(&record.key.thread_id) {
            return (root_turn.thread_id.clone(), root_turn.turn_id.clone());
        }
        (
            self.root_thread_id_for_thread(&record.key.thread_id),
            record.key.turn_id.clone(),
        )
    }

    fn root_thread_id_for_thread(&self, thread_id: &str) -> String {
        let mut root_thread_id = thread_id.to_string();
        let mut current_thread_id = thread_id;
        let mut seen = HashSet::new();
        for _ in 0..self.parent_thread_by_child.len() {
            if !seen.insert(current_thread_id.to_string()) {
                break;
            }
            let Some(parent_thread_id) = self.parent_thread_by_child.get(current_thread_id) else {
                break;
            };
            if parent_thread_id == current_thread_id {
                break;
            }
            root_thread_id = parent_thread_id.clone();
            current_thread_id = parent_thread_id;
        }
        root_thread_id
    }

    fn record_is_visible_for_thread(
        &self,
        record: &ToolActivityRecord,
        selected_thread_id: &str,
    ) -> bool {
        if record.key.thread_id == selected_thread_id {
            return true;
        }
        let mut current_thread_id = record.key.thread_id.as_str();
        let mut seen = HashSet::new();
        for _ in 0..self.parent_thread_by_child.len() {
            if !seen.insert(current_thread_id.to_string()) {
                return false;
            }
            let Some(parent_thread_id) = self.parent_thread_by_child.get(current_thread_id) else {
                return false;
            };
            if parent_thread_id == selected_thread_id {
                return true;
            }
            if parent_thread_id == current_thread_id {
                return false;
            }
            current_thread_id = parent_thread_id;
        }
        false
    }

    fn prune_derived_state(&mut self) {
        let mut referenced_threads = HashSet::new();
        let mut required_child_links = HashSet::new();
        let mut retained_record_child_links = HashSet::new();
        let mut retained_v2_child_sources = HashSet::new();
        let mut retained_v2_path_sources = HashSet::new();
        let mut retained_non_lifecycle_threads = HashSet::new();

        for record in &self.records {
            if record.multi_agent_v2_lifecycle_kind.is_some() {
                if let Some(subject_thread_id) = record.activity_subject_thread_id.as_ref() {
                    retained_v2_child_sources.insert(subject_thread_id.clone());
                    if record.multi_agent_v2_agent_path.is_some() {
                        retained_v2_path_sources.insert(subject_thread_id.clone());
                    }
                }
            } else {
                retained_non_lifecycle_threads.insert(record.key.thread_id.clone());
            }
            self.collect_thread_reference(
                record.key.thread_id.as_str(),
                &mut referenced_threads,
                &mut required_child_links,
            );
            for child_thread_id in &record.receiver_thread_ids {
                retained_record_child_links.insert(child_thread_id.clone());
                referenced_threads.insert(child_thread_id.clone());
                referenced_threads.insert(record.key.thread_id.clone());
            }
            if let Some(subject_thread_id) = &record.activity_subject_thread_id {
                retained_record_child_links.insert(subject_thread_id.clone());
                referenced_threads.insert(subject_thread_id.clone());
                referenced_threads.insert(record.key.thread_id.clone());
            }
        }

        self.parent_thread_by_child.retain(|child, parent| {
            let keep =
                required_child_links.contains(child) || retained_record_child_links.contains(child);
            if keep {
                referenced_threads.insert(child.clone());
                referenced_threads.insert(parent.clone());
            }
            keep
        });
        self.root_turn_by_child_thread
            .retain(|child, _| self.parent_thread_by_child.contains_key(child));
        self.agent_labels_by_thread
            .retain(|thread_id, _| referenced_threads.contains(thread_id));
        self.runtime_metadata_by_subagent_thread
            .retain(|thread_id, _| referenced_threads.contains(thread_id));
        self.multi_agent_v2_child_threads.retain(|thread_id| {
            retained_v2_child_sources.contains(thread_id)
                || retained_non_lifecycle_threads.contains(thread_id)
        });
        self.multi_agent_v2_agent_path_by_child
            .retain(|thread_id, _| {
                self.multi_agent_v2_child_threads.contains(thread_id)
                    && (retained_v2_path_sources.contains(thread_id)
                        || retained_non_lifecycle_threads.contains(thread_id))
            });
    }

    fn collect_thread_reference(
        &self,
        thread_id: &str,
        referenced_threads: &mut HashSet<String>,
        required_child_links: &mut HashSet<String>,
    ) {
        referenced_threads.insert(thread_id.to_string());
        let mut current_thread_id = thread_id;
        let mut seen = HashSet::new();
        for _ in 0..self.parent_thread_by_child.len() {
            if !seen.insert(current_thread_id.to_string()) {
                break;
            }
            let Some(parent_thread_id) = self.parent_thread_by_child.get(current_thread_id) else {
                break;
            };
            if parent_thread_id == current_thread_id {
                break;
            }
            required_child_links.insert(current_thread_id.to_string());
            referenced_threads.insert(parent_thread_id.clone());
            current_thread_id = parent_thread_id;
        }
    }

    fn rebuild_rows(&mut self) {
        let previous_row_model = self
            .rows
            .iter()
            .map(|row| {
                (
                    row.key.thread_id.clone(),
                    row.key.turn_id.clone(),
                    row.key.item_id.clone(),
                    row.status,
                )
            })
            .collect::<Vec<_>>();
        self.prune_derived_state();
        self.prune_retained_records();
        self.prune_derived_state();
        let mut records = self.records.clone();
        records.sort_by(|left, right| {
            compare_activity_records(
                left,
                right,
                self.is_main_activity_record(left),
                self.is_main_activity_record(right),
            )
        });
        self.rows = records
            .into_iter()
            .map(|record| ToolActivityRow {
                agent_label: self.agent_label_for_record(&record),
                stable_identity: record.key.stable_identity(),
                key: record.key,
                tool_display_value: record.tool_display_value,
                status: record.status,
            })
            .collect();
        self.rebuild_visible_row_indexes();
        let current_row_model = self
            .rows
            .iter()
            .map(|row| {
                (
                    row.key.thread_id.clone(),
                    row.key.turn_id.clone(),
                    row.key.item_id.clone(),
                    row.status,
                )
            })
            .collect::<Vec<_>>();
        if previous_row_model != current_row_model {
            self.projection_revision = self.projection_revision.saturating_add(1);
            self.newest_lifecycle_sequence_in_projection =
                self.lifecycle_diagnostics.snapshot().newest_sequence;
        }
    }

    fn refresh_projection_lifecycle_sequence(&mut self) {
        self.newest_lifecycle_sequence_in_projection =
            self.lifecycle_diagnostics.snapshot().newest_sequence;
    }

    fn next_start_order(&mut self) -> u64 {
        let order = self.next_start_order;
        self.next_start_order = self.next_start_order.saturating_add(1);
        order
    }

    fn active_epoch_for_thread(&self, thread_id: &str, fresh_epoch: u64) -> u64 {
        self.records
            .iter()
            .filter(|record| {
                record.status == ToolActivityRowStatus::Running
                    && active_thread_id_for_record(record) == thread_id
            })
            .filter_map(|record| record.active_epoch)
            .min()
            .unwrap_or(fresh_epoch)
    }

    fn is_main_activity_record(&self, record: &ToolActivityRecord) -> bool {
        self.last_selected_thread_id.as_deref() == Some(active_thread_id_for_record(record))
            || (record.explicit_agent_label.as_deref() == Some("Main")
                && !self.is_observed_subagent_thread(record.key.thread_id.as_str())
                && record.activity_subject_thread_id.is_none())
    }

    fn visible_row_indexes_for_selected_thread(
        &self,
        selected_thread_id: Option<&str>,
    ) -> Option<&[usize]> {
        let selected_thread_id = selected_thread_id.and_then(non_blank_str)?;
        self.visible_row_indexes_by_thread
            .get(selected_thread_id)
            .map(Vec::as_slice)
    }

    fn rebuild_visible_row_indexes(&mut self) {
        let mut visible_row_indexes_by_thread: HashMap<String, Vec<usize>> = HashMap::new();

        for (row_index, row) in self.rows.iter().enumerate() {
            for thread_id in self.visible_thread_ids_for_row(row) {
                visible_row_indexes_by_thread
                    .entry(thread_id)
                    .or_default()
                    .push(row_index);
            }
        }

        self.visible_row_indexes_by_thread = visible_row_indexes_by_thread;
    }

    fn visible_thread_ids_for_row(&self, row: &ToolActivityRow) -> Vec<String> {
        let mut thread_ids = vec![row.key.thread_id.clone()];
        let mut current_thread_id = row.key.thread_id.as_str();

        for _ in 0..self.parent_thread_by_child.len() {
            let Some(parent_thread_id) = self.parent_thread_by_child.get(current_thread_id) else {
                break;
            };
            if parent_thread_id == current_thread_id
                || thread_ids
                    .iter()
                    .any(|thread_id| thread_id == parent_thread_id)
            {
                break;
            }

            thread_ids.push(parent_thread_id.clone());
            current_thread_id = parent_thread_id.as_str();
        }

        thread_ids
    }

    fn agent_label_for_record(&self, record: &ToolActivityRecord) -> String {
        if record.multi_agent_v2_lifecycle_kind.is_some()
            && let Some(subject_thread_id) = record.activity_subject_thread_id.as_deref()
        {
            let Some(agent_path) = record.multi_agent_v2_agent_path.as_deref() else {
                return String::new();
            };
            return self.multi_agent_v2_agent_label(subject_thread_id, agent_path);
        }
        if self.last_selected_thread_id.as_deref() == Some(active_thread_id_for_record(record)) {
            return "Main".to_string();
        }
        let thread_id = record.key.thread_id.as_str();
        if self.is_observed_subagent_thread(thread_id) {
            if let Some(agent_path) = self.multi_agent_v2_agent_path_by_child.get(thread_id) {
                return self.multi_agent_v2_agent_label(thread_id, agent_path);
            }
            if self.multi_agent_v2_child_threads.contains(thread_id) {
                return String::new();
            }
            return self.subagent_agent_label(thread_id, "Subagent");
        }
        if record.explicit_agent_label.as_deref() == Some("Main") {
            return "Main".to_string();
        }
        if let Some(explicit_agent_label) = record.explicit_agent_label.as_ref() {
            return explicit_agent_label.clone();
        }
        self.agent_labels_by_thread
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    fn subagent_agent_label(&self, thread_id: &str, label: &str) -> String {
        if label.is_empty() {
            return String::new();
        }
        self.runtime_metadata_by_subagent_thread
            .get(thread_id)
            .map_or_else(
                || label.to_string(),
                |metadata| format_subagent_agent_label(label, metadata),
            )
    }

    fn multi_agent_v2_agent_label(&self, thread_id: &str, agent_path: &str) -> String {
        if agent_path.is_empty() {
            return String::new();
        }
        self.runtime_metadata_by_subagent_thread
            .get(thread_id)
            .map_or_else(
                || agent_path.to_string(),
                |metadata| format_multi_agent_v2_agent_label(agent_path, metadata),
            )
    }

    fn is_observed_subagent_thread(&self, thread_id: &str) -> bool {
        self.parent_thread_by_child.contains_key(thread_id)
    }
}

impl ToolActivityKey {
    fn from_activity(activity: &ToolActivityEvent) -> Self {
        Self {
            thread_id: activity.thread_id.clone(),
            turn_id: activity.turn_id.clone(),
            item_id: activity.item_id.clone(),
        }
    }

    fn stable_identity(&self) -> SharedString {
        format!(
            "thread:{}:{}:turn:{}:{}:item:{}:{}",
            self.thread_id.len(),
            self.thread_id,
            self.turn_id.len(),
            self.turn_id,
            self.item_id.len(),
            self.item_id,
        )
        .into()
    }
}

impl From<ToolActivitySource> for ToolActivityRecordSource {
    fn from(source: ToolActivitySource) -> Self {
        Self::Backend(source)
    }
}

impl ToolActivityRecordSource {
    fn is_backend(self, source: ToolActivitySource) -> bool {
        self == Self::Backend(source)
    }
}

impl MultiAgentV2LifecycleKind {
    pub(super) fn diagnostic_label(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Interacted => "interacted",
            Self::Interrupted => "interrupted",
        }
    }
}

impl ToolActivityRowStatus {
    fn sort_rank(self) -> u8 {
        match self {
            Self::Running => 0,
            Self::FinishedOk | Self::FinishedError => 1,
        }
    }

    pub(super) fn diagnostic_label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::FinishedOk => "finished_ok",
            Self::FinishedError => "finished_error",
        }
    }
}

fn compare_activity_records(
    left: &ToolActivityRecord,
    right: &ToolActivityRecord,
    left_is_main: bool,
    right_is_main: bool,
) -> std::cmp::Ordering {
    left.status
        .sort_rank()
        .cmp(&right.status.sort_rank())
        .then_with(|| match (left.status, right.status) {
            (ToolActivityRowStatus::Running, ToolActivityRowStatus::Running) => right_is_main
                .cmp(&left_is_main)
                .then_with(|| left.active_epoch.cmp(&right.active_epoch))
                .then_with(|| left.start_order.cmp(&right.start_order)),
            _ => right.completion_order.cmp(&left.completion_order),
        })
        .then_with(|| left.key.thread_id.cmp(&right.key.thread_id))
        .then_with(|| left.key.turn_id.cmp(&right.key.turn_id))
        .then_with(|| left.key.item_id.cmp(&right.key.item_id))
}

fn active_thread_id_for_record(record: &ToolActivityRecord) -> &str {
    record
        .activity_subject_thread_id
        .as_deref()
        .unwrap_or(record.key.thread_id.as_str())
}

fn latest_completion_order(records: &[ToolActivityRecord], indexes: &[usize]) -> u64 {
    indexes
        .iter()
        .filter_map(|index| records[*index].completion_order)
        .max()
        .unwrap_or_default()
}

fn completed_record_display_payload_bytes(record: &ToolActivityRecord) -> usize {
    let activity_subject_identity_bytes = record
        .activity_subject_thread_id
        .as_ref()
        .map_or(0, String::len);
    let receiver_identity_bytes = record
        .receiver_thread_ids
        .iter()
        .map(String::len)
        .sum::<usize>();
    activity_subject_identity_bytes
        + receiver_identity_bytes
        + record.explicit_agent_label.as_ref().map_or(0, String::len)
        + record
            .multi_agent_v2_agent_path
            .as_ref()
            .map_or(0, String::len)
        + record.tool_display_value.len()
        + record
            .reasoning_summary_parts
            .iter()
            .map(String::len)
            .sum::<usize>()
}

pub(super) fn fallback_agent_label(thread_id: &str) -> String {
    let trimmed = thread_id.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        format!("thread:{trimmed}")
    }
}

fn format_subagent_agent_label(label: &str, metadata: &SubagentRuntimeMetadata) -> String {
    let Some(model) = metadata.model.as_deref() else {
        return label.to_string();
    };
    let label = if let Some(reasoning_effort) = metadata.reasoning_effort.as_deref() {
        format!("{label} ({model}/{reasoning_effort})")
    } else {
        format!("{label} ({model})")
    };
    truncate_label_payload(&label)
}

fn format_multi_agent_v2_agent_label(
    agent_path: &str,
    metadata: &SubagentRuntimeMetadata,
) -> String {
    let Some(model) = metadata.model.as_deref() else {
        return agent_path.to_string();
    };
    if let Some(reasoning_effort) = metadata.reasoning_effort.as_deref() {
        format!("{agent_path} ({model}/{reasoning_effort})")
    } else {
        format!("{agent_path} ({model})")
    }
}

fn is_fallback_agent_label_for_thread(label: &str, thread_id: &str) -> bool {
    label.trim() == fallback_agent_label(thread_id)
}

fn truncate_display_payload(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    if max_bytes <= 3 {
        return ".".repeat(max_bytes);
    }

    let prefix_budget = max_bytes.saturating_sub(3);
    let mut end = 0;
    for (index, character) in value.char_indices() {
        let next = index.saturating_add(character.len_utf8());
        if next > prefix_budget {
            break;
        }
        end = next;
    }
    let mut truncated = value[..end].trim_end().to_string();
    truncated.push_str("...");
    truncated
}

fn truncate_label_payload(value: &str) -> String {
    truncate_display_payload(value, ACTIVITY_LABEL_DISPLAY_BYTE_LIMIT)
}

fn truncate_activity_display_payload(value: &str) -> String {
    truncate_display_payload(value, ACTIVITY_DISPLAY_VALUE_BYTE_LIMIT)
}

fn truncate_reasoning_summary_payload(value: &str) -> String {
    truncate_display_payload(value, ACTIVITY_REASONING_SUMMARY_BYTE_LIMIT)
}

pub(super) const ACTIVITY_COMPLETED_ROW_BUDGET: usize = 2_000;
pub(super) const ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET: usize = 8 * 1024 * 1024;
pub(super) const ACTIVITY_SELECTED_COMPLETED_ROW_WINDOW: usize = 200;
pub(super) const ACTIVITY_LABEL_DISPLAY_BYTE_LIMIT: usize = 16 * 1024;
pub(super) const ACTIVITY_DISPLAY_VALUE_BYTE_LIMIT: usize = 16 * 1024;
pub(super) const ACTIVITY_REASONING_SUMMARY_BYTE_LIMIT: usize = 64 * 1024;
const ACTIVITY_RECEIVER_THREAD_ID_LIMIT: usize = 64;
const ACTIVITY_REASONING_SUMMARY_PART_LIMIT: usize = 64;
const REASONING_SUMMARY_DISPLAY_MAX_CHARS: usize = 120;
const WINDOWS_POWERSHELL_LAUNCHER_DISPLAY: &str = "powershell.exe";
static WINDOWS_POWERSHELL_LAUNCHER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^[A-Z]:(?:\\\\|\\)Windows(?:\.old)?(?:\\\\|\\)System32(?:\\\\|\\)WindowsPowerShell(?:\\\\|\\)v1\.0(?:\\\\|\\)powershell\.exe$",
    )
    .expect("Windows PowerShell launcher regex must compile")
});

fn tool_activity_display_value(
    activity: &ToolActivityEvent,
    execution_target: Option<&WorkspaceId>,
) -> String {
    let display_value = if activity.source == ToolActivitySource::Reasoning {
        reasoning_activity_display_value(
            activity
                .reasoning_summary_text
                .as_deref()
                .unwrap_or_default(),
        )
    } else if activity.source == ToolActivitySource::CommandExecution {
        if let Some(command_line) = first_non_empty_command_line(activity.raw_command.as_deref()) {
            command_execution_display_line(command_line)
        } else {
            activity.item_type.clone()
        }
    } else if activity.source == ToolActivitySource::FileChange
        && let Some(summary) = activity.file_change_summary.as_ref()
    {
        file_change_display_value(summary, execution_target)
    } else {
        activity
            .raw_tool_name
            .as_deref()
            .or(activity.raw_resource_uri.as_deref())
            .map(str::to_string)
            .unwrap_or_else(|| activity.item_type.clone())
    };
    truncate_activity_display_payload(&display_value)
}

fn subagent_handoff_display_value(byte_len: usize) -> String {
    format!("handoff: {byte_len} bytes")
}

fn file_change_display_value(
    summary: &ToolActivityFileChangeSummary,
    execution_target: Option<&WorkspaceId>,
) -> String {
    if let Some(path) = single_relative_file_change_path(summary, execution_target) {
        return format!(
            "Patching {}, +{} -{}",
            path, summary.additions, summary.deletions
        );
    }

    let file_label = if summary.file_count == 1 {
        "file"
    } else {
        "files"
    };
    format!(
        "Patching {} {}, +{} -{}",
        summary.file_count, file_label, summary.additions, summary.deletions
    )
}

fn single_relative_file_change_path(
    summary: &ToolActivityFileChangeSummary,
    execution_target: Option<&WorkspaceId>,
) -> Option<String> {
    if summary.file_count != 1 {
        return None;
    }

    let path = summary.single_file_path.as_deref()?;
    match execution_target.map(WorkspaceId::runtime_mode) {
        Some(RuntimeMode::HostWindows) => execution_target.and_then(|target| {
            host_windows_relative_file_change_path(path, target.canonical_path())
        }),
        Some(RuntimeMode::WslLinux { .. }) => execution_target
            .and_then(|target| wsl_relative_file_change_path(path, target.canonical_path())),
        None => generic_relative_file_change_path(path),
    }
}

fn generic_relative_file_change_path(path: &Path) -> Option<String> {
    let path_text = path.to_string_lossy();
    is_windows_plain_relative_path(&path_text).then(|| path_text.into_owned())
}

fn host_windows_relative_file_change_path(path: &Path, root: &Path) -> Option<String> {
    let path_text = path.to_string_lossy();
    if is_windows_plain_relative_path(&path_text) {
        return non_empty_path_text(path_text.into_owned());
    }
    if !is_windows_absolute_path_text(&path_text) {
        return None;
    }

    let normalized_path = normalize_windows_file_change_path(&path_text);
    let normalized_root =
        trim_windows_prefix_root(&normalize_windows_file_change_path(&root.to_string_lossy()));
    if normalized_path.eq_ignore_ascii_case(&normalized_root) {
        return None;
    }

    let prefix = normalized_path.get(..normalized_root.len())?;
    if !prefix.eq_ignore_ascii_case(&normalized_root) {
        return None;
    }

    let relative_path = normalized_path
        .get(normalized_root.len()..)?
        .strip_prefix('\\')?;
    non_empty_path_text(relative_path.to_string())
}

fn wsl_relative_file_change_path(path: &Path, root: &Path) -> Option<String> {
    let path_text = path.to_string_lossy();
    if path_text.is_empty() {
        return None;
    }
    if !path_text.starts_with('/') {
        return Some(path_text.into_owned());
    }

    let root_text = root.to_string_lossy();
    let root_text = trim_wsl_root_path(&root_text);
    if root_text == "/" {
        return path_text
            .strip_prefix('/')
            .and_then(|relative_path| non_empty_path_text(relative_path.to_string()));
    }
    if path_text == root_text {
        return None;
    }

    let relative_path = path_text.strip_prefix(root_text)?.strip_prefix('/')?;
    non_empty_path_text(relative_path.to_string())
}

fn is_windows_plain_relative_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\");
    !normalized.is_empty()
        && !normalized.starts_with('\\')
        && !has_windows_drive_prefix(normalized.as_str())
}

fn is_windows_absolute_path_text(path: &str) -> bool {
    let normalized = path.replace('/', "\\");
    has_windows_drive_root(normalized.as_str()) || normalized.starts_with(r"\\")
}

fn has_windows_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn normalize_windows_file_change_path(path: &str) -> String {
    let normalized = path.replace('/', "\\");
    if let Some(stripped) = normalized.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{stripped}")
    } else if let Some(stripped) = normalized.strip_prefix(r"\\?\") {
        stripped.to_string()
    } else {
        normalized
    }
}

fn trim_windows_prefix_root(path: &str) -> String {
    let trimmed = path.trim_end_matches('\\');
    if trimmed.is_empty() {
        path.to_string()
    } else {
        trimmed.to_string()
    }
}

fn trim_wsl_root_path(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() { "/" } else { trimmed }
}

fn non_empty_path_text(path: String) -> Option<String> {
    (!path.is_empty()).then_some(path)
}

fn receiver_thread_ids_for_activity(activity: &ToolActivityEvent) -> Vec<String> {
    if activity.source != ToolActivitySource::CollabAgentToolCall {
        return Vec::new();
    }
    let Some(parent_thread_id) = non_blank_str(activity.thread_id.as_str()) else {
        return Vec::new();
    };

    let mut receiver_thread_ids = Vec::new();
    for receiver_thread_id in &activity.receiver_thread_ids {
        let Some(receiver_thread_id) = non_blank_str(receiver_thread_id.as_str()) else {
            continue;
        };
        if receiver_thread_id == parent_thread_id
            || receiver_thread_ids
                .iter()
                .any(|existing: &String| existing == receiver_thread_id)
        {
            continue;
        }
        receiver_thread_ids.push(receiver_thread_id.to_string());
        if receiver_thread_ids.len() >= ACTIVITY_RECEIVER_THREAD_ID_LIMIT {
            break;
        }
    }
    receiver_thread_ids
}

fn activity_subject_thread_id_for_activity(activity: &ToolActivityEvent) -> Option<String> {
    (activity.item_type == "subAgentActivity"
        && activity.source == ToolActivitySource::CollabAgentToolCall
        && activity.lifecycle == ToolActivityLifecycle::Completed)
        .then(|| receiver_thread_ids_for_activity(activity))
        .and_then(|receiver_thread_ids| {
            (receiver_thread_ids.len() == 1).then(|| receiver_thread_ids[0].clone())
        })
}

fn multi_agent_v2_lifecycle_kind_for_activity(
    activity: &ToolActivityEvent,
) -> Option<MultiAgentV2LifecycleKind> {
    if activity.item_type != "subAgentActivity"
        || activity.source != ToolActivitySource::CollabAgentToolCall
        || activity.lifecycle != ToolActivityLifecycle::Completed
    {
        return None;
    }
    match activity.raw_tool_name.as_deref() {
        Some("started") => Some(MultiAgentV2LifecycleKind::Started),
        Some("interacted") => Some(MultiAgentV2LifecycleKind::Interacted),
        Some("interrupted") => Some(MultiAgentV2LifecycleKind::Interrupted),
        _ => None,
    }
}

fn multi_agent_v2_agent_path_for_activity(activity: &ToolActivityEvent) -> Option<String> {
    multi_agent_v2_lifecycle_kind_for_activity(activity)?;
    activity_subject_thread_id_for_activity(activity)?;
    activity
        .sub_agent_activity_path
        .as_deref()
        .and_then(non_blank_str)
        .map(str::to_string)
}

fn merge_multi_agent_v2_lifecycle_kind(
    record: &mut ToolActivityRecord,
    activity: &ToolActivityEvent,
) -> bool {
    let Some(incoming) = multi_agent_v2_lifecycle_kind_for_activity(activity) else {
        return false;
    };
    let merged = record
        .multi_agent_v2_lifecycle_kind
        .map_or(incoming, |existing| existing.min(incoming));
    if record.multi_agent_v2_lifecycle_kind == Some(merged) {
        return false;
    }
    record.multi_agent_v2_lifecycle_kind = Some(merged);
    true
}

fn merge_multi_agent_v2_agent_path(
    record: &mut ToolActivityRecord,
    activity: &ToolActivityEvent,
) -> bool {
    let Some(incoming) = multi_agent_v2_agent_path_for_activity(activity) else {
        return false;
    };
    if record.multi_agent_v2_agent_path.as_ref() == Some(&incoming) {
        return false;
    }
    record.multi_agent_v2_agent_path = Some(incoming);
    true
}

fn merge_activity_subject_thread_id(
    record: &mut ToolActivityRecord,
    activity: &ToolActivityEvent,
) -> bool {
    let Some(subject_thread_id) = activity_subject_thread_id_for_activity(activity) else {
        return false;
    };
    if record.activity_subject_thread_id.as_deref() == Some(subject_thread_id.as_str()) {
        return false;
    }
    record.activity_subject_thread_id = Some(subject_thread_id);
    true
}

fn merge_receiver_thread_ids(
    record: &mut ToolActivityRecord,
    activity: &ToolActivityEvent,
) -> bool {
    let mut changed = false;
    for receiver_thread_id in receiver_thread_ids_for_activity(activity) {
        if record
            .receiver_thread_ids
            .iter()
            .any(|existing| existing == &receiver_thread_id)
        {
            continue;
        }
        if record.receiver_thread_ids.len() >= ACTIVITY_RECEIVER_THREAD_ID_LIMIT {
            break;
        }
        record.receiver_thread_ids.push(receiver_thread_id);
        changed = true;
    }
    changed
}

fn activity_display_value_for_record(
    record: &ToolActivityRecord,
    activity: &ToolActivityEvent,
    execution_target: Option<&WorkspaceId>,
) -> String {
    if let Some(kind) = record.multi_agent_v2_lifecycle_kind {
        return kind.diagnostic_label().to_string();
    }
    if record.source.is_backend(ToolActivitySource::Reasoning) {
        return reasoning_activity_display_value(&record.reasoning_summary_parts.join(""));
    }

    tool_activity_display_value(activity, execution_target)
}

fn apply_reasoning_summary_detail(
    record: &mut ToolActivityRecord,
    activity: &ToolActivityEvent,
) -> bool {
    if activity.source != ToolActivitySource::Reasoning {
        return false;
    }

    if let Some(summary_text) = activity.reasoning_summary_text.as_ref() {
        if summary_text.is_empty() {
            return false;
        }
        let replacement = vec![truncate_reasoning_summary_payload(summary_text)];
        if record.reasoning_summary_parts == replacement {
            return false;
        }
        record.reasoning_summary_parts = replacement;
        return true;
    }

    let Some(summary_index) = activity
        .reasoning_summary_index
        .or_else(|| activity.reasoning_summary_delta.as_ref().map(|_| 0))
    else {
        return false;
    };
    if summary_index >= ACTIVITY_REASONING_SUMMARY_PART_LIMIT {
        return false;
    }

    let mut changed =
        ensure_reasoning_summary_slot(&mut record.reasoning_summary_parts, summary_index);
    if let Some(delta) = activity.reasoning_summary_delta.as_ref()
        && !delta.is_empty()
    {
        let current_bytes = record
            .reasoning_summary_parts
            .iter()
            .map(String::len)
            .sum::<usize>();
        let remaining = ACTIVITY_REASONING_SUMMARY_BYTE_LIMIT.saturating_sub(current_bytes);
        if remaining > 0 {
            let delta = truncate_display_payload(delta, remaining);
            if !delta.is_empty() {
                record.reasoning_summary_parts[summary_index].push_str(&delta);
                changed = true;
            }
        }
    }
    changed
}

fn ensure_reasoning_summary_slot(parts: &mut Vec<String>, index: usize) -> bool {
    let required_len = index.saturating_add(1);
    if parts.len() >= required_len {
        return false;
    }
    parts.resize(required_len, String::new());
    true
}

fn reasoning_activity_display_value(summary_text: &str) -> String {
    normalized_reasoning_summary_excerpt(summary_text)
        .map(|summary| format!("reasoning: {summary}"))
        .unwrap_or_else(|| "reasoning".to_string())
}

fn normalized_reasoning_summary_excerpt(summary_text: &str) -> Option<String> {
    let normalized = summary_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        return None;
    }
    if normalized.chars().count() <= REASONING_SUMMARY_DISPLAY_MAX_CHARS {
        return Some(normalized);
    }

    let prefix_len = REASONING_SUMMARY_DISPLAY_MAX_CHARS.saturating_sub(3);
    let mut truncated = normalized.chars().take(prefix_len).collect::<String>();
    let trimmed_len = truncated.trim_end().len();
    truncated.truncate(trimmed_len);
    truncated.push_str("...");
    Some(truncated)
}

fn first_non_empty_command_line(command: Option<&str>) -> Option<&str> {
    command?
        .split(['\r', '\n'])
        .map(str::trim)
        .find(|line| !line.is_empty())
}

fn command_execution_display_line(line: &str) -> String {
    let Some((token, rest)) = first_command_token(line) else {
        return line.to_string();
    };

    if is_windows_powershell_launcher(token) {
        format!("{WINDOWS_POWERSHELL_LAUNCHER_DISPLAY}{rest}")
    } else {
        line.to_string()
    }
}

fn first_command_token(line: &str) -> Option<(&str, &str)> {
    if let Some(unquoted) = line.strip_prefix('"') {
        if let Some(closing_quote_index) = unquoted.find('"') {
            let token = &unquoted[..closing_quote_index];
            let rest = &unquoted[closing_quote_index + 1..];
            return Some((token, rest));
        }

        return None;
    }

    let first_whitespace_index = line
        .char_indices()
        .find_map(|(index, character)| character.is_whitespace().then_some(index));

    if let Some(first_whitespace_index) = first_whitespace_index {
        Some((
            &line[..first_whitespace_index],
            &line[first_whitespace_index..],
        ))
    } else if line.is_empty() {
        None
    } else {
        Some((line, ""))
    }
}

fn is_windows_powershell_launcher(token: &str) -> bool {
    WINDOWS_POWERSHELL_LAUNCHER_RE.is_match(token)
}

fn non_empty_trimmed_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalized_optional_metadata_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(truncate_label_payload)
}

fn explicit_agent_label_for_activity(
    activity: &ToolActivityEvent,
    explicit_agent_label: Option<String>,
) -> Option<String> {
    let explicit_agent_label = explicit_agent_label.and_then(non_empty_trimmed_string)?;
    if explicit_agent_label == "Main"
        || !is_fallback_agent_label_for_thread(&explicit_agent_label, &activity.thread_id)
    {
        return Some(truncate_label_payload(&explicit_agent_label));
    }
    None
}

fn non_blank_str(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

fn final_status_from_item_status(raw_item_status: Option<&str>) -> ToolActivityRowStatus {
    let Some(raw_item_status) = raw_item_status else {
        return ToolActivityRowStatus::FinishedOk;
    };
    let normalized = raw_item_status
        .chars()
        .filter(|character| *character != '-' && *character != '_' && !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "failed" | "error" | "errored" | "declined" | "interrupted" | "canceled" | "cancelled" => {
            ToolActivityRowStatus::FinishedError
        }
        _ => ToolActivityRowStatus::FinishedOk,
    }
}

fn lifecycle_diagnostic_kind(lifecycle: ToolActivityLifecycle) -> &'static str {
    match lifecycle {
        ToolActivityLifecycle::Started => "started",
        ToolActivityLifecycle::Updated => "updated",
        ToolActivityLifecycle::Completed => "completed",
    }
}

fn lifecycle_projection_outcome(
    lifecycle: ToolActivityLifecycle,
    before_status: Option<ToolActivityRowStatus>,
) -> &'static str {
    match (lifecycle, before_status) {
        (ToolActivityLifecycle::Started, None) => "inserted_running",
        (ToolActivityLifecycle::Started, Some(ToolActivityRowStatus::Running)) => "matched_running",
        (ToolActivityLifecycle::Started, Some(_)) => "reactivated_existing",
        (ToolActivityLifecycle::Updated, None) => "inserted_running",
        (ToolActivityLifecycle::Updated, Some(_)) => "matched_existing",
        (ToolActivityLifecycle::Completed, None) => "inserted_completed",
        (ToolActivityLifecycle::Completed, Some(_)) => "matched_existing",
    }
}

fn fallback_outcome(affected: usize) -> &'static str {
    if affected == 0 {
        "no_running_match"
    } else {
        "finished_running_rows"
    }
}

fn final_status_from_turn_status(status: TurnStatus) -> Option<ToolActivityRowStatus> {
    match status {
        TurnStatus::Completed => Some(ToolActivityRowStatus::FinishedOk),
        TurnStatus::Interrupted | TurnStatus::Failed => Some(ToolActivityRowStatus::FinishedError),
        TurnStatus::InProgress => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, TrySendError},
    };

    use serde_json::json;

    use super::*;

    fn without_elapsed(
        mut snapshot: ActivityLifecycleDiagnosticSnapshot,
    ) -> ActivityLifecycleDiagnosticSnapshot {
        for event in &mut snapshot.events {
            event.elapsed_micros = 0;
        }
        snapshot
    }

    fn apply_command_lifecycle(projection: &mut ToolActivityProjection) {
        let started_item: ThreadItem = serde_json::from_value(json!({
            "id": "command",
            "type": "commandExecution",
            "command": "content excluded from diagnostics",
            "cwd": "C:/content/excluded",
            "status": "inProgress"
        }))
        .unwrap();
        let completed_item: ThreadItem = serde_json::from_value(json!({
            "id": "command",
            "type": "commandExecution",
            "command": "content excluded from diagnostics",
            "cwd": "C:/content/excluded",
            "status": "failed"
        }))
        .unwrap();

        assert!(projection.apply_stream_event(
            &TurnStreamEvent::ItemStarted {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                item: started_item,
            },
            None,
        ));
        assert!(projection.apply_stream_event(
            &TurnStreamEvent::ItemCompleted {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
                item: completed_item,
            },
            None,
        ));
    }

    fn assert_projection_diagnostics_equal(
        actual: &ToolActivityProjection,
        expected: &ToolActivityProjection,
    ) {
        assert_eq!(actual.rows(), expected.rows());
        let actual_presentation = actual.presentation_diagnostic_state();
        let expected_presentation = expected.presentation_diagnostic_state();
        assert_eq!(actual_presentation.revision, expected_presentation.revision);
        assert_eq!(
            actual_presentation.newest_lifecycle_sequence,
            expected_presentation.newest_lifecycle_sequence
        );
        assert_eq!(
            actual_presentation.total_row_count,
            expected_presentation.total_row_count
        );
        assert_eq!(
            actual_presentation.running_row_count,
            expected_presentation.running_row_count
        );
        assert_eq!(
            actual_presentation.finished_ok_row_count,
            expected_presentation.finished_ok_row_count
        );
        assert_eq!(
            actual_presentation.finished_error_row_count,
            expected_presentation.finished_error_row_count
        );
        assert_eq!(
            without_elapsed(actual.lifecycle_diagnostic_snapshot()),
            without_elapsed(expected.lifecycle_diagnostic_snapshot())
        );
    }

    #[test]
    fn observer_queue_pressure_and_disconnect_cannot_change_projection_or_ring_reads() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let full_count = Arc::new(AtomicUsize::new(0));
        let pressured_observer = ActivityLifecycleDiagnosticObserver::new({
            let full_count = Arc::clone(&full_count);
            move |event| {
                if matches!(sender.try_send(event.sequence), Err(TrySendError::Full(_))) {
                    full_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        let (sender, receiver) = mpsc::sync_channel::<u64>(1);
        drop(receiver);
        let disconnected_count = Arc::new(AtomicUsize::new(0));
        let disconnected_observer = ActivityLifecycleDiagnosticObserver::new({
            let disconnected_count = Arc::clone(&disconnected_count);
            move |event| {
                if matches!(
                    sender.try_send(event.sequence),
                    Err(TrySendError::Disconnected(_))
                ) {
                    disconnected_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        let mut baseline = ToolActivityProjection::default();
        let mut pressured = ToolActivityProjection::with_activity_lifecycle_diagnostic_observer(
            Some(pressured_observer),
        );
        let mut disconnected = ToolActivityProjection::with_activity_lifecycle_diagnostic_observer(
            Some(disconnected_observer),
        );

        apply_command_lifecycle(&mut baseline);
        apply_command_lifecycle(&mut pressured);
        apply_command_lifecycle(&mut disconnected);

        assert_eq!(full_count.load(Ordering::Relaxed), 1);
        assert_eq!(disconnected_count.load(Ordering::Relaxed), 2);
        assert_eq!(baseline.rows().len(), 1);
        assert_eq!(
            baseline.rows()[0].status,
            ToolActivityRowStatus::FinishedError
        );
        assert_eq!(baseline.presentation_diagnostic_state().revision, 2);
        assert_projection_diagnostics_equal(&pressured, &baseline);
        assert_projection_diagnostics_equal(&disconnected, &baseline);
    }
}
