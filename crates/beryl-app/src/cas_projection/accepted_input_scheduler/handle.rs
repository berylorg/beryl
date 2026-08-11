use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::Arc,
    thread::JoinHandle,
};

#[cfg(feature = "test-faults")]
use beryl_home_store::HomeGeneration;
#[cfg(feature = "test-faults")]
use beryl_model::BerylHomeId;

use super::{
    AcceptedInputSchedulerContext, AcceptedInputSchedulerDiagnostics, AcceptedInputSchedulerExit,
    AcceptedInputSchedulerSignal, AcceptedInputWakeReason, ActiveSteeringCancellationLifecycle,
    SchedulerRuntime,
};
use crate::cas_projection::{ProjectionCancellationToken, ProjectionCoordinatorError};

pub(in crate::cas_projection) struct AcceptedInputScheduler {
    signal: AcceptedInputSchedulerSignal,
    cancellation: ActiveSteeringCancellationLifecycle,
    ordinary_cancellation: ProjectionCancellationToken,
    handle: Option<JoinHandle<AcceptedInputSchedulerExit>>,
    #[cfg(feature = "test-faults")]
    test_identity: (
        BerylHomeId,
        HomeGeneration,
        crate::cas_projection::ProjectionServiceGeneration,
    ),
}

impl AcceptedInputScheduler {
    pub(in crate::cas_projection) fn start(
        context: AcceptedInputSchedulerContext,
    ) -> Result<Self, ProjectionCoordinatorError> {
        Self::start_with_initial_start(
            context,
            crate::cas_projection::initial_start::InitialStartGate::ready(),
        )
    }

    pub(in crate::cas_projection) fn start_with_initial_start(
        context: AcceptedInputSchedulerContext,
        initial_start: Arc<crate::cas_projection::initial_start::InitialStartGate>,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let signal = context.signal.clone();
        let cancellation = context.cancellation.clone();
        let ordinary_cancellation = context.ordinary_cancellation.clone();
        #[cfg(feature = "test-faults")]
        let test_identity = (
            context.home_id,
            context.home_generation,
            context.command_gate.service_generation(),
        );
        let handle = std::thread::Builder::new()
            .name("beryl-accepted-input-scheduler".to_owned())
            .spawn(move || {
                if !initial_start.wait() {
                    context.signal.update_diagnostics(|diagnostics| {
                        diagnostics.stopped = true;
                        diagnostics.fatal = false;
                    });
                    return AcceptedInputSchedulerExit::Clean;
                }
                let mut runtime = SchedulerRuntime::new(context);
                match catch_unwind(AssertUnwindSafe(|| runtime.run())) {
                    Ok(exit) => exit,
                    Err(payload) => {
                        runtime.emergency_quiesce();
                        resume_unwind(payload)
                    }
                }
            })
            .map_err(
                |source| ProjectionCoordinatorError::AcceptedInputSchedulerSpawn {
                    message: source.to_string(),
                },
            )?;
        Ok(Self {
            signal,
            cancellation,
            ordinary_cancellation,
            handle: Some(handle),
            #[cfg(feature = "test-faults")]
            test_identity,
        })
    }

    pub(in crate::cas_projection) fn diagnostics(&self) -> AcceptedInputSchedulerDiagnostics {
        self.signal.diagnostics()
    }

    pub(in crate::cas_projection) fn request_shutdown(&self) {
        self.cancellation.cancel_current();
        self.ordinary_cancellation.cancel();
        self.signal.request_shutdown();
    }

    #[allow(
        dead_code,
        reason = "the renewable boundary is mounted before the later stop controller"
    )]
    pub(in crate::cas_projection) fn cancel_current_lifecycle(&self) {
        self.cancellation.cancel_current();
        self.signal
            .wake(AcceptedInputWakeReason::CancellationRequested);
    }

    #[allow(
        dead_code,
        reason = "the renewable boundary is mounted before the later stop controller"
    )]
    pub(in crate::cas_projection) fn renew_cancellation_lifecycle(&self) {
        self.cancellation.renew();
        self.signal
            .wake(AcceptedInputWakeReason::CancellationLifecycle);
    }

    pub(in crate::cas_projection) fn join(mut self) -> Result<AcceptedInputSchedulerExit, ()> {
        let handle = self.handle.take().ok_or(())?;
        #[cfg(feature = "test-faults")]
        crate::cas_projection::test_faults::observe_accepted_input_scheduler_join(
            self.test_identity.0,
            self.test_identity.1,
            self.test_identity.2,
        );
        handle.join().map_err(|_| ())
    }
}
