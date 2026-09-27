#[test]
fn initial_refresh_preserves_typed_change_provenance() {
    for error in [
        ExitWorkError::Observation(AppServiceCloseError::Work(ShutdownWorkError::Work(
            ProcessWorkError::StaleRevision,
        ))),
        ExitWorkError::Admission(IdleShutdownError::Service(AppServiceCloseError::Work(
            ShutdownWorkError::Runtime(RuntimeWorkError::Stale),
        ))),
        ExitWorkError::Admission(IdleShutdownError::Preparation(
            CloseConfirmationPreparationError::Runtime(RuntimeWorkError::Stale),
        )),
    ] {
        assert!(work_changed(&error), "{error:?}");
    }
    for error in [
        ExitWorkError::Request("stale".into()),
        ExitWorkError::IntentBusy,
        ExitWorkError::Confirmation("stale".into()),
        ExitWorkError::RefreshScheduling("stale".into()),
        ExitWorkError::Observation(AppServiceCloseError::Unavailable),
        ExitWorkError::Observation(AppServiceCloseError::Work(ShutdownWorkError::Work(
            ProcessWorkError::Cancelled,
        ))),
        ExitWorkError::Admission(IdleShutdownError::Window("stale".into())),
        ExitWorkError::Admission(IdleShutdownError::Preparation(
            CloseConfirmationPreparationError::Window("stale".into()),
        )),
        ExitWorkError::Admission(IdleShutdownError::Preparation(
            CloseConfirmationPreparationError::Unavailable,
        )),
        ExitWorkError::Admission(IdleShutdownError::Preparation(
            CloseConfirmationPreparationError::AlreadyShuttingDown,
        )),
    ] {
        assert!(!work_changed(&error), "{error:?}");
    }
    for runtime in [
        RuntimeWorkError::Busy,
        RuntimeWorkError::Unavailable,
        RuntimeWorkError::Closed,
        RuntimeWorkError::Foreign,
    ] {
        let error = ExitWorkError::Admission(IdleShutdownError::Preparation(
            CloseConfirmationPreparationError::Runtime(runtime),
        ));
        assert!(!work_changed(&error), "{error:?}");
    }
}
