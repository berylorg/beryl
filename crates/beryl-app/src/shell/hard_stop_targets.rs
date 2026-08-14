use std::collections::HashMap;

use beryl_backend::{
    HardStopCapabilities, HardStopTarget, ThreadStatus, ToolActivityEvent, ToolActivityLifecycle,
    ToolActivitySource, TurnStreamEvent,
};

use super::status_line::{
    CancellableActiveTurn, CancellableActiveTurnKind, HardStopLimitation,
    SelectedTurnHardStopTargets,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct HardStopTargetProjection {
    capabilities: HardStopCapabilities,
    child_ownership_by_thread: HashMap<String, ChildThreadOwnership>,
    active_turn_by_thread: HashMap<String, String>,
    active_command_executions: HashMap<CommandExecutionKey, CommandExecutionHandle>,
    command_executions_without_process_handle: HashMap<CommandExecutionKey, ()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChildThreadOwnership {
    parent_thread_id: String,
    parent_turn_id: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct CommandExecutionKey {
    thread_id: String,
    turn_id: String,
    item_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommandExecutionHandle {
    process_id: String,
}

impl HardStopTargetProjection {
    pub(super) fn set_capabilities(&mut self, capabilities: HardStopCapabilities) -> bool {
        if self.capabilities == capabilities {
            return false;
        }
        self.capabilities = capabilities;
        true
    }

    pub(super) fn selected_turn_targets(
        &self,
        selected_turn: Option<&CancellableActiveTurn>,
    ) -> Option<SelectedTurnHardStopTargets> {
        let selected_turn = selected_turn?.clone();
        let mut targets = Vec::new();
        let mut limitations = Vec::new();
        push_unique_target(
            &mut targets,
            HardStopTarget::turn(
                selected_turn.thread_id.clone(),
                selected_turn.turn_id.clone(),
            ),
        );

        let associated_threads = self.associated_thread_ids(&selected_turn);
        for thread_id in associated_threads.iter().skip(1) {
            if let Some(turn_id) = self.active_turn_by_thread.get(thread_id) {
                push_unique_target(
                    &mut targets,
                    HardStopTarget::turn(thread_id.clone(), turn_id.clone()),
                );
            }
        }

        self.collect_command_execution_targets(&associated_threads, &mut targets, &mut limitations);
        self.collect_background_terminal_targets(
            &associated_threads,
            &mut targets,
            &mut limitations,
        );

        Some(SelectedTurnHardStopTargets::new(
            selected_turn,
            targets,
            limitations,
        ))
    }

    pub(super) fn apply_stream_event(&mut self, event: &TurnStreamEvent) -> bool {
        if let Some(activity) = event.tool_activity() {
            return self.apply_tool_activity(&activity);
        }

        match event {
            TurnStreamEvent::TurnStarted { thread_id, turn } => {
                self.set_active_turn(thread_id, turn.id.as_str())
            }
            TurnStreamEvent::TurnCompleted { thread_id, turn } => {
                self.finish_turn(thread_id, &turn.id)
            }
            TurnStreamEvent::ThreadStatusChanged { thread_id, status }
                if matches!(status, ThreadStatus::Idle) =>
            {
                self.finish_thread(thread_id)
            }
            TurnStreamEvent::ThreadClosed { thread_id } => self.finish_thread(thread_id),
            TurnStreamEvent::ProtocolError { .. } => self.clear_active_targets(),
            _ => false,
        }
    }

    pub(super) fn clear_all(&mut self) -> bool {
        let changed = !self.child_ownership_by_thread.is_empty()
            || !self.active_turn_by_thread.is_empty()
            || !self.active_command_executions.is_empty()
            || !self.command_executions_without_process_handle.is_empty();
        self.child_ownership_by_thread.clear();
        self.active_turn_by_thread.clear();
        self.active_command_executions.clear();
        self.command_executions_without_process_handle.clear();
        changed
    }

    fn apply_tool_activity(&mut self, activity: &ToolActivityEvent) -> bool {
        let mut changed = self.apply_child_thread_ownership(activity);
        if activity.source == ToolActivitySource::CommandExecution {
            changed |= self.apply_command_execution_activity(activity);
        }
        changed
    }

    fn apply_child_thread_ownership(&mut self, activity: &ToolActivityEvent) -> bool {
        if activity.source != ToolActivitySource::CollabAgentToolCall {
            return false;
        }
        let Some(parent_thread_id) = non_empty(activity.thread_id.as_str()) else {
            return false;
        };
        let Some(parent_turn_id) = non_empty(activity.turn_id.as_str()) else {
            return false;
        };

        let ownership = ChildThreadOwnership {
            parent_thread_id: parent_thread_id.to_string(),
            parent_turn_id: parent_turn_id.to_string(),
        };
        let mut changed = false;
        for child_thread_id in &activity.receiver_thread_ids {
            let Some(child_thread_id) = non_empty(child_thread_id.as_str()) else {
                continue;
            };
            if child_thread_id == parent_thread_id {
                continue;
            }
            if self.child_ownership_by_thread.get(child_thread_id) == Some(&ownership) {
                continue;
            }
            self.child_ownership_by_thread
                .insert(child_thread_id.to_string(), ownership.clone());
            changed = true;
        }
        changed
    }

    fn apply_command_execution_activity(&mut self, activity: &ToolActivityEvent) -> bool {
        let Some(key) = CommandExecutionKey::from_activity(activity) else {
            return false;
        };

        match activity.lifecycle {
            ToolActivityLifecycle::Started => {
                self.command_executions_without_process_handle.remove(&key);
                if let Some(process_id) = activity
                    .command_exec_process_id
                    .as_deref()
                    .and_then(non_empty)
                {
                    let handle = CommandExecutionHandle {
                        process_id: process_id.to_string(),
                    };
                    let changed = self.active_command_executions.get(&key) != Some(&handle);
                    self.active_command_executions.insert(key, handle);
                    changed
                } else {
                    let changed = self.active_command_executions.remove(&key).is_some()
                        || !self
                            .command_executions_without_process_handle
                            .contains_key(&key);
                    self.command_executions_without_process_handle
                        .insert(key, ());
                    changed
                }
            }
            ToolActivityLifecycle::Completed => {
                self.active_command_executions.remove(&key).is_some()
                    | self
                        .command_executions_without_process_handle
                        .remove(&key)
                        .is_some()
            }
            ToolActivityLifecycle::Updated => false,
        }
    }

    fn set_active_turn(&mut self, thread_id: &str, turn_id: &str) -> bool {
        let Some(thread_id) = non_empty(thread_id) else {
            return false;
        };
        let Some(turn_id) = non_empty(turn_id) else {
            return false;
        };
        if self
            .active_turn_by_thread
            .get(thread_id)
            .map(String::as_str)
            == Some(turn_id)
        {
            return false;
        }
        self.active_turn_by_thread
            .insert(thread_id.to_string(), turn_id.to_string());
        true
    }

    pub(super) fn finish_turn(&mut self, thread_id: &str, turn_id: &str) -> bool {
        let mut changed = false;
        if self
            .active_turn_by_thread
            .get(thread_id)
            .map(String::as_str)
            == Some(turn_id)
        {
            self.active_turn_by_thread.remove(thread_id);
            changed = true;
        }
        changed |= self.remove_command_executions_for_turn(thread_id, turn_id);
        changed |= self.remove_child_ownership_for_parent_turn(thread_id, turn_id);
        changed
    }

    pub(super) fn finish_thread(&mut self, thread_id: &str) -> bool {
        let mut changed = self.active_turn_by_thread.remove(thread_id).is_some();
        changed |= self.remove_command_executions_for_thread(thread_id);
        changed |= self.child_ownership_by_thread.remove(thread_id).is_some();
        let before = self.child_ownership_by_thread.len();
        self.child_ownership_by_thread
            .retain(|_, ownership| ownership.parent_thread_id != thread_id);
        changed | (self.child_ownership_by_thread.len() != before)
    }

    fn clear_active_targets(&mut self) -> bool {
        self.clear_all()
    }

    fn associated_thread_ids(&self, selected_turn: &CancellableActiveTurn) -> Vec<String> {
        let mut thread_ids = vec![selected_turn.thread_id.clone()];
        if selected_turn.kind != CancellableActiveTurnKind::Ordinary {
            return thread_ids;
        }

        let mut children = self
            .child_ownership_by_thread
            .iter()
            .filter_map(|(child_thread_id, ownership)| {
                (ownership.parent_thread_id == selected_turn.thread_id
                    && ownership.parent_turn_id == selected_turn.turn_id)
                    .then(|| child_thread_id.clone())
            })
            .collect::<Vec<_>>();
        children.sort();
        for child_thread_id in children {
            if !thread_ids.contains(&child_thread_id) {
                thread_ids.push(child_thread_id);
            }
        }
        thread_ids
    }

    fn collect_command_execution_targets(
        &self,
        associated_threads: &[String],
        targets: &mut Vec<HardStopTarget>,
        limitations: &mut Vec<HardStopLimitation>,
    ) {
        for (key, handle) in &self.active_command_executions {
            if !associated_threads.contains(&key.thread_id) {
                continue;
            }
            if self.capabilities.command_exec_terminate() {
                push_unique_target(
                    targets,
                    HardStopTarget::command_execution(handle.process_id.clone()),
                );
            } else {
                limitations.push(HardStopLimitation::CommandExecutionTerminateUnsupported {
                    process_id: handle.process_id.clone(),
                });
            }
        }

        for key in self.command_executions_without_process_handle.keys() {
            if associated_threads.contains(&key.thread_id) {
                limitations.push(
                    HardStopLimitation::CommandExecutionProcessHandleUnavailable {
                        thread_id: key.thread_id.clone(),
                        turn_id: key.turn_id.clone(),
                        item_id: key.item_id.clone(),
                    },
                );
            }
        }
    }

    fn collect_background_terminal_targets(
        &self,
        associated_threads: &[String],
        targets: &mut Vec<HardStopTarget>,
        limitations: &mut Vec<HardStopLimitation>,
    ) {
        for thread_id in associated_threads {
            if self.capabilities.thread_background_terminals_clean() {
                push_unique_target(
                    targets,
                    HardStopTarget::background_terminals(thread_id.clone()),
                );
            } else {
                limitations.push(HardStopLimitation::BackgroundTerminalCleanupUnsupported {
                    thread_id: thread_id.clone(),
                });
            }
        }
    }

    fn remove_child_ownership_for_parent_turn(&mut self, thread_id: &str, turn_id: &str) -> bool {
        let before = self.child_ownership_by_thread.len();
        self.child_ownership_by_thread.retain(|_, ownership| {
            ownership.parent_thread_id != thread_id || ownership.parent_turn_id != turn_id
        });
        self.child_ownership_by_thread.len() != before
    }

    fn remove_command_executions_for_turn(&mut self, thread_id: &str, turn_id: &str) -> bool {
        let before_active = self.active_command_executions.len();
        self.active_command_executions
            .retain(|key, _| key.thread_id != thread_id || key.turn_id != turn_id);
        let before_missing = self.command_executions_without_process_handle.len();
        self.command_executions_without_process_handle
            .retain(|key, _| key.thread_id != thread_id || key.turn_id != turn_id);
        self.active_command_executions.len() != before_active
            || self.command_executions_without_process_handle.len() != before_missing
    }

    fn remove_command_executions_for_thread(&mut self, thread_id: &str) -> bool {
        let before_active = self.active_command_executions.len();
        self.active_command_executions
            .retain(|key, _| key.thread_id != thread_id);
        let before_missing = self.command_executions_without_process_handle.len();
        self.command_executions_without_process_handle
            .retain(|key, _| key.thread_id != thread_id);
        self.active_command_executions.len() != before_active
            || self.command_executions_without_process_handle.len() != before_missing
    }
}

impl CommandExecutionKey {
    fn from_activity(activity: &ToolActivityEvent) -> Option<Self> {
        Some(Self {
            thread_id: non_empty(activity.thread_id.as_str())?.to_string(),
            turn_id: non_empty(activity.turn_id.as_str())?.to_string(),
            item_id: non_empty(activity.item_id.as_str())?.to_string(),
        })
    }
}

fn push_unique_target(targets: &mut Vec<HardStopTarget>, target: HardStopTarget) {
    if !targets.contains(&target) {
        targets.push(target);
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}
