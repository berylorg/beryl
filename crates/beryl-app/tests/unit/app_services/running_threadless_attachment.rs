use crate::app_services::recovery_threadless::ThreadlessRecoveryWindow;
use crate::running_owner::InterruptedExitCandidate;

mod native_appearance {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/native_shell_appearance.rs"
    ));
}

pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    foreign_candidate: &InterruptedExitCandidate,
    cx: &mut AsyncApp,
) -> Arc<crate::theme_runtime::AppearanceGeneration> {
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let window_id = window
        .read_with(cx, |root, _| root.controller().unwrap().window_id())
        .unwrap();
    let home = owner.borrow_mut().test_take_retired_recovery_home();
    let (candidate, mut source, appearance) = cx
        .background_executor()
        .spawn(async move {
            let home_id = home.home_id();
            let generation = home.health().generation().unwrap();
            let mut candidate = home.recover_same_home().unwrap();
            let state = BerylState::reacquire_candidate(&candidate).unwrap();
            let access = candidate.recovery_access().unwrap();
            let before = access.home_revision().unwrap();
            let source =
                ThreadlessRecoveryWindow::prepare(&access, &state, home_id, generation, window_id)
                    .unwrap();
            source.revalidate(&access, &state).unwrap();
            assert_eq!(before, access.home_revision().unwrap());
            let appearance = crate::theme_runtime::AppearanceCoordinator::new(
                crate::theme_runtime::AppearanceCoordinatorConfig::new(
                    NonZeroUsize::new(4).unwrap(),
                ),
                native_appearance::system_font_appearance(&state),
            )
            .current();
            (
                InterruptedExitCandidate {
                    candidate,
                    session: state.session(),
                },
                Some(source),
                appearance,
            )
        })
        .await;
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let foreign = request.test_foreign();
    window
        .update(cx, |root, window, app| {
            let mut attach =
                |owner: &mut RunningProcessOwner,
                 request: &crate::startup_owner::RunningExitRequest,
                 candidate: &InterruptedExitCandidate,
                 source: &mut Option<ThreadlessRecoveryWindow>| {
                    owner.attach_interrupted_exit_threadless(
                        request, candidate, root, source, window, app,
                    )
                };
            assert!(
                attach(&mut owner.borrow_mut(), &foreign, &candidate, &mut source)
                    .unwrap_err()
                    .contains("request changed")
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&foreign);
            assert!(
                attach(&mut owner.borrow_mut(), request, &candidate, &mut source)
                    .unwrap_err()
                    .contains("request changed")
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
            owner.borrow().test_set_resident_graph_retirement(None);
            assert!(
                attach(&mut owner.borrow_mut(), request, &candidate, &mut source)
                    .unwrap_err()
                    .contains("has not returned")
            );
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Err("failed retirement".into())));
            assert_eq!(
                attach(&mut owner.borrow_mut(), request, &candidate, &mut source).unwrap_err(),
                "failed retirement"
            );
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            assert!(
                attach(
                    &mut owner.borrow_mut(),
                    request,
                    foreign_candidate,
                    &mut source
                )
                .unwrap_err()
                .contains("candidate identity changed")
            );
            assert!(
                attach(&mut owner.borrow_mut(), request, &candidate, &mut None)
                    .unwrap_err()
                    .contains("source is unavailable")
            );
            let drafts = owner
                .borrow_mut()
                .test_replace_recovery_drafts(None)
                .unwrap();
            assert!(
                attach(&mut owner.borrow_mut(), request, &candidate, &mut source)
                    .unwrap_err()
                    .contains("drafts are unavailable")
            );
            owner
                .borrow_mut()
                .test_replace_recovery_drafts(Some(drafts.clone()));
            let borrowed = drafts.borrow_mut();
            assert!(
                attach(&mut owner.borrow_mut(), request, &candidate, &mut source)
                    .unwrap_err()
                    .contains("drafts are busy")
            );
            drop(borrowed);
            assert!(source.is_some());
            assert!(drafts.borrow().test_recovery_ready());
            attach(&mut owner.borrow_mut(), request, &candidate, &mut source).unwrap();
            assert!(source.is_none());
            assert!(!drafts.borrow().test_recovery_ready());
            assert!(attach(&mut owner.borrow_mut(), request, &candidate, &mut source).is_err());
            assert!(!drafts.borrow().test_recovery_ready());
            assert!(root.controller().unwrap().is_threadless());
            assert_eq!(root.controller().unwrap().window_id(), window_id);
            assert!(root.controller().unwrap().composer_mount().is_none());
            assert!(
                root.set_shutdown_interaction_gated(false, app)
                    .unwrap_err()
                    .contains("fresh appearance")
            );
        })
        .unwrap();
    assert_eq!(
        original,
        format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
    );
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    cx.background_executor()
        .spawn(async move {
            drop(candidate.session);
            candidate.candidate.abort().close().unwrap();
        })
        .await;
    appearance
}
