use beryl_backend::TurnStatus;

use crate::LifecycleYieldOutcome;

use super::{execution_detail::UserInputFragment, lifecycle_yield::TerminalLifecycleYield};

pub(super) const PHASE_CONTINUE_RESUME_TEXT: &str = "Continue from the root doc/plan.md.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PhaseContinueRequest {
    thread_id: String,
    resume_fragment: UserInputFragment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PhaseContinueNewThreadHandoff {
    source_thread_id: String,
    source_turn_id: String,
    resume_fragment: UserInputFragment,
}

impl PhaseContinueNewThreadHandoff {
    pub(super) fn source_thread_id(&self) -> &str {
        self.source_thread_id.as_str()
    }

    #[allow(dead_code)]
    pub(super) fn source_turn_id(&self) -> &str {
        self.source_turn_id.as_str()
    }

    #[allow(dead_code)]
    pub(super) fn resume_fragment(&self) -> UserInputFragment {
        self.resume_fragment.clone()
    }
}

impl PhaseContinueRequest {
    pub(super) fn thread_id(&self) -> &str {
        self.thread_id.as_str()
    }

    pub(super) fn resume_fragment(&self) -> UserInputFragment {
        self.resume_fragment.clone()
    }
}

pub(super) fn phase_continue_request(
    lifecycle_yield: &TerminalLifecycleYield,
) -> Option<PhaseContinueRequest> {
    (lifecycle_yield.outcome() == LifecycleYieldOutcome::PhaseContinue).then(|| {
        PhaseContinueRequest {
            thread_id: lifecycle_yield.thread_id().to_string(),
            resume_fragment: UserInputFragment::text(PHASE_CONTINUE_RESUME_TEXT),
        }
    })
}

pub(super) fn phase_continue_new_thread_handoff(
    lifecycle_yield: &TerminalLifecycleYield,
    terminal_status: TurnStatus,
) -> Option<PhaseContinueNewThreadHandoff> {
    (lifecycle_yield.outcome() == LifecycleYieldOutcome::PhaseContinueNewThread
        && terminal_status == TurnStatus::Completed)
        .then(|| PhaseContinueNewThreadHandoff {
            source_thread_id: lifecycle_yield.thread_id().to_string(),
            source_turn_id: lifecycle_yield.turn_id().to_string(),
            resume_fragment: UserInputFragment::text(PHASE_CONTINUE_RESUME_TEXT),
        })
}

pub(super) fn take_phase_continue_new_thread_handoff_for_finished_worker(
    pending_handoff: &mut Option<PhaseContinueNewThreadHandoff>,
    finished_source_thread_id: Option<&str>,
) -> Option<PhaseContinueNewThreadHandoff> {
    let handoff = pending_handoff.take()?;
    (finished_source_thread_id == Some(handoff.source_thread_id())).then_some(handoff)
}

pub(super) fn pending_turn_queue_should_wait_for_compaction(
    context_compaction_thread_id: Option<&str>,
    thread_id: &str,
) -> bool {
    context_compaction_thread_id == Some(thread_id)
}

pub(super) fn context_compaction_queue_failure_message(message: &str) -> String {
    format!("Beryl could not send the queued input because context compaction failed: {message}")
}
