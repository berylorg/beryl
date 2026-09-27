use super::*;

mod partial {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services/running_shutdown_draft_partial.rs"
    ));
}

pub(super) async fn prepare_work(owner: &Rc<RefCell<RunningProcessOwner>>, cx: &mut AsyncApp) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.update(|app| {
            RunningProcessOwner::advance_shutdown(
                owner,
                ProjectionCancellationToken::new(),
                app,
                |_, _| {},
            )
        })
        .unwrap()
        .unwrap();
        settled(owner, cx).await;
        match owner
            .borrow_mut()
            .take_shutdown_progress()
            .unwrap()
            .unwrap()
        {
            AppServiceShutdownProgress::Ready => break,
            AppServiceShutdownProgress::Waiting => assert!(Instant::now() < deadline),
            other => panic!("unexpected draft preparation work: {other:?}"),
        }
    }
}

pub(super) async fn exercise_preparation_failure(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    cx: &mut AsyncApp,
) {
    prepare_work(owner, cx).await;
    cx.update(|app| {
        let window = owner.borrow().test_process().windows.shells()[0].window();
        window
            .update(app, |root, _, cx| {
                root.set_shutdown_interaction_gated(false, cx)
            })
            .unwrap()
            .unwrap();
        let first = RunningProcessOwner::advance_shutdown_drafts(owner, app).unwrap_err();
        window
            .update(app, |root, _, cx| {
                root.set_shutdown_interaction_gated(true, cx)
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            RunningProcessOwner::advance_shutdown_drafts(owner, app).unwrap_err(),
            first
        );
        let cancelled = ProjectionCancellationToken::new();
        cancelled.cancel();
        assert!(
            RunningProcessOwner::advance_shutdown(owner, cancelled, app, |_, _| panic!(
                "failed preparation still requires release disposition"
            ))
            .is_err()
        );
        assert_eq!(
            RunningProcessOwner::release_shutdown_drafts(owner, app).unwrap(),
            RunningShutdownDraftProgress::Released
        );
        assert!(RunningProcessOwner::advance_shutdown_drafts(owner, app).is_err());
    })
    .unwrap();
}

pub(super) fn exercise_ready(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    invoking: WindowId,
    intent: ShutdownIntent,
    cancellation: &ProjectionCancellationToken,
    cx: &mut AsyncApp,
) {
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let original_attempt = owner.borrow().test_services().graph().unwrap().shutdown;
    cx.update(|app| {
        RunningProcessOwner::install_shutdown_interaction_gate(&owner, app).unwrap();
        assert!(RunningProcessOwner::release_shutdown_drafts(&owner, app).is_err());
        for _ in 0..2 {
            assert_eq!(
                RunningProcessOwner::advance_shutdown_drafts(&owner, app).unwrap(),
                RunningShutdownDraftProgress::Ready
            );
        }
        assert!(
            RunningProcessOwner::advance_shutdown(
                &owner,
                cancellation.clone(),
                app,
                |_, _| panic!("draft custody must prevent service progress")
            )
            .is_err()
        );
        assert!(!owner.borrow().test_services_on_worker());
        assert_eq!(
            owner.borrow().shutdown_status(),
            Some((invoking, intent, RunningShutdownStatus::WorkReady))
        );
        assert!(RunningProcessOwner::release_shutdown_interaction_gate(&owner, app).is_err());
        window
            .update(app, |root, _, cx| {
                root.set_shutdown_interaction_gated(false, cx)
            })
            .unwrap()
            .unwrap();
        assert!(RunningProcessOwner::release_shutdown_drafts(&owner, app).is_err());
        assert!(
            RunningProcessOwner::advance_shutdown(
                &owner,
                cancellation.clone(),
                app,
                |_, _| panic!("failed draft release must retain custody")
            )
            .is_err()
        );
        window
            .update(app, |root, _, cx| {
                root.set_shutdown_interaction_gated(true, cx)
            })
            .unwrap()
            .unwrap();
        assert!(RunningProcessOwner::advance_shutdown_drafts(&owner, app).is_err());
        for _ in 0..2 {
            assert_eq!(
                RunningProcessOwner::release_shutdown_drafts(&owner, app).unwrap(),
                RunningShutdownDraftProgress::Released
            );
        }
        assert!(RunningProcessOwner::release_shutdown_interaction_gate(&owner, app).is_err());
        assert_eq!(
            owner.borrow().test_services().graph().unwrap().shutdown,
            original_attempt
        );
    })
    .unwrap();
}
