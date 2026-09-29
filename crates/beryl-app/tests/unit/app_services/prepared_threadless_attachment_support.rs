pub(super) fn assert_unavailable(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    app: &mut gpui::App,
) {
    let window = owner.borrow().test_process().windows.shells()[0].window();
    window
        .update(app, |root, window, cx| {
            let error = owner
                .borrow_mut()
                .attach_interrupted_exit_threadless(request, root, &mut None, window, cx)
                .unwrap_err();
            assert!(!error.contains("source is unavailable"), "{error}");
        })
        .unwrap();
}

pub(super) fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    authenticated: ThreadlessRecoveryWindow,
    stale: &mut Option<ThreadlessRecoveryWindow>,
    app: &mut gpui::App,
) {
    let window_handle = owner.borrow().test_process().windows.shells()[0].window();
    let mut source = Some(authenticated);
    let window_id = source.as_ref().unwrap().window().window_id();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let foreign = request.test_foreign();
    window_handle
        .update(app, |root, window, cx| {
            let mut draft = root.begin_shutdown_draft(window, cx).unwrap();
            assert!(root.retire_shutdown_draft(&mut draft, cx).unwrap());
            let drafts = Rc::new(RefCell::new(
                crate::running_owner::RunningShutdownDrafts::test_recovery_drafts(
                    window_handle,
                    draft,
                ),
            ));
            owner
                .borrow_mut()
                .test_replace_recovery_drafts(Some(drafts.clone()));
            let mut attach = |request, source: &mut Option<ThreadlessRecoveryWindow>| {
                owner
                    .borrow_mut()
                    .attach_interrupted_exit_threadless(request, root, source, window, cx)
            };
            assert!(
                attach(&foreign, &mut source)
                    .unwrap_err()
                    .contains("request changed")
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&foreign);
            assert!(
                attach(request, &mut source)
                    .unwrap_err()
                    .contains("request changed")
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
            for retirement in [None, Some(Err("failed retirement".into()))] {
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(retirement);
                assert!(attach(request, &mut source).is_err());
            }
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            assert!(
                attach(request, stale)
                    .unwrap_err()
                    .contains("candidate identity changed")
            );
            assert!(stale.is_some());
            assert!(
                attach(request, &mut None)
                    .unwrap_err()
                    .contains("source is unavailable")
            );
            owner.borrow_mut().test_replace_recovery_drafts(None);
            assert!(
                attach(request, &mut source)
                    .unwrap_err()
                    .contains("drafts are unavailable")
            );
            owner
                .borrow_mut()
                .test_replace_recovery_drafts(Some(drafts.clone()));
            let borrowed = drafts.borrow_mut();
            assert!(
                attach(request, &mut source)
                    .unwrap_err()
                    .contains("drafts are busy")
            );
            drop(borrowed);
            drafts.borrow_mut().test_recovery_driving(true);
            assert!(attach(request, &mut source).is_err());
            drafts.borrow_mut().test_recovery_driving(false);
            assert!(source.is_some());
            assert!(drafts.borrow().test_recovery_ready());
            attach(request, &mut source).unwrap();
            assert!(source.is_none());
            assert!(!drafts.borrow().test_recovery_ready());
            assert!(attach(request, &mut source).is_err());
            assert!(root.controller().unwrap().is_threadless());
            assert_eq!(root.controller().unwrap().window_id(), window_id);
            assert!(root.controller().unwrap().composer_mount().is_none());
            assert!(root.set_shutdown_interaction_gated(false, cx).is_err());
        })
        .unwrap();
    assert_eq!(
        original,
        format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
    );
    owner
        .borrow()
        .interrupted_exit_services_result(request)
        .unwrap();
    assert!(owner.borrow().test_services().graph().is_none());
    assert!(!RunningProcessOwner::finish_exit(owner, request));
}
