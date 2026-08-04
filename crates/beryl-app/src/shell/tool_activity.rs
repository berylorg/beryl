use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use beryl_backend::ThreadReadMetadata;

#[derive(Clone, Debug)]
pub(super) struct ToolActivityProjection {
    records: Vec<ToolActivityRecord>,
    rows: Vec<ToolActivityRow>,
    agent_labels_by_thread: HashMap<String, AgentLabel>,
    runtime_metadata_by_subagent_thread: HashMap<String, SubagentRuntimeMetadata>,
    parent_thread_by_child: HashMap<String, String>,
    root_turn_by_child_thread: HashMap<String, ToolActivityRootTurnKey>,
    visible_row_indexes_by_thread: HashMap<String, Vec<usize>>,
    last_selected_thread_id: Option<String>,
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
    explicit_agent_label: Option<String>,
    tool_display_value: String,
    status: ToolActivityRowStatus,
    start_order: u64,
    reasoning_summary_parts: Vec<String>,
    receiver_thread_ids: Vec<String>,
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
    ThreadMetadataNickname,
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
        Self {
            records: Vec::new(),
            rows: Vec::new(),
            agent_labels_by_thread: HashMap::new(),
            runtime_metadata_by_subagent_thread: HashMap::new(),
            parent_thread_by_child: HashMap::new(),
            root_turn_by_child_thread: HashMap::new(),
            visible_row_indexes_by_thread: HashMap::new(),
            last_selected_thread_id: None,
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
                .saturating_add(parent_payload_bytes)
                .saturating_add(root_turn_payload_bytes)
                .saturating_add(visible_thread_index_key_bytes)
                .saturating_add(visible_thread_index_bytes)
                .saturating_add(selected_thread_id_bytes),
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

    pub(super) fn set_selected_thread_id(&mut self, selected_thread_id: Option<&str>) -> bool {
        let selected_thread_id = selected_thread_id
            .and_then(non_empty_trimmed_str)
            .map(str::to_string);
        if self.last_selected_thread_id == selected_thread_id {
            return false;
        }
        self.last_selected_thread_id = selected_thread_id;
        true
    }

    pub(super) fn apply_protocol_error(&mut self) -> bool {
        self.finish_all_running(ToolActivityRowStatus::FinishedError)
    }

    pub(super) fn apply_thread_read_metadata<'a>(
        &mut self,
        metadata: impl IntoIterator<Item = &'a ThreadReadMetadata>,
    ) -> bool {
        let mut changed = false;
        for metadata in metadata {
            let thread_id = metadata.thread_id().as_str();
            if !self.is_observed_subagent_thread(thread_id) {
                continue;
            }
            changed |= self.note_agent_label(
                thread_id,
                metadata.agent_nickname(),
                AgentLabelPriority::ThreadMetadataNickname,
            );
            changed |= self.note_subagent_runtime_metadata_values(thread_id, None, None, true);
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
            || !self.root_turn_by_child_thread.is_empty()
            || !self.visible_row_indexes_by_thread.is_empty()
            || self.last_selected_thread_id.is_some();
        self.records.clear();
        self.rows.clear();
        self.agent_labels_by_thread.clear();
        self.runtime_metadata_by_subagent_thread.clear();
        self.parent_thread_by_child.clear();
        self.root_turn_by_child_thread.clear();
        self.visible_row_indexes_by_thread.clear();
        self.last_selected_thread_id.take();
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

    pub(super) fn finish_running_for_turn(
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
        let label = truncate_label_payload(label);

        if let Some(previous) = self.agent_labels_by_thread.get(thread_id)
            && (previous.priority > priority
                || (previous.priority == priority && previous.value == label))
        {
            return false;
        }

        self.agent_labels_by_thread.insert(
            thread_id.to_string(),
            AgentLabel {
                value: label,
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

        for (index, record) in self.records.iter().enumerate() {
            if record.status == ToolActivityRowStatus::Running || protected_indexes.contains(&index)
            {
                keep[index] = true;
                if record.status != ToolActivityRowStatus::Running {
                    retained_completed_rows = retained_completed_rows.saturating_add(1);
                    retained_completed_bytes = retained_completed_bytes
                        .saturating_add(completed_record_display_payload_bytes(record));
                }
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
            let left_latest = latest_start_order(&self.records, left_indexes);
            let right_latest = latest_start_order(&self.records, right_indexes);
            right_latest
                .cmp(&left_latest)
                .then_with(|| left_key.cmp(right_key))
        });

        for (_, mut indexes) in groups {
            indexes.sort_by(|left, right| {
                self.records[*right]
                    .start_order
                    .cmp(&self.records[*left].start_order)
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
            let group_bytes = indexes
                .iter()
                .map(|index| completed_record_display_payload_bytes(&self.records[*index]))
                .sum::<usize>();
            if retained_completed_rows.saturating_add(group_rows) <= ACTIVITY_COMPLETED_ROW_BUDGET
                && retained_completed_bytes.saturating_add(group_bytes)
                    <= ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET
            {
                for index in indexes {
                    keep[index] = true;
                }
                retained_completed_rows = retained_completed_rows.saturating_add(group_rows);
                retained_completed_bytes = retained_completed_bytes.saturating_add(group_bytes);
                continue;
            }

            for index in indexes {
                if retained_completed_rows >= ACTIVITY_COMPLETED_ROW_BUDGET {
                    break;
                }
                let record_bytes = completed_record_display_payload_bytes(&self.records[index]);
                if retained_completed_bytes.saturating_add(record_bytes)
                    > ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET
                {
                    continue;
                }
                keep[index] = true;
                retained_completed_rows = retained_completed_rows.saturating_add(1);
                retained_completed_bytes = retained_completed_bytes.saturating_add(record_bytes);
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
        self.records
            .iter()
            .filter(|record| record.status != ToolActivityRowStatus::Running)
            .map(completed_record_display_payload_bytes)
            .sum()
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
            .map(|(index, record)| (index, record.start_order))
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
        let mut active_parent_threads = HashSet::new();
        let mut retained_record_child_links = HashSet::new();

        for record in &self.records {
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
            if record.status == ToolActivityRowStatus::Running {
                active_parent_threads.insert(record.key.thread_id.clone());
            }
        }

        self.parent_thread_by_child.retain(|child, parent| {
            let keep = required_child_links.contains(child)
                || retained_record_child_links.contains(child)
                || active_parent_threads.contains(parent);
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
        self.prune_retained_records();
        self.prune_derived_state();
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
            .is_some_and(|label| label.priority == AgentLabelPriority::ThreadMetadataNickname)
    }

    fn is_observed_subagent_thread(&self, thread_id: &str) -> bool {
        self.parent_thread_by_child.contains_key(thread_id)
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

fn latest_start_order(records: &[ToolActivityRecord], indexes: &[usize]) -> u64 {
    indexes
        .iter()
        .map(|index| records[*index].start_order)
        .max()
        .unwrap_or_default()
}

fn completed_record_display_payload_bytes(record: &ToolActivityRecord) -> usize {
    record.explicit_agent_label.as_ref().map_or(0, String::len)
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

pub(super) const ACTIVITY_COMPLETED_ROW_BUDGET: usize = 2_000;
pub(super) const ACTIVITY_COMPLETED_DISPLAY_BYTE_BUDGET: usize = 8 * 1024 * 1024;
pub(super) const ACTIVITY_SELECTED_COMPLETED_ROW_WINDOW: usize = 200;
pub(super) const ACTIVITY_LABEL_DISPLAY_BYTE_LIMIT: usize = 16 * 1024;
fn normalized_optional_metadata_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(truncate_label_payload)
}

fn non_empty_trimmed_str(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}
