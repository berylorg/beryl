use std::{collections::HashMap, ops::Range, path::Path};

use beryl_backend::{
    AgentMessageItem, ProtocolPhase, ThreadItem, ThreadReadMetadata, ThreadSessionMetadata,
    ThreadSummary, ToolActivityCollabAgentSpawnMetadata, ToolActivityEvent,
    ToolActivityFileChangeSummary, ToolActivityLifecycle, ToolActivitySource, TurnStatus,
    TurnStreamEvent,
};
use beryl_model::workspace::{RuntimeMode, WorkspaceId};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ToolActivityProjection {
    records: Vec<ToolActivityRecord>,
    rows: Vec<ToolActivityRow>,
    agent_labels_by_thread: HashMap<String, AgentLabel>,
    runtime_metadata_by_subagent_thread: HashMap<String, SubagentRuntimeMetadata>,
    parent_thread_by_child: HashMap<String, String>,
    visible_row_indexes_by_thread: HashMap<String, Vec<usize>>,
    next_start_order: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ToolActivityRow {
    key: ToolActivityKey,
    pub(super) agent_label: String,
    pub(super) tool_display_value: String,
    pub(super) status: ToolActivityRowStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolActivityRowStatus {
    Running,
    FinishedOk,
    FinishedError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ToolActivitySubagentMetadataTarget {
    pub(super) thread_id: String,
    pub(super) requires_nickname: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolActivityRecord {
    key: ToolActivityKey,
    source: ToolActivityRecordSource,
    explicit_agent_label: Option<String>,
    tool_display_value: String,
    status: ToolActivityRowStatus,
    start_order: u64,
    reasoning_summary_parts: Vec<String>,
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
struct AgentLabel {
    value: String,
    priority: AgentLabelPriority,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SubagentRuntimeMetadata {
    model: Option<String>,
    reasoning_effort: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum AgentLabelPriority {
    ActivityMetadata,
    ThreadDisplayLabel,
    ThreadMetadataNickname,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ReceiverThreadOwnershipChange {
    changed: bool,
    requires_row_rebuild: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ToolActivityRetainedCounts {
    pub(super) records: usize,
    pub(super) rows: usize,
    pub(super) visible_thread_indexes: usize,
    pub(super) payload_bytes: usize,
}

impl Default for ToolActivityProjection {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            rows: Vec::new(),
            agent_labels_by_thread: HashMap::new(),
            runtime_metadata_by_subagent_thread: HashMap::new(),
            parent_thread_by_child: HashMap::new(),
            visible_row_indexes_by_thread: HashMap::new(),
            next_start_order: 0,
        }
    }
}

impl ToolActivityProjection {
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

    pub(super) fn retained_counts(&self) -> ToolActivityRetainedCounts {
        let record_payload_bytes = self
            .records
            .iter()
            .map(|record| {
                record.key.thread_id.len()
                    + record.key.turn_id.len()
                    + record.key.item_id.len()
                    + record.explicit_agent_label.as_ref().map_or(0, String::len)
                    + record.tool_display_value.len()
                    + record
                        .reasoning_summary_parts
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
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
            .map(|(thread_id, label)| thread_id.len() + label.value.len())
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
        let parent_payload_bytes = self
            .parent_thread_by_child
            .iter()
            .map(|(child, parent)| child.len() + parent.len())
            .sum::<usize>();

        ToolActivityRetainedCounts {
            records: self.records.len(),
            rows: self.rows.len(),
            visible_thread_indexes: self
                .visible_row_indexes_by_thread
                .values()
                .map(Vec::len)
                .sum(),
            payload_bytes: record_payload_bytes
                .saturating_add(row_payload_bytes)
                .saturating_add(label_payload_bytes)
                .saturating_add(metadata_payload_bytes)
                .saturating_add(parent_payload_bytes),
        }
    }

    #[allow(dead_code)]
    pub(super) fn unresolved_subagent_thread_ids(&self) -> Vec<String> {
        let mut thread_ids = self
            .parent_thread_by_child
            .keys()
            .filter(|thread_id| !self.has_resolved_subagent_label(thread_id))
            .cloned()
            .collect::<Vec<_>>();
        thread_ids.sort();
        thread_ids
    }

    pub(super) fn subagent_metadata_resolution_targets(
        &self,
    ) -> Vec<ToolActivitySubagentMetadataTarget> {
        let mut targets = self
            .parent_thread_by_child
            .keys()
            .filter_map(|thread_id| {
                let requires_nickname = !self.has_resolved_subagent_label(thread_id);
                let requires_runtime_metadata = !self
                    .runtime_metadata_by_subagent_thread
                    .contains_key(thread_id);
                (requires_nickname || requires_runtime_metadata).then(|| {
                    ToolActivitySubagentMetadataTarget {
                        thread_id: thread_id.clone(),
                        requires_nickname,
                    }
                })
            })
            .collect::<Vec<_>>();
        targets.sort_by(|left, right| left.thread_id.cmp(&right.thread_id));
        targets
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
            TurnStreamEvent::ThreadStarted { thread } => self
                .apply_thread_agent_nickname(thread.id.as_str(), thread.agent_nickname.as_deref()),
            TurnStreamEvent::AgentLabelUpdated { thread_id, label } => {
                self.apply_thread_agent_nickname(thread_id.as_str(), Some(label.as_str()))
            }
            TurnStreamEvent::TurnCompleted { thread_id, turn } => {
                match final_status_from_turn_status(turn.status) {
                    Some(status) => self.finish_running_for_turn(thread_id, &turn.id, status),
                    None => false,
                }
            }
            TurnStreamEvent::ThreadClosed { thread_id } => {
                self.finish_running_for_thread(thread_id, ToolActivityRowStatus::FinishedOk)
            }
            TurnStreamEvent::ProtocolError { .. } => {
                self.finish_all_running(ToolActivityRowStatus::FinishedError)
            }
            _ => false,
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
            || !self.parent_thread_by_child.is_empty()
            || !self.visible_row_indexes_by_thread.is_empty();
        self.records.clear();
        self.rows.clear();
        self.agent_labels_by_thread.clear();
        self.runtime_metadata_by_subagent_thread.clear();
        self.parent_thread_by_child.clear();
        self.visible_row_indexes_by_thread.clear();
        changed
    }

    pub(super) fn finish_running_for_thread(
        &mut self,
        thread_id: &str,
        status: ToolActivityRowStatus,
    ) -> bool {
        let mut changed = false;
        for record in &mut self.records {
            if record.key.thread_id == thread_id && record.status == ToolActivityRowStatus::Running
            {
                record.status = status;
                changed = true;
            }
        }
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    fn apply_tool_activity(
        &mut self,
        activity: ToolActivityEvent,
        agent_label: Option<String>,
        execution_target: Option<&WorkspaceId>,
    ) -> bool {
        let ownership_changed = self.apply_receiver_thread_ownership_updates(&activity);
        let labels_changed = self.apply_agent_label_updates(&activity);
        let explicit_agent_label = explicit_agent_label_for_activity(&activity, agent_label);
        let key = ToolActivityKey::from_activity(&activity);
        let activity_changed = match activity.lifecycle {
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
        if ownership_changed.changed && !labels_changed && !activity_changed {
            if ownership_changed.requires_row_rebuild {
                self.rebuild_rows();
            } else {
                self.rebuild_visible_row_indexes();
            }
        }
        ownership_changed.changed || labels_changed || activity_changed
    }

    fn start_activity(
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
            if existing.status != ToolActivityRowStatus::Running {
                existing.status = ToolActivityRowStatus::Running;
                changed = true;
            }
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

    fn finish_running_for_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        status: ToolActivityRowStatus,
    ) -> bool {
        let mut changed = false;
        for record in &mut self.records {
            if record.key.thread_id == thread_id
                && record.key.turn_id == turn_id
                && record.status == ToolActivityRowStatus::Running
            {
                record.status = status;
                changed = true;
            }
        }
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    fn finish_all_running(&mut self, status: ToolActivityRowStatus) -> bool {
        let mut changed = false;
        for record in &mut self.records {
            if record.status == ToolActivityRowStatus::Running {
                record.status = status;
                changed = true;
            }
        }
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    fn finish_or_insert_completed(
        &mut self,
        key: ToolActivityKey,
        activity: ToolActivityEvent,
        explicit_agent_label: Option<String>,
        status: ToolActivityRowStatus,
        execution_target: Option<&WorkspaceId>,
    ) -> bool {
        if let Some(existing) = self.records.iter_mut().find(|existing| existing.key == key) {
            let mut changed = false;
            let source = ToolActivityRecordSource::from(activity.source);
            if existing.source != source {
                existing.source = source;
                changed = true;
            }
            if existing.status != status {
                existing.status = status;
                changed = true;
            }
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

        if let Some(existing) = self.records.iter_mut().find(|existing| existing.key == key) {
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

        let record = ToolActivityRecord {
            key,
            source: ToolActivityRecordSource::SubagentHandoff,
            explicit_agent_label: None,
            tool_display_value,
            status: ToolActivityRowStatus::FinishedOk,
            start_order: self.next_start_order(),
            reasoning_summary_parts: Vec::new(),
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
        let mut record = ToolActivityRecord {
            source: ToolActivityRecordSource::from(activity.source),
            explicit_agent_label,
            tool_display_value: tool_activity_display_value(&activity, execution_target),
            status,
            start_order: self.next_start_order(),
            reasoning_summary_parts: Vec::new(),
            key,
        };
        apply_reasoning_summary_detail(&mut record, &activity);
        record.tool_display_value =
            activity_display_value_for_record(&record, &activity, execution_target);
        record
    }

    fn apply_agent_label_updates(&mut self, activity: &ToolActivityEvent) -> bool {
        let mut changed = false;
        for update in &activity.agent_label_updates {
            changed |= self.note_agent_label(
                update.thread_id.as_str(),
                Some(update.label.as_str()),
                AgentLabelPriority::ActivityMetadata,
            );
        }
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    fn apply_receiver_thread_ownership_updates(
        &mut self,
        activity: &ToolActivityEvent,
    ) -> ReceiverThreadOwnershipChange {
        if activity.source != ToolActivitySource::CollabAgentToolCall {
            return ReceiverThreadOwnershipChange::default();
        }

        let Some(parent_thread_id) = non_empty_trimmed_str(activity.thread_id.as_str()) else {
            return ReceiverThreadOwnershipChange::default();
        };

        let mut change = ReceiverThreadOwnershipChange::default();
        for receiver_thread_id in &activity.receiver_thread_ids {
            let Some(receiver_thread_id) = non_empty_trimmed_str(receiver_thread_id.as_str())
            else {
                continue;
            };
            if receiver_thread_id == parent_thread_id {
                continue;
            }
            let previous = self
                .parent_thread_by_child
                .insert(receiver_thread_id.to_string(), parent_thread_id.to_string());
            if previous.as_deref() != Some(parent_thread_id) {
                change.changed = true;
            }
            if self
                .agent_labels_by_thread
                .get(receiver_thread_id)
                .is_some_and(|label| label.priority == AgentLabelPriority::ThreadDisplayLabel)
            {
                change.requires_row_rebuild = true;
            }
            if self.note_activity_subagent_runtime_metadata(
                receiver_thread_id,
                activity.collab_agent_spawn_metadata.as_ref(),
            ) {
                change.changed = true;
                change.requires_row_rebuild = true;
            }
        }
        change
    }

    fn apply_thread_agent_nickname(
        &mut self,
        thread_id: &str,
        agent_nickname: Option<&str>,
    ) -> bool {
        let changed = self.note_agent_label(
            thread_id,
            agent_nickname,
            AgentLabelPriority::ThreadMetadataNickname,
        );
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    fn note_thread_summary_agent_labels(&mut self, thread: &ThreadSummary) -> bool {
        let mut changed = self.note_agent_label(
            thread.id.as_str(),
            thread.agent_nickname.as_deref(),
            AgentLabelPriority::ThreadMetadataNickname,
        );
        if !self.is_observed_subagent_thread(&thread.id) {
            changed |= self.note_agent_label(
                thread.id.as_str(),
                thread.name.as_deref().or(Some(thread.preview.as_str())),
                AgentLabelPriority::ThreadDisplayLabel,
            );
        }
        changed
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
        let Some(thread_id) = non_empty_trimmed_str(thread_id) else {
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

    fn note_agent_label(
        &mut self,
        thread_id: &str,
        label: Option<&str>,
        priority: AgentLabelPriority,
    ) -> bool {
        let thread_id = thread_id.trim();
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

        if let Some(previous) = self.agent_labels_by_thread.get(thread_id)
            && (previous.priority > priority
                || (previous.priority == priority && previous.value == label))
        {
            return false;
        }

        self.agent_labels_by_thread.insert(
            thread_id.to_string(),
            AgentLabel {
                value: label.to_string(),
                priority,
            },
        );
        true
    }

    fn rebuild_rows_if_len_changed(&mut self, before_len: usize) -> bool {
        let changed = self.records.len() != before_len;
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    fn rebuild_rows(&mut self) {
        let mut records = self.records.clone();
        records.sort_by(|left, right| {
            left.status
                .sort_rank()
                .cmp(&right.status.sort_rank())
                .then_with(|| right.start_order.cmp(&left.start_order))
                .then_with(|| left.key.thread_id.cmp(&right.key.thread_id))
                .then_with(|| left.key.turn_id.cmp(&right.key.turn_id))
                .then_with(|| left.key.item_id.cmp(&right.key.item_id))
        });
        self.rows = records
            .into_iter()
            .map(|record| ToolActivityRow {
                agent_label: self.agent_label_for_record(&record),
                key: record.key,
                tool_display_value: record.tool_display_value,
                status: record.status,
            })
            .collect();
        self.rebuild_visible_row_indexes();
    }

    fn next_start_order(&mut self) -> u64 {
        let order = self.next_start_order;
        self.next_start_order = self.next_start_order.saturating_add(1);
        order
    }

    fn visible_row_indexes_for_selected_thread(
        &self,
        selected_thread_id: Option<&str>,
    ) -> Option<&[usize]> {
        let selected_thread_id = selected_thread_id.and_then(non_empty_trimmed_str)?;
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
        if record.explicit_agent_label.as_deref() == Some("Main") {
            return "Main".to_string();
        }

        let thread_id = record.key.thread_id.as_str();
        let stored_label = self.agent_labels_by_thread.get(thread_id);
        if let Some(stored_label) = stored_label
            && stored_label.priority == AgentLabelPriority::ThreadMetadataNickname
        {
            return self.display_agent_label_for_thread(thread_id, &stored_label.value);
        }
        if let Some(explicit_agent_label) = record.explicit_agent_label.as_ref() {
            return self.display_agent_label_for_thread(thread_id, explicit_agent_label);
        }
        if let Some(stored_label) = stored_label {
            match stored_label.priority {
                AgentLabelPriority::ActivityMetadata => {
                    return self.display_agent_label_for_thread(thread_id, &stored_label.value);
                }
                AgentLabelPriority::ThreadDisplayLabel
                    if !self.is_observed_subagent_thread(thread_id) =>
                {
                    return stored_label.value.clone();
                }
                AgentLabelPriority::ThreadDisplayLabel
                | AgentLabelPriority::ThreadMetadataNickname => {}
            }
        }

        String::new()
    }

    fn display_agent_label_for_thread(&self, thread_id: &str, label: &str) -> String {
        if !self.is_observed_subagent_thread(thread_id) {
            return label.to_string();
        }
        let Some(metadata) = self.runtime_metadata_by_subagent_thread.get(thread_id) else {
            return label.to_string();
        };
        format_subagent_agent_label(label, metadata)
    }

    fn has_resolved_subagent_label(&self, thread_id: &str) -> bool {
        self.agent_labels_by_thread
            .get(thread_id)
            .is_some_and(|label| {
                matches!(
                    label.priority,
                    AgentLabelPriority::ActivityMetadata
                        | AgentLabelPriority::ThreadMetadataNickname
                )
            })
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

impl ToolActivityRowStatus {
    fn sort_rank(self) -> u8 {
        match self {
            Self::Running => 0,
            Self::FinishedOk | Self::FinishedError => 1,
        }
    }
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
    if let Some(reasoning_effort) = metadata.reasoning_effort.as_deref() {
        format!("{label} ({model}/{reasoning_effort})")
    } else {
        format!("{label} ({model})")
    }
}

fn is_fallback_agent_label_for_thread(label: &str, thread_id: &str) -> bool {
    label.trim() == fallback_agent_label(thread_id)
}

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
    if activity.source == ToolActivitySource::Reasoning {
        return reasoning_activity_display_value(
            activity
                .reasoning_summary_text
                .as_deref()
                .unwrap_or_default(),
        );
    }

    if activity.source == ToolActivitySource::CommandExecution {
        if let Some(command_line) = first_non_empty_command_line(activity.raw_command.as_deref()) {
            return command_execution_display_line(command_line);
        }
    }

    if activity.source == ToolActivitySource::FileChange
        && let Some(summary) = activity.file_change_summary.as_ref()
    {
        return file_change_display_value(summary, execution_target);
    }

    activity
        .raw_tool_name
        .as_deref()
        .or(activity.raw_resource_uri.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| activity.item_type.clone())
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

fn activity_display_value_for_record(
    record: &ToolActivityRecord,
    activity: &ToolActivityEvent,
    execution_target: Option<&WorkspaceId>,
) -> String {
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
        let replacement = vec![summary_text.clone()];
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

    let mut changed =
        ensure_reasoning_summary_slot(&mut record.reasoning_summary_parts, summary_index);
    if let Some(delta) = activity.reasoning_summary_delta.as_ref()
        && !delta.is_empty()
    {
        record.reasoning_summary_parts[summary_index].push_str(delta);
        changed = true;
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
        .map(str::to_string)
}

fn explicit_agent_label_for_activity(
    activity: &ToolActivityEvent,
    explicit_agent_label: Option<String>,
) -> Option<String> {
    let explicit_agent_label = explicit_agent_label.and_then(non_empty_trimmed_string)?;
    if explicit_agent_label == "Main"
        || !is_fallback_agent_label_for_thread(&explicit_agent_label, &activity.thread_id)
    {
        return Some(explicit_agent_label);
    }
    None
}

fn non_empty_trimmed_str(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
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

fn final_status_from_turn_status(status: TurnStatus) -> Option<ToolActivityRowStatus> {
    match status {
        TurnStatus::Completed => Some(ToolActivityRowStatus::FinishedOk),
        TurnStatus::Interrupted | TurnStatus::Failed => Some(ToolActivityRowStatus::FinishedError),
        TurnStatus::InProgress => None,
    }
}
