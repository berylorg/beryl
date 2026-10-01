use crate::theme_runtime::AppearancePublicationTarget;

pub(super) async fn assert_refused(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    cx: &mut AsyncApp,
) {
    assert!(
        RunningProcessOwner::recover_prepared_interrupted_exit(
            owner,
            request,
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
    let prepared = owner.borrow().interrupted_exit_appearance(request).unwrap();
    let home = prepared.prepared().home();
    let services = owner.borrow();
    assert_eq!(
        services
            .test_services()
            .retired_service_generation_for_home_return(home.home_id())
            .unwrap(),
        retired
    );
    assert!(
        services
            .test_services()
            .validate_retired_service_home_return(home.home_generation(), Some(home.home_id()))
            .is_err()
    );
    assert!(
        services
            .test_services()
            .retired_service_generation()
            .is_err()
    );
    drop(services);
    let foreign = request.test_foreign();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (supplied, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::recover_prepared_interrupted_exit(
                owner,
                supplied,
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
