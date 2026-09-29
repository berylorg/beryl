use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};

pub(super) fn assert_unavailable(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
    app: &mut gpui::App,
) {
    assert!(owner.borrow().interrupted_exit_appearance(request).is_err());
    let window = owner.borrow().test_process().windows.shells()[0].window();
    assert!(
        owner
            .borrow_mut()
            .bind_interrupted_exit_appearance(request, window, appearance, app)
            .is_err()
    );
}

pub(super) fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
    substituted: &gpui::Entity<GpuiAppearanceWindowSet>,
    previous: &gpui::Entity<GpuiAppearanceWindowSet>,
    app: &mut gpui::App,
) {
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let foreign = request.test_foreign();
    let mut running = owner.borrow_mut();
    assert!(running.interrupted_exit_appearance(&foreign).is_err());
    assert!(std::sync::Arc::ptr_eq(
        &running.interrupted_exit_appearance(request).unwrap(),
        &appearance.read(app).target().snapshot().current,
    ));
    let bind = |running: &mut RunningProcessOwner, request, appearance, app: &mut gpui::App| {
        running.bind_interrupted_exit_appearance(request, window, appearance, app)
    };
    assert!(
        bind(&mut running, &foreign, appearance, app)
            .unwrap_err()
            .contains("request changed")
    );
    running.test_replace_interrupted_exit_request(&foreign);
    assert!(
        bind(&mut running, request, appearance, app)
            .unwrap_err()
            .contains("request changed")
    );
    running.test_replace_interrupted_exit_request(request);
    for retirement in [None, Some(Err("failed retirement".into()))] {
        running.test_set_resident_graph_retirement(retirement);
        assert!(bind(&mut running, request, appearance, app).is_err());
    }
    running.test_set_resident_graph_retirement(Some(Ok(())));
    assert!(
        bind(&mut running, request, substituted, app)
            .unwrap_err()
            .contains("differs from the prepared graph")
    );
    assert!(
        bind(&mut running, request, previous, app)
            .unwrap_err()
            .contains("candidate identity changed")
    );
    let inactive = GpuiAppearanceWindowSet::new(
        appearance.read(app).target().snapshot().current,
        NonZeroUsize::new(4).unwrap(),
        app,
    );
    inactive.update(app, |set, _| set.retire());
    assert!(
        bind(&mut running, request, &inactive, app)
            .unwrap_err()
            .contains("candidate identity changed")
    );
    let drafts = running.test_replace_recovery_drafts(None).unwrap();
    assert!(
        bind(&mut running, request, appearance, app)
            .unwrap_err()
            .contains("drafts are unavailable")
    );
    running.test_replace_recovery_drafts(Some(drafts.clone()));
    let borrowed = drafts.borrow_mut();
    assert!(
        bind(&mut running, request, appearance, app)
            .unwrap_err()
            .contains("drafts are busy")
    );
    drop(borrowed);
    drafts.borrow_mut().test_recovery_driving(true);
    assert!(
        bind(&mut running, request, appearance, app)
            .unwrap_err()
            .contains("not available for binding")
    );
    drafts.borrow_mut().test_recovery_driving(false);
    assert!(bind(&mut running, request, appearance, app).is_err());
    assert_eq!(appearance.read(app).target().snapshot().count, 0);
    previous.update(app, |set, _| set.retire());
    bind(&mut running, request, appearance, app).unwrap();
    assert_eq!(appearance.read(app).target().snapshot().count, 1);
    assert!(bind(&mut running, request, appearance, app).is_err());
    assert_eq!(appearance.read(app).target().snapshot().count, 1);
    assert!(!drafts.borrow().test_recovery_ready());
    assert!(
        running
            .release_interrupted_exit_drafts(request, appearance, app)
            .is_err()
    );
    assert_eq!(
        original,
        format!("{:?}", running.interrupted_exit_session().unwrap())
    );
    running.interrupted_exit_services_result(request).unwrap();
    assert!(running.test_services().graph().is_none());
    assert_ne!(running.test_process_appearance(), *appearance);
    drop(running);
    assert!(!RunningProcessOwner::finish_exit(owner, request));
}
