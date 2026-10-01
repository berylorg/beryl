use crate::theme_runtime::AppearancePublicationTarget;

pub(super) async fn assert_refused(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    cx: &mut AsyncApp,
) {
    assert!(
        RunningProcessOwner::prepare_retired_interrupted_exit_selected_windows(
            owner,
            request,
            retired,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
            |_| panic!("refused retired recovery"),
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
    let foreign = request.test_foreign();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (supplied, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::prepare_retired_interrupted_exit_selected_windows(
                owner,
                supplied,
                retired,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                cancellation,
                |_| panic!("refused retired recovery"),
                cx,
            )
            .await
            .is_err()
        );
        assert!(owner.borrow().test_selected_recovery_appearance().is_none());
        assert!(
            owner
                .borrow()
                .interrupted_exit_graph_retirement_result(request)
                .is_ok()
        );
        assert!(
            owner
                .borrow()
                .interrupted_exit_services_result(request)
                .is_err()
        );
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        cx.update(|app| assert!(previous.read(app).target().snapshot().active))
            .unwrap();
    }
}
