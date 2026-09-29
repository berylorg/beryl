pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    candidate: InterruptedExitCandidate,
    appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    cx: &mut AsyncApp,
) -> InterruptedExitCandidate {
    let mut candidate = Some(candidate);
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        assert!(
            owner
                .borrow()
                .release_interrupted_exit_drafts(request, appearance, app)
                .is_err()
        );
        assert!(
            RunningProcessOwner::revalidate_interrupted_exit_candidate(
                owner,
                request,
                app,
                |_, _| panic!("no settlement"),
            )
            .is_err()
        );
        RunningProcessOwner::settle_interrupted_exit_candidate(
            owner,
            request,
            &mut candidate,
            app,
            move |_, _| {
                sender.send(()).unwrap();
            },
        )
        .unwrap();
    })
    .unwrap();
    receiver.await.unwrap();
    owner
        .borrow()
        .interrupted_exit_candidate_result(request)
        .unwrap();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let foreign = request.test_foreign();
    cx.update(|app| {
        use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};
        let mut running = owner.borrow_mut();
        assert!(
            running
                .release_interrupted_exit_drafts(&foreign, appearance, app)
                .is_err()
        );
        let old = running.test_process_appearance();
        assert!(
            running
                .release_interrupted_exit_drafts(request, &old, app)
                .is_err()
        );
        let impostor = GpuiAppearanceWindowSet::new(
            appearance.read(app).target().snapshot().current,
            NonZeroUsize::new(4).unwrap(),
            app,
        );
        assert!(
            running
                .release_interrupted_exit_drafts(request, &impostor, app)
                .is_err()
        );
        impostor.update(app, |owner, _| owner.retire());
        let drafts = running.test_replace_recovery_drafts(None).unwrap();
        assert!(
            running
                .release_interrupted_exit_drafts(request, appearance, app)
                .is_err()
        );
        running.test_replace_recovery_drafts(Some(drafts.clone()));
        let borrow = drafts.borrow_mut();
        assert!(
            running
                .release_interrupted_exit_drafts(request, appearance, app)
                .is_err()
        );
        drop(borrow);
        drafts.borrow_mut().test_recovery_driving(true);
        assert!(
            running
                .release_interrupted_exit_drafts(request, appearance, app)
                .is_err()
        );
        drafts.borrow_mut().test_recovery_driving(false);
        for _ in 0..2 {
            assert!(
                running
                    .release_interrupted_exit_drafts(request, appearance, app)
                    .unwrap()
            );
        }
        assert!(!drafts.borrow().test_recovery_ready());
    })
    .unwrap();
    for unwind in [false, true] {
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            assert!(
                RunningProcessOwner::revalidate_interrupted_exit_candidate(
                    owner,
                    &foreign,
                    app,
                    |_, _| panic!("foreign revalidation"),
                )
                .unwrap_err()
                .contains("request changed")
            );
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Err("failed retirement".into())));
            assert_eq!(
                RunningProcessOwner::revalidate_interrupted_exit_candidate(
                    owner,
                    request,
                    app,
                    |_, _| panic!("failed retirement"),
                )
                .unwrap_err(),
                "failed retirement"
            );
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            RunningProcessOwner::test_revalidate_interrupted_exit_candidate(
                owner,
                request,
                app,
                move |_, _| {
                    sender.send(()).unwrap();
                },
                move |_| {
                    if unwind {
                        panic!("injected revalidation unwind");
                    }
                },
            )
            .unwrap();
            assert!(owner.borrow().interrupted_exit_session().is_none());
            assert!(
                owner
                    .borrow()
                    .release_interrupted_exit_drafts(request, appearance, app)
                    .is_err()
            );
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_candidate_result(request)
                    .is_err()
            );
            assert!(
                RunningProcessOwner::revalidate_interrupted_exit_candidate(
                    owner,
                    request,
                    app,
                    |_, _| panic!("duplicate revalidation"),
                )
                .is_err()
            );
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&foreign);
        })
        .unwrap();
        receiver.await.unwrap();
        assert!(
            owner
                .borrow()
                .interrupted_exit_candidate_result(request)
                .unwrap_err()
                .contains("request changed")
        );
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(request);
        let result = owner.borrow().interrupted_exit_candidate_result(request);
        cx.update(|app| {
            assert_eq!(
                owner
                    .borrow()
                    .release_interrupted_exit_drafts(request, appearance, app)
                    .is_err(),
                unwind
            );
        })
        .unwrap();
        if unwind {
            assert!(result.unwrap_err().contains("unwound"));
            cx.update(|app| {
                assert!(
                    RunningProcessOwner::revalidate_interrupted_exit_candidate(
                        owner,
                        request,
                        app,
                        |_, _| panic!("failed settlement"),
                    )
                    .unwrap_err()
                    .contains("no successful settlement")
                );
            })
            .unwrap();
        } else {
            result.unwrap();
        }
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
    }
    owner.borrow().test_take_interrupted_exit_candidate()
}
