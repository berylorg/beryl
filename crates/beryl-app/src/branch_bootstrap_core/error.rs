use super::*;

impl fmt::Display for BranchBootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMessage { thread_id } => write!(
                formatter,
                "Beryl could not bootstrap branch thread {} because the bootstrap message was empty.",
                thread_id.as_str()
            ),
            Self::TurnStartFailed { thread_id, error } => write!(
                formatter,
                "Beryl created branch thread {} but could not start its bootstrap turn: {error}",
                thread_id.as_str()
            ),
            Self::BootstrapTurnMissingId { thread_id } => write!(
                formatter,
                "Beryl started the bootstrap turn for branch thread {}, but the backend did not return a turn id to prove completion.",
                thread_id.as_str()
            ),
            Self::BootstrapStreamFailed {
                thread_id,
                turn_id,
                error,
            } => write!(
                formatter,
                "Beryl started bootstrap turn {} for branch thread {} but lost the turn stream before completion: {error}",
                turn_id.as_str(),
                thread_id.as_str()
            ),
            Self::BootstrapTurnFailed {
                thread_id,
                turn_id,
                status,
                error,
            } => write!(
                formatter,
                "Bootstrap turn {} for branch thread {} finished with status {}{}.",
                turn_id.as_str(),
                thread_id.as_str(),
                turn_status_label(*status),
                error
                    .as_deref()
                    .map(|error| format!(": {error}"))
                    .unwrap_or_default()
            ),
            Self::BootstrapUnexpectedApprovalRequest {
                thread_id,
                turn_id,
                request,
            } => write!(
                formatter,
                "Bootstrap turn {} for branch thread {} requested backend approval unexpectedly; Beryl denied it and did not publish the branch. Request: {request}",
                turn_id.as_str(),
                thread_id.as_str()
            ),
            Self::BootstrapApprovalDenialFailed {
                thread_id,
                turn_id,
                error,
            } => write!(
                formatter,
                "Bootstrap turn {} for branch thread {} requested backend approval unexpectedly, and Beryl could not deny it: {error}",
                turn_id.as_str(),
                thread_id.as_str()
            ),
            Self::BootstrapUnexpectedDynamicToolRequest {
                thread_id,
                turn_id,
                request,
            } => write!(
                formatter,
                "Bootstrap turn {} for branch thread {} requested a dynamic tool unexpectedly; Beryl returned an unavailable response and did not publish the branch. Request: {request}",
                turn_id.as_str(),
                thread_id.as_str()
            ),
            Self::BootstrapDynamicToolResponseFailed {
                thread_id,
                turn_id,
                error,
            } => write!(
                formatter,
                "Bootstrap turn {} for branch thread {} requested a dynamic tool unexpectedly, and Beryl could not return the unavailable response: {error}",
                turn_id.as_str(),
                thread_id.as_str()
            ),
            Self::DurabilityProofFailed { thread_id, error } => write!(
                formatter,
                "Beryl completed the bootstrap turn for branch thread {} but could not prove the thread is durable: {error}",
                thread_id.as_str()
            ),
            Self::DurableThreadIdMismatch {
                expected_thread_id,
                actual_thread_id,
            } => write!(
                formatter,
                "Beryl started a bootstrap turn for branch thread {}, but the backend durability read returned thread {}.",
                expected_thread_id.as_str(),
                actual_thread_id
            ),
            Self::DurableThreadMarkedEphemeral { thread_id } => write!(
                formatter,
                "Beryl started a bootstrap turn for branch thread {}, but the backend still marked the thread ephemeral.",
                thread_id.as_str()
            ),
            Self::BootstrapTurnMissingFromHistory { thread_id, turn_id } => write!(
                formatter,
                "Beryl completed bootstrap turn {} for branch thread {}, but the final history read did not contain that turn.",
                turn_id.as_str(),
                thread_id.as_str()
            ),
            Self::BootstrapTurnNotCompletedInHistory {
                thread_id,
                turn_id,
                status,
            } => write!(
                formatter,
                "Beryl completed bootstrap turn {} for branch thread {}, but the final history read still reported status {}.",
                turn_id.as_str(),
                thread_id.as_str(),
                turn_status_label(*status)
            ),
            Self::BootstrapTurnMissingVisibleMessage { thread_id, turn_id } => write!(
                formatter,
                "Beryl completed bootstrap turn {} for branch thread {}, but the final history read did not contain the visible bootstrap user message.",
                turn_id.as_str(),
                thread_id.as_str()
            ),
        }
    }
}

impl std::error::Error for BranchBootstrapError {}

fn turn_status_label(status: TurnStatus) -> &'static str {
    match status {
        TurnStatus::Completed => "completed",
        TurnStatus::Interrupted => "interrupted",
        TurnStatus::Failed => "failed",
        TurnStatus::InProgress => "in progress",
    }
}
