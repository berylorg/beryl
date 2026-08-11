use std::time::Duration;

use beryl_home_store::{CommandError, CommitReceipt, ReconciliationDescriptor};
use beryl_model::SyndicThreadId;
use syndic_storage::CompactionAdmissionIneligibility;
use thiserror::Error;

const MAX_COMPLETION_TIMEOUT_SECONDS: u64 = 86_400;

/// Caller-selected wait policy for one manual compaction admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextCompactionRequest {
    thread_id: SyndicThreadId,
    completion_timeout: Duration,
}

impl ContextCompactionRequest {
    #[must_use]
    pub const fn new(thread_id: SyndicThreadId, completion_timeout: Duration) -> Self {
        Self {
            thread_id,
            completion_timeout,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn completion_timeout(self) -> Duration {
        self.completion_timeout
    }

    /// Validates the exact process wait policy without admitting provider work.
    pub fn validate(self) -> Result<Self, ContextCompactionError> {
        validate_completion_timeout(self.completion_timeout)?;
        Ok(self)
    }
}

/// Closed result visible to a non-GPUI compaction caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextCompactionOutcome {
    /// Exact provider success settled the durable operation.
    Succeeded,
    /// The shared completion deadline expired while exact provider authority remained live.
    StillRunning,
    /// The durable operation settled without provider success.
    Failed,
}

/// Failure to admit, dispatch, correlate, or settle exact context compaction.
#[derive(Debug, Error)]
pub enum ContextCompactionError {
    #[error("the context-compaction coordinator is unavailable")]
    Unavailable,
    #[error(
        "the context-compaction completion timeout must be a whole number of seconds in 1..=86400"
    )]
    InvalidTimeout,
    #[error("the context-compaction operation disagreed with durable or connection authority")]
    AuthorityMismatch,
    #[error("the selected thread is not eligible for context compaction: {0:?}")]
    Ineligible(CompactionAdmissionIneligibility),
    #[error("the context-compaction durable transition failed")]
    Storage,
    #[error("context-compaction transition was proven not committed: {0}")]
    CommandNotCommitted(#[source] CommandError),
    #[error("context-compaction transition committed before a later failure: {later_failure}")]
    CommandCommitted {
        receipt: CommitReceipt,
        #[source]
        later_failure: CommandError,
    },
    #[error("context-compaction transition has an indeterminate durable outcome: {failure}")]
    CommandIndeterminate {
        #[source]
        failure: CommandError,
        reconciliation: ReconciliationDescriptor,
    },
    #[error("the context-compaction driver is unavailable")]
    Driver,
}

/// Content-free bounded-capacity diagnostics for process-owned context compaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextCompactionDiagnostics {
    pub(super) queue_capacity: usize,
    pub(super) worker_capacity: usize,
    pub(super) queued_current: usize,
    pub(super) queued_high_water: usize,
    pub(super) workers_current: usize,
    pub(super) workers_high_water: usize,
    pub(super) denied_admissions: u64,
    pub(super) retained_operations: usize,
    pub(super) lifecycle_continuation_failures: u64,
}

impl ContextCompactionDiagnostics {
    #[must_use]
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity
    }

    #[must_use]
    pub const fn worker_capacity(self) -> usize {
        self.worker_capacity
    }

    #[must_use]
    pub const fn queued_current(self) -> usize {
        self.queued_current
    }

    #[must_use]
    pub const fn queued_high_water(self) -> usize {
        self.queued_high_water
    }

    #[must_use]
    pub const fn workers_current(self) -> usize {
        self.workers_current
    }

    #[must_use]
    pub const fn workers_high_water(self) -> usize {
        self.workers_high_water
    }

    #[must_use]
    pub const fn denied_admissions(self) -> u64 {
        self.denied_admissions
    }

    #[must_use]
    pub const fn retained_operations(self) -> usize {
        self.retained_operations
    }

    #[must_use]
    pub const fn lifecycle_continuation_failures(self) -> u64 {
        self.lifecycle_continuation_failures
    }
}

pub(super) fn validate_completion_timeout(timeout: Duration) -> Result<(), ContextCompactionError> {
    if timeout.subsec_nanos() != 0
        || !(1..=MAX_COMPLETION_TIMEOUT_SECONDS).contains(&timeout.as_secs())
    {
        return Err(ContextCompactionError::InvalidTimeout);
    }
    Ok(())
}
