use super::*;

impl fmt::Display for ThreadedDecisionStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateRecordId { record_id } => {
                write!(
                    f,
                    "threaded-decision record {} already exists",
                    record_id.as_str()
                )
            }
            Self::ActiveBranchExists {
                checklist_item_id,
                existing_record_id,
            } => write!(
                f,
                "checklist item {} already has active threaded-decision record {}",
                checklist_item_id.as_str(),
                existing_record_id.as_str()
            ),
            Self::MissingRecord { record_id } => {
                write!(
                    f,
                    "threaded-decision record {} does not exist",
                    record_id.as_str()
                )
            }
            Self::InvalidTransition {
                record_id,
                from,
                to,
            } => write!(
                f,
                "threaded-decision record {} cannot transition from {} to {}",
                record_id.as_str(),
                from.label(),
                to.label()
            ),
        }
    }
}

impl Error for ThreadedDecisionStateError {}

impl ThreadedDecisionStatus {
    fn label(self) -> &'static str {
        match self {
            Self::QueuedBranch => "queued_branch",
            Self::ActiveBranch => "active_branch",
            Self::PendingResolution => "pending_resolution",
            Self::HandoffStarted => "handoff_started",
            Self::ChecklistUpdated => "checklist_updated",
            Self::ArchivePending => "archive_pending",
            Self::ArchiveFailed => "archive_failed",
            Self::Closed => "closed",
            Self::Superseded => "superseded",
            Self::Invalidated => "invalidated",
        }
    }
}
