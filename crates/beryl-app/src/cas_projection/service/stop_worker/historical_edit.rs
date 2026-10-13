use super::*;
use syndic_storage::{DiscussionHandoffGateState, InputGateState, ThreadArchiveState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoricalEditUnavailable {
    AuthorityUnavailable,
    RepairRequired,
    TurnRunning,
    CompactionRunning,
    QueuedInput,
    Archived,
    HandoffUnavailable,
}

impl HistoricalEditUnavailable {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::AuthorityUnavailable => "Historical editing is unavailable.",
            Self::RepairRequired => "Repair the selected turn before editing an earlier input.",
            Self::TurnRunning => {
                "Wait for the current turn to finish before editing an earlier input."
            }
            Self::CompactionRunning => {
                "Wait for compaction to finish before editing an earlier input."
            }
            Self::QueuedInput => "Wait for queued input to finish before editing an earlier input.",
            Self::Archived => "Archived discussions cannot be edited.",
            Self::HandoffUnavailable => "This discussion is no longer open for editing.",
        }
    }
}

impl ExactStopWorker {
    pub fn selected_historical_edit_availability(
        &self,
        thread: SyndicThreadId,
    ) -> Result<(), HistoricalEditUnavailable> {
        let _command = self
            .command_authorizer
            .authorize()
            .map_err(|_| HistoricalEditUnavailable::AuthorityUnavailable)?;
        self.read()
            .ok_or(HistoricalEditUnavailable::AuthorityUnavailable)?
            .selected_historical_edit_availability(thread)
    }
}

impl ExactStopRead {
    fn selected_historical_edit_availability(
        &self,
        thread: SyndicThreadId,
    ) -> Result<(), HistoricalEditUnavailable> {
        use HistoricalEditUnavailable as Denial;
        let read = || -> Option<Result<(), Denial>> {
            let home = self.home.as_deref()?;
            let revision = home.home_revision().ok()?;
            let limit = SyndicPointReadLimit::new(65_540).ok()?;
            let record = self.storage.thread(home, thread, limit).ok()??;
            let gate = self.storage.input_gate(home, thread, limit).ok()??;
            let attributes = self.storage.thread_attributes(home, thread, limit).ok()??;
            let handoff = self
                .storage
                .discussion_handoff_gate(home, thread, limit)
                .ok()?;
            if record.id() != thread
                || gate.thread_id() != thread
                || attributes.thread_id() != thread
            {
                return None;
            }
            let editable = match (record.parent_thread_id(), attributes.archive(), handoff) {
                (None, ThreadArchiveState::Ordinary, None) => record.context_owner_id().is_none(),
                (Some(_), ThreadArchiveState::BranchDiscussionOpen, Some(gate)) => {
                    gate.thread_id() == thread
                        && record.context_owner_id().is_some()
                        && gate.state() == DiscussionHandoffGateState::Open
                }
                _ => false,
            };
            let availability = if attributes.archive().is_archived() {
                Err(Denial::Archived)
            } else if !editable {
                Err(Denial::HandoffUnavailable)
            } else {
                match gate.state() {
                    InputGateState::RepairRequired(_) => Err(Denial::RepairRequired),
                    InputGateState::Compacting { .. } => Err(Denial::CompactionRunning),
                    InputGateState::Idle if gate.live_count() == 0 => Ok(()),
                    InputGateState::Idle => Err(Denial::QueuedInput),
                    _ => Err(Denial::TurnRunning),
                }
            };
            if home.home_revision().ok()? != revision || !self.command_authorizer.is_open() {
                return None;
            }
            Some(availability)
        };
        read().unwrap_or(Err(Denial::AuthorityUnavailable))
    }
}
