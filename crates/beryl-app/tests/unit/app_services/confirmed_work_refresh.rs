#[test]
fn refreshes_only_changed_work_evidence() {
    for work in [
        ShutdownWorkError::Runtime(RuntimeWorkError::Stale),
        ShutdownWorkError::Home(HomeObservedCoherenceError::Observation(
            HomeMutationObservationError::Stale,
        )),
        ShutdownWorkError::Work(ProcessWorkError::StaleRevision),
        ShutdownWorkError::Work(ProcessWorkError::MutationObservation(
            HomeMutationObservationError::Stale,
        )),
        ShutdownWorkError::Work(ProcessWorkError::Sessions(
            ScheduledSessionWorkError::StaleRevision,
        )),
        ShutdownWorkError::Work(ProcessWorkError::Connections(
            ConnectionWorkError::StaleRevision,
        )),
        ShutdownWorkError::Work(ProcessWorkError::Controls(ControlWorkError::Stop(
            StopWorkError::StaleRevision,
        ))),
        ShutdownWorkError::Work(ProcessWorkError::Controls(ControlWorkError::Compaction(
            CompactionWorkError::StaleRevision,
        ))),
        ShutdownWorkError::Work(ProcessWorkError::Durable(
            SyndicReadError::StaleNonIdleGateSourceScan,
        )),
        ShutdownWorkError::Work(ProcessWorkError::Durable(
            SyndicReadError::ConcurrentChange {
                operation: "shutdown read",
            },
        )),
    ] {
        let error = ConfirmedShutdownError::Service(AppServiceCloseError::Work(work));
        assert!(work_changed(&error), "{error:?}");
        let ConfirmedShutdownError::Service(AppServiceCloseError::Work(work)) = error else {
            unreachable!()
        };
        assert!(work_changed(&ConfirmedShutdownError::Service(
            AppServiceCloseError::Coordinator(ShutdownCoordinatorError::Work(work))
        )));
    }
    for error in [
        ConfirmedShutdownError::StaleObservation,
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Window(
            crate::window_acquisition::WindowCloseAdmissionError::WindowSetChanged,
        ))),
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Home(
            HomeObservedCoherenceError::Coherence(
                beryl_home_store::HomeCoherenceError::ReconciliationPending,
            ),
        ))),
        ConfirmedShutdownError::Service(AppServiceCloseError::Unavailable),
        ConfirmedShutdownError::Service(AppServiceCloseError::Coordinator(
            ShutdownCoordinatorError::StaleAttempt,
        )),
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Runtime(
            RuntimeWorkError::Foreign,
        ))),
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Runtime(
            RuntimeWorkError::Busy,
        ))),
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Runtime(
            RuntimeWorkError::Closed,
        ))),
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Runtime(
            RuntimeWorkError::Unavailable,
        ))),
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Home(
            HomeObservedCoherenceError::ForeignObservation,
        ))),
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Home(
            HomeObservedCoherenceError::Observation(HomeMutationObservationError::Revoked),
        ))),
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Work(
            ProcessWorkError::Cancelled,
        ))),
        ConfirmedShutdownError::Service(AppServiceCloseError::Work(ShutdownWorkError::Work(
            ProcessWorkError::ForeignSources,
        ))),
    ] {
        assert!(!work_changed(&error), "{error:?}");
    }
}
