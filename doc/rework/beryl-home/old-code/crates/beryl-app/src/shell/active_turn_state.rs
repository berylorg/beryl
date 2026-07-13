use std::sync::Arc;

use beryl_backend::{
    AgentMessageItem, ProtocolPhase, ThreadItem, TurnError, TurnStatus, TurnStreamEvent,
};

use super::{
    syndic_ingestion::SyndicTurnIdentity,
    turn_input::{UserInputFragment, user_input_payload_bytes},
};

const ACTIVE_TURN_RETAINED_SOURCE_MAX_BYTES: usize = 100 * 1024 * 1024;

#[derive(Clone, Default)]
pub(super) struct ActiveTurnState {
    turns: Vec<Arc<TurnExecutionRecord>>,
    active_turn_index: Option<usize>,
}

#[derive(Clone)]
pub(super) struct TurnExecutionRecord {
    pub(super) user_input_fragments: Vec<UserInputFragment>,
    pub(super) narrative_entries: Vec<TurnNarrativeEntry>,
    pub(super) thread_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) syndic_turn: Option<SyndicTurnIdentity>,
    pub(super) status: TurnExecutionStatus,
    pub(super) terminal_assistant_item_id: Option<String>,
    pub(super) error_message: Option<String>,
    pub(super) items: Vec<ExecutionItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TurnNarrativeEntry {
    UserInput { fragment_id: u64 },
    Item { item_id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TurnExecutionStatus {
    Queued,
    Starting,
    Running,
    Completed,
    Interrupted,
    Failed,
}

#[derive(Clone)]
pub(super) enum ExecutionItem {
    AgentMessage(AgentMessageDetail),
}

#[derive(Clone)]
pub(super) struct AgentMessageDetail {
    pub(super) id: String,
    pub(super) phase: Option<ProtocolPhase>,
    pub(super) text: String,
    pub(super) complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActiveTurnIdentity {
    pub(super) turn_index: usize,
    pub(super) thread_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) syndic_turn: Option<SyndicTurnIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LastTurnState {
    Unknown,
    Working,
    Active,
    Ok,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ActiveTurnSourcePinSnapshot {
    pub(super) active: bool,
    pub(super) retained_bytes: usize,
    pub(super) max_retained_bytes: usize,
    pub(super) fallback_active: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ActiveTurnRetainedCounts {
    pub(super) turns: usize,
    pub(super) items: usize,
    pub(super) text_bytes: usize,
    pub(super) user_fragments: usize,
    pub(super) user_fragment_text_bytes: usize,
    pub(super) backend_input_records: usize,
    pub(super) backend_input_bytes: usize,
    pub(super) image_marker_bytes: usize,
    pub(super) narrative_entries: usize,
    pub(super) generated_image_items: usize,
    pub(super) active_turn_payload_bytes: usize,
    pub(super) agent_text_bytes: usize,
    pub(super) reasoning_summary_bytes: usize,
    pub(super) reasoning_content_bytes: usize,
    pub(super) command_text_bytes: usize,
    pub(super) command_output_bytes: usize,
    pub(super) file_change_path_bytes: usize,
    pub(super) file_change_output_bytes: usize,
    pub(super) generated_image_inline_bytes: usize,
    pub(super) generated_image_metadata_bytes: usize,
    pub(super) error_bytes: usize,
    pub(super) identity_bytes: usize,
    pub(super) payload_bytes: usize,
}

impl ActiveTurnState {
    pub(super) fn reset(&mut self) {
        self.turns.clear();
        self.active_turn_index = None;
    }

    pub(super) fn begin_turn_with_thread_fragments(
        &mut self,
        thread_id: Option<String>,
        user_input_fragments: Vec<UserInputFragment>,
        syndic_turn: Option<SyndicTurnIdentity>,
    ) -> usize {
        self.push_turn_with_fragments(
            thread_id,
            user_input_fragments,
            syndic_turn,
            TurnExecutionStatus::Starting,
            true,
        )
    }

    pub(super) fn begin_pending_turn_with_fragments(
        &mut self,
        user_input_fragments: Vec<UserInputFragment>,
        syndic_turn: Option<SyndicTurnIdentity>,
    ) -> usize {
        self.push_turn_with_fragments(
            None,
            user_input_fragments,
            syndic_turn,
            TurnExecutionStatus::Queued,
            false,
        )
    }

    pub(super) fn append_user_input_fragment(
        &mut self,
        turn_index: usize,
        fragment: UserInputFragment,
    ) -> Option<usize> {
        let turn = Arc::make_mut(self.turns.get_mut(turn_index)?);
        turn.append_user_input_fragment_to_narrative(fragment);
        Some(turn.user_input_fragments.len().saturating_sub(1))
    }

    pub(super) fn remove_user_input_fragments(
        &mut self,
        removals: &[(usize, u64, &str)],
    ) -> Vec<usize> {
        let mut affected = Vec::new();
        for (turn_index, fragment_id, text) in removals {
            let Some(turn) = self.turns.get_mut(*turn_index) else {
                continue;
            };
            let turn = Arc::make_mut(turn);
            let before = turn.user_input_fragments.len();
            turn.user_input_fragments
                .retain(|fragment| fragment.id != *fragment_id || fragment.text != *text);
            turn.narrative_entries.retain(|entry| {
                !matches!(entry, TurnNarrativeEntry::UserInput { fragment_id: id } if id == fragment_id)
            });
            if turn.user_input_fragments.len() != before {
                affected.push(*turn_index);
            }
        }
        affected
    }

    pub(super) fn activate_pending_turn(&mut self, turn_index: usize) -> bool {
        let Some(turn) = self.turns.get_mut(turn_index) else {
            return false;
        };
        let turn = Arc::make_mut(turn);
        if turn.status != TurnExecutionStatus::Queued {
            return false;
        }
        turn.status = TurnExecutionStatus::Starting;
        self.active_turn_index = Some(turn_index);
        true
    }

    pub(super) fn turns(&self) -> &[Arc<TurnExecutionRecord>] {
        &self.turns
    }

    pub(super) fn syndic_turn_identity(&self, turn_index: usize) -> Option<SyndicTurnIdentity> {
        self.turns
            .get(turn_index)
            .and_then(|turn| turn.syndic_turn.clone())
    }

    fn push_turn_with_fragments(
        &mut self,
        thread_id: Option<String>,
        user_input_fragments: Vec<UserInputFragment>,
        syndic_turn: Option<SyndicTurnIdentity>,
        status: TurnExecutionStatus,
        active: bool,
    ) -> usize {
        let mut turn = TurnExecutionRecord {
            user_input_fragments: Vec::new(),
            narrative_entries: Vec::new(),
            thread_id,
            turn_id: None,
            syndic_turn,
            status,
            terminal_assistant_item_id: None,
            error_message: None,
            items: Vec::new(),
        };
        for fragment in user_input_fragments {
            turn.append_user_input_fragment_to_narrative(fragment);
        }
        self.turns.push(Arc::new(turn));
        let index = self.turns.len().saturating_sub(1);
        if active {
            self.active_turn_index = Some(index);
        }
        index
    }

    pub(super) fn retained_counts(&self) -> ActiveTurnRetainedCounts {
        let mut counts = ActiveTurnRetainedCounts {
            turns: self.turns.len(),
            ..ActiveTurnRetainedCounts::default()
        };
        for (index, turn) in self.turns.iter().enumerate() {
            counts.items = counts.items.saturating_add(turn.items.len());
            counts.user_fragments = counts
                .user_fragments
                .saturating_add(turn.user_input_fragments.len());
            counts.narrative_entries = counts
                .narrative_entries
                .saturating_add(turn.narrative_entries.len());
            counts.identity_bytes = counts
                .identity_bytes
                .saturating_add(turn.thread_id.as_ref().map_or(0, String::len))
                .saturating_add(turn.turn_id.as_ref().map_or(0, String::len));
            counts.error_bytes = counts
                .error_bytes
                .saturating_add(turn.error_message.as_ref().map_or(0, String::len));
            let mut turn_text_bytes = 0usize;
            for fragment in &turn.user_input_fragments {
                turn_text_bytes = turn_text_bytes.saturating_add(fragment.text.len());
                counts.user_fragment_text_bytes = counts
                    .user_fragment_text_bytes
                    .saturating_add(fragment.text.len());
                counts.backend_input_records = counts
                    .backend_input_records
                    .saturating_add(fragment.backend_input().len());
                counts.backend_input_bytes = counts.backend_input_bytes.saturating_add(
                    fragment
                        .backend_input()
                        .iter()
                        .map(user_input_payload_bytes)
                        .sum::<usize>(),
                );
                counts.image_marker_bytes = counts
                    .image_marker_bytes
                    .saturating_add(fragment.image_marker_specs().len().saturating_mul(32));
            }
            for item in &turn.items {
                match item {
                    ExecutionItem::AgentMessage(message) => {
                        turn_text_bytes = turn_text_bytes.saturating_add(message.text.len());
                        counts.agent_text_bytes =
                            counts.agent_text_bytes.saturating_add(message.text.len());
                    }
                }
            }
            let turn_payload = turn.retained_payload_bytes();
            if self.active_turn_index == Some(index) {
                counts.active_turn_payload_bytes = counts
                    .active_turn_payload_bytes
                    .saturating_add(turn_payload);
            }
            counts.payload_bytes = counts.payload_bytes.saturating_add(turn_payload);
            counts.text_bytes = counts.text_bytes.saturating_add(turn_text_bytes);
        }
        counts
    }

    pub(super) fn active_turn_source_pin_snapshot(&self) -> ActiveTurnSourcePinSnapshot {
        let Some(turn_index) = self.active_turn_index else {
            return ActiveTurnSourcePinSnapshot {
                max_retained_bytes: ACTIVE_TURN_RETAINED_SOURCE_MAX_BYTES,
                ..ActiveTurnSourcePinSnapshot::default()
            };
        };
        let Some(turn) = self.turns.get(turn_index) else {
            return ActiveTurnSourcePinSnapshot {
                max_retained_bytes: ACTIVE_TURN_RETAINED_SOURCE_MAX_BYTES,
                ..ActiveTurnSourcePinSnapshot::default()
            };
        };
        ActiveTurnSourcePinSnapshot {
            active: true,
            retained_bytes: turn.retained_payload_bytes(),
            max_retained_bytes: ACTIVE_TURN_RETAINED_SOURCE_MAX_BYTES,
            fallback_active: false,
        }
    }

    pub(super) fn working_turn_index(&self) -> Option<usize> {
        let index = self.active_turn_index?;
        self.turns
            .get(index)
            .is_some_and(|turn| {
                matches!(
                    turn.status,
                    TurnExecutionStatus::Starting | TurnExecutionStatus::Running
                )
            })
            .then_some(index)
    }

    pub(super) fn non_owned_active_turn_index(&self) -> Option<usize> {
        if self.active_turn_index.is_some() {
            return None;
        }
        self.turns
            .last()
            .is_some_and(|turn| {
                matches!(
                    turn.status,
                    TurnExecutionStatus::Starting | TurnExecutionStatus::Running
                )
            })
            .then(|| self.turns.len().saturating_sub(1))
    }

    pub(super) fn has_backend_active_turn(&self) -> bool {
        self.working_turn_index().is_some() || self.non_owned_active_turn_index().is_some()
    }

    pub(super) fn active_turn_identity(&self) -> Option<ActiveTurnIdentity> {
        let turn_index = self.active_turn_index?;
        let turn = self.turns.get(turn_index)?;
        matches!(
            turn.status,
            TurnExecutionStatus::Starting | TurnExecutionStatus::Running
        )
        .then(|| ActiveTurnIdentity {
            turn_index,
            thread_id: turn.thread_id.clone(),
            turn_id: turn.turn_id.clone(),
            syndic_turn: turn.syndic_turn.clone(),
        })
    }

    pub(super) fn last_turn_state(&self) -> LastTurnState {
        if self.working_turn_index().is_some() {
            return LastTurnState::Working;
        }

        match self.turns.last().map(|turn| turn.status) {
            None | Some(TurnExecutionStatus::Queued) => LastTurnState::Unknown,
            Some(TurnExecutionStatus::Starting | TurnExecutionStatus::Running) => {
                LastTurnState::Active
            }
            Some(TurnExecutionStatus::Completed) => LastTurnState::Ok,
            Some(TurnExecutionStatus::Interrupted | TurnExecutionStatus::Failed) => {
                LastTurnState::Error
            }
        }
    }

    pub(super) fn finish_turn_failure(&mut self, message: impl Into<String>) -> Option<usize> {
        let index = self.active_turn_index?;
        let turn = Arc::make_mut(self.turns.get_mut(index)?);
        turn.status = TurnExecutionStatus::Failed;
        turn.error_message = Some(message.into());
        self.active_turn_index = None;
        Some(index)
    }

    pub(super) fn apply_stream_event(&mut self, event: TurnStreamEvent) -> Option<usize> {
        let index = self.active_turn_index?;
        if !self
            .turns
            .get(index)
            .is_some_and(|turn| stream_event_matches_turn(turn, &event))
        {
            return None;
        }

        let turn = Arc::make_mut(self.turns.get_mut(index)?);
        let mut finished = false;
        match event {
            TurnStreamEvent::TurnStarted {
                thread_id,
                turn: info,
            } => {
                turn.thread_id = Some(thread_id);
                turn.turn_id = Some(info.id.clone());
                turn.status = TurnExecutionStatus::Running;
                turn.error_message = None;
                turn.ingest_turn_items(info.items, false);
            }
            TurnStreamEvent::TurnCompleted {
                thread_id,
                turn: info,
            } => {
                turn.thread_id = Some(thread_id);
                turn.turn_id = Some(info.id.clone());
                turn.status = turn_status_from_backend(info.status);
                turn.error_message = info.error.as_ref().map(turn_error_message);
                turn.ingest_turn_items(info.items, true);
                turn.terminal_assistant_item_id = resolve_terminal_assistant_item(turn);
                self.active_turn_index = None;
                finished = true;
            }
            TurnStreamEvent::ItemStarted { item, .. } => {
                turn.upsert_item(item, false);
            }
            TurnStreamEvent::ItemCompleted { item, .. } => {
                turn.upsert_item(item, true);
                turn.terminal_assistant_item_id = resolve_terminal_assistant_item(turn);
            }
            TurnStreamEvent::AgentMessageDelta { item_id, delta, .. } => {
                turn.ensure_agent_message(item_id).text.push_str(&delta);
            }
            TurnStreamEvent::ThreadStatusChanged { status, .. }
                if status.waiting_on_user_input() =>
            {
                turn.status = TurnExecutionStatus::Running;
            }
            _ => {}
        }
        Some(index).filter(|_| {
            finished
                || matches!(
                    turn.status,
                    TurnExecutionStatus::Starting | TurnExecutionStatus::Running
                )
        })
    }
}

impl TurnExecutionRecord {
    pub(super) fn has_resident_payload(&self) -> bool {
        !self.user_input_fragments.is_empty()
            || !self.narrative_entries.is_empty()
            || !self.items.is_empty()
            || self.error_message.is_some()
    }

    pub(super) fn user_input_fragments(&self) -> &[UserInputFragment] {
        &self.user_input_fragments
    }

    pub(super) fn narrative_entries(&self) -> &[TurnNarrativeEntry] {
        &self.narrative_entries
    }

    pub(super) fn user_input_fragment_by_id(
        &self,
        fragment_id: u64,
    ) -> Option<(usize, &UserInputFragment)> {
        self.user_input_fragments
            .iter()
            .enumerate()
            .find(|(_, fragment)| fragment.id == fragment_id)
    }

    pub(super) fn item_by_id(&self, item_id: &str) -> Option<&ExecutionItem> {
        self.items.iter().find(|item| item.id() == item_id)
    }

    pub(super) fn first_user_input_fragment_text(&self) -> Option<&str> {
        self.user_input_fragments
            .iter()
            .map(|fragment| fragment.text.trim())
            .find(|text| !text.is_empty())
    }

    pub(super) fn terminal_assistant_message(&self) -> Option<&AgentMessageDetail> {
        let terminal_id = self.terminal_assistant_item_id.as_deref();
        self.items.iter().rev().find_map(|item| match item {
            ExecutionItem::AgentMessage(message)
                if terminal_id.is_none() || terminal_id == Some(message.id.as_str()) =>
            {
                Some(message)
            }
            _ => None,
        })
    }

    fn append_user_input_fragment_to_narrative(&mut self, fragment: UserInputFragment) {
        let fragment_id = fragment.id;
        self.user_input_fragments.push(fragment);
        self.narrative_entries
            .push(TurnNarrativeEntry::UserInput { fragment_id });
    }

    fn ingest_turn_items(&mut self, items: Vec<ThreadItem>, complete: bool) {
        for item in items {
            self.upsert_item(item, complete);
        }
    }

    fn upsert_item(&mut self, item: ThreadItem, complete: bool) {
        let Some(execution_item) = execution_item_from_thread_item(item, complete) else {
            return;
        };
        let item_id = execution_item.id().to_string();
        if let Some(existing) = self.items.iter_mut().find(|item| item.id() == item_id) {
            *existing = execution_item;
        } else {
            self.items.push(execution_item);
            self.narrative_entries
                .push(TurnNarrativeEntry::Item { item_id });
        }
    }

    fn ensure_agent_message(&mut self, item_id: String) -> &mut AgentMessageDetail {
        let existing_index = self.items.iter().position(
            |item| matches!(item, ExecutionItem::AgentMessage(message) if message.id == item_id),
        );
        let index = match existing_index {
            Some(index) => index,
            None => {
                self.items
                    .push(ExecutionItem::AgentMessage(AgentMessageDetail {
                        id: item_id.clone(),
                        phase: None,
                        text: String::new(),
                        complete: false,
                    }));
                self.narrative_entries
                    .push(TurnNarrativeEntry::Item { item_id });
                self.items.len().saturating_sub(1)
            }
        };
        match &mut self.items[index] {
            ExecutionItem::AgentMessage(message) => message,
        }
    }

    fn retained_payload_bytes(&self) -> usize {
        let user_bytes = self
            .user_input_fragments
            .iter()
            .map(UserInputFragment::retained_payload_bytes_lower_bound)
            .sum::<usize>();
        let item_bytes = self
            .items
            .iter()
            .map(|item| match item {
                ExecutionItem::AgentMessage(message) => message.text.len(),
            })
            .sum::<usize>();
        user_bytes
            .saturating_add(item_bytes)
            .saturating_add(self.error_message.as_ref().map_or(0, String::len))
            .saturating_add(self.thread_id.as_ref().map_or(0, String::len))
            .saturating_add(self.turn_id.as_ref().map_or(0, String::len))
    }
}

impl ExecutionItem {
    fn id(&self) -> &str {
        match self {
            Self::AgentMessage(message) => &message.id,
        }
    }
}

impl LastTurnState {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Working => "Working",
            Self::Active => "Active",
            Self::Ok => "Ok",
            Self::Error => "Error",
        }
    }
}

fn execution_item_from_thread_item(item: ThreadItem, complete: bool) -> Option<ExecutionItem> {
    match item {
        ThreadItem::AgentMessage(item) => Some(ExecutionItem::AgentMessage(agent_message_detail(
            item, complete,
        ))),
        _ => None,
    }
}

fn agent_message_detail(item: AgentMessageItem, complete: bool) -> AgentMessageDetail {
    AgentMessageDetail {
        id: item.id,
        phase: item.phase,
        text: item.text,
        complete,
    }
}

fn resolve_terminal_assistant_item(turn: &TurnExecutionRecord) -> Option<String> {
    turn.items.iter().rev().find_map(|item| match item {
        ExecutionItem::AgentMessage(message)
            if message.phase == Some(ProtocolPhase::FinalAnswer) || !message.text.is_empty() =>
        {
            Some(message.id.clone())
        }
        _ => None,
    })
}

fn turn_status_from_backend(status: TurnStatus) -> TurnExecutionStatus {
    match status {
        TurnStatus::Completed => TurnExecutionStatus::Completed,
        TurnStatus::Interrupted => TurnExecutionStatus::Interrupted,
        TurnStatus::Failed => TurnExecutionStatus::Failed,
        TurnStatus::InProgress => TurnExecutionStatus::Running,
    }
}

fn turn_error_message(error: &TurnError) -> String {
    match error.additional_details.as_deref() {
        Some(details) if !details.trim().is_empty() => {
            format!("{} ({details})", error.message)
        }
        _ => error.message.clone(),
    }
}

fn stream_event_matches_turn(turn: &TurnExecutionRecord, event: &TurnStreamEvent) -> bool {
    let (thread_id, turn_id) = match event {
        TurnStreamEvent::TurnStarted { thread_id, turn }
        | TurnStreamEvent::TurnCompleted { thread_id, turn } => {
            (Some(thread_id.as_str()), Some(turn.id.as_str()))
        }
        TurnStreamEvent::ItemStarted {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ItemCompleted {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::AgentMessageDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ReasoningSummaryPartAdded {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ReasoningSummaryTextDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ReasoningTextDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::CommandExecutionOutputDelta {
            thread_id, turn_id, ..
        } => (Some(thread_id.as_str()), Some(turn_id.as_str())),
        TurnStreamEvent::ThreadStatusChanged { thread_id, .. } => (Some(thread_id.as_str()), None),
        _ => (None, None),
    };

    if let Some(active_thread_id) = turn.thread_id.as_deref()
        && let Some(event_thread_id) = thread_id
        && active_thread_id != event_thread_id
    {
        return false;
    }
    if let Some(active_turn_id) = turn.turn_id.as_deref()
        && let Some(event_turn_id) = turn_id
        && active_turn_id != event_turn_id
    {
        return false;
    }
    true
}
