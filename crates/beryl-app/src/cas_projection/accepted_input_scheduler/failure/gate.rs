use crate::cas_projection::{
    LiveCommandAdmissionError, LiveCommandPermit, persistent_failure::LiveCommandGateStatus,
};

use super::super::AcceptedInputSchedulerContext;
use super::types::SchedulerFailure;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection::accepted_input_scheduler) enum SchedulerGateStatus {
    Open,
    OrdinaryShutdown,
    PersistentHomeFailure,
}

pub(in crate::cas_projection::accepted_input_scheduler) fn authorize(
    context: &AcceptedInputSchedulerContext,
) -> Result<Option<LiveCommandPermit>, SchedulerFailure> {
    let authorizer = context.command_gate.authorizer();
    match authorizer.authorize() {
        Ok(command) => Ok(Some(command)),
        Err(LiveCommandAdmissionError::Closed) => match gate_status(context)? {
            SchedulerGateStatus::OrdinaryShutdown => Ok(None),
            SchedulerGateStatus::PersistentHomeFailure => {
                Err(SchedulerFailure::PersistentHomeFailure)
            }
            SchedulerGateStatus::Open => Err(SchedulerFailure::Fatal),
        },
        Err(LiveCommandAdmissionError::Unavailable) => Err(SchedulerFailure::Fatal),
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn gate_status(
    context: &AcceptedInputSchedulerContext,
) -> Result<SchedulerGateStatus, SchedulerFailure> {
    match context.command_gate.authorizer().status_exact() {
        Ok(LiveCommandGateStatus::Open) => Ok(SchedulerGateStatus::Open),
        Ok(LiveCommandGateStatus::OrdinaryShutdown) => Ok(SchedulerGateStatus::OrdinaryShutdown),
        Ok(LiveCommandGateStatus::PersistentFailure) => {
            Ok(SchedulerGateStatus::PersistentHomeFailure)
        }
        Ok(LiveCommandGateStatus::LocalFailure) | Err(_) => Err(SchedulerFailure::Fatal),
    }
}
