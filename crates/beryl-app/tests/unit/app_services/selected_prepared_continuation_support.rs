use crate::theme_runtime::AppearancePublicationTarget;

pub(super) async fn assert_refused(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    cx: &mut AsyncApp,
) {
    assert!(
        RunningProcessOwner::complete_prepared_interrupted_exit_selected_windows(
            owner,
            request,
            retired,
            configuration(),
            CommandCancellation::new(),
            cx,
        )
        .await
        .is_err()
    );
}

pub(super) async fn verify_refusals(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    cx: &mut AsyncApp,
) {
    let previous = owner.borrow().test_process_appearance();
    assert!(owner.borrow().test_selected_recovery_appearance().is_none());
    let generation = owner
        .borrow()
        .interrupted_exit_appearance(request)
        .unwrap()
        .prepared()
        .home()
        .home_generation();
    let foreign = request.test_foreign();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (supplied, supplied_retired, cancellation) in [
        (&foreign, retired, CommandCancellation::new()),
        (request, generation, CommandCancellation::new()),
        (request, retired, cancelled),
    ] {
        assert!(
            RunningProcessOwner::complete_prepared_interrupted_exit_selected_windows(
                owner,
                supplied,
                supplied_retired,
                configuration(),
                cancellation,
                cx,
            )
            .await
            .is_err()
        );
        assert!(owner.borrow().test_selected_recovery_appearance().is_none());
        assert!(
            owner
                .borrow()
                .interrupted_exit_services_result(request)
                .is_ok()
        );
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        cx.update(|app| assert!(previous.read(app).target().snapshot().active))
            .unwrap();
    }
}
