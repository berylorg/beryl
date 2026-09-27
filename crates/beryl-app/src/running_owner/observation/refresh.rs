use super::ConfirmedShutdownError;
use crate::{
    app_services::AppServiceCloseError,
    cas_projection::{
        CompactionWorkError, ConnectionWorkError, ControlWorkError, ProcessWorkError,
        RuntimeWorkError, ScheduledSessionWorkError, ShutdownCoordinatorError, ShutdownWorkError,
        StopWorkError,
    },
};
use beryl_home_store::{HomeMutationObservationError, HomeObservedCoherenceError};
use syndic_storage::SyndicReadError;

pub(super) fn work_changed(error: &ConfirmedShutdownError) -> bool {
    match error {
        ConfirmedShutdownError::Service(error) => service_work_changed(error),
        _ => false,
    }
}

pub(in crate::running_owner) fn service_work_changed(error: &AppServiceCloseError) -> bool {
    let work = match error {
        AppServiceCloseError::Work(work)
        | AppServiceCloseError::Coordinator(ShutdownCoordinatorError::Work(work)) => work,
        _ => return false,
    };
    matches!(
        work,
        ShutdownWorkError::Runtime(RuntimeWorkError::Stale)
            | ShutdownWorkError::Home(HomeObservedCoherenceError::Observation(
                HomeMutationObservationError::Stale
            ))
            | ShutdownWorkError::Work(
                ProcessWorkError::StaleRevision
                    | ProcessWorkError::MutationObservation(HomeMutationObservationError::Stale)
                    | ProcessWorkError::Sessions(ScheduledSessionWorkError::StaleRevision)
                    | ProcessWorkError::Connections(ConnectionWorkError::StaleRevision)
                    | ProcessWorkError::Controls(ControlWorkError::Stop(
                        StopWorkError::StaleRevision
                    ))
                    | ProcessWorkError::Controls(ControlWorkError::Compaction(
                        CompactionWorkError::StaleRevision
                    ))
                    | ProcessWorkError::Durable(
                        SyndicReadError::StaleNonIdleGateSourceScan
                            | SyndicReadError::ConcurrentChange { .. }
                    )
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("../../../tests/unit/app_services/confirmed_work_refresh.rs");
}
