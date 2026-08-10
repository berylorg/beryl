use crate::cas_projection::{
    PersistentFailureNotificationStatus, persistent_failure::LiveCommandGateStatus,
};

use super::super::AcceptedInputSchedulerContext;
use super::types::SchedulerFailure;

pub(in crate::cas_projection::accepted_input_scheduler) fn reconcile_failure(
    context: &AcceptedInputSchedulerContext,
    failure: SchedulerFailure,
) -> Option<SchedulerFailure> {
    debug_assert_ne!(failure, SchedulerFailure::VerificationPending);
    if failure == SchedulerFailure::VerificationPending {
        return Some(SchedulerFailure::Fatal);
    }
    if failure == SchedulerFailure::Fatal {
        return Some(SchedulerFailure::Fatal);
    }

    let authorizer = context.command_gate.authorizer();
    match authorizer.status_exact() {
        Ok(LiveCommandGateStatus::PersistentFailure) => {
            Some(SchedulerFailure::PersistentHomeFailure)
        }
        Ok(LiveCommandGateStatus::OrdinaryShutdown) => None,
        Ok(LiveCommandGateStatus::LocalFailure) => {
            if !authorizer.is_persistent_failure_cut() {
                let _ = authorizer.observe_persistent_failure();
            }
            Some(SchedulerFailure::Fatal)
        }
        Ok(LiveCommandGateStatus::Open) => {
            match authorizer.observe_persistent_failure() {
                PersistentFailureNotificationStatus::Signaled
                | PersistentFailureNotificationStatus::Joined => {}
                PersistentFailureNotificationStatus::VerificationSignaled
                | PersistentFailureNotificationStatus::VerificationJoined
                | PersistentFailureNotificationStatus::NotFailed
                | PersistentFailureNotificationStatus::Unavailable => {
                    return Some(SchedulerFailure::Fatal);
                }
            }
            match authorizer.status_exact() {
                Ok(LiveCommandGateStatus::PersistentFailure) => {
                    Some(SchedulerFailure::PersistentHomeFailure)
                }
                Ok(
                    LiveCommandGateStatus::Open
                    | LiveCommandGateStatus::OrdinaryShutdown
                    | LiveCommandGateStatus::LocalFailure,
                )
                | Err(_) => Some(SchedulerFailure::Fatal),
            }
        }
        Err(_) => Some(SchedulerFailure::Fatal),
    }
}
