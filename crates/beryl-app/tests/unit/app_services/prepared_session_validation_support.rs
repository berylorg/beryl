pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    home: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    retired: beryl_home_store::HomeGeneration,
    cx: &mut AsyncApp,
) {
    use beryl_home_store::CommandCancellation;
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let appearance = owner.borrow().interrupted_exit_appearance(request).unwrap();
    let foreign = request.test_foreign();
    for mode in ["ready", "stale", "cancel", "repeat"] {
        let cancellation = CommandCancellation::new();
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            let refused = |request, home, generation, cancellation, app: &mut gpui::App| {
                RunningProcessOwner::revalidate_interrupted_exit_services(
                    owner,
                    request,
                    home,
                    generation,
                    cancellation,
                    app,
                    |_, _, _| panic!("refused session validation callback"),
                )
            };
            assert!(refused(&foreign, home, generation, CommandCancellation::new(), app).is_err());
            assert!(refused(request, home, retired, CommandCancellation::new(), app).is_err());
            assert!(
                refused(
                    request,
                    beryl_model::BerylHomeId::from_bytes([99; 16]),
                    generation,
                    CommandCancellation::new(),
                    app
                )
                .is_err()
            );
            let cancelled = CommandCancellation::new();
            cancelled.cancel();
            assert!(refused(request, home, generation, cancelled, app).is_err());
            for retirement in [None, Some(Err("failed retirement".into()))] {
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(retirement);
                assert!(
                    refused(request, home, generation, CommandCancellation::new(), app).is_err()
                );
            }
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            RunningProcessOwner::revalidate_interrupted_exit_services(
                owner,
                request,
                home,
                generation,
                cancellation.clone(),
                app,
                move |owner, result, _| {
                    assert!(owner.borrow().interrupted_exit_session().is_some());
                    assert!(!owner.borrow().test_services_on_worker());
                    assert_eq!(
                        owner.borrow().test_interrupted_exit_service_validation(),
                        Some(result.clone())
                    );
                    sender.send(result).unwrap();
                },
            )
            .unwrap();
            assert!(owner.borrow().interrupted_exit_session().is_none());
            assert!(
                owner
                    .borrow()
                    .test_interrupted_exit_service_validation()
                    .is_none()
            );
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_services_result(request)
                    .is_err()
            );
            assert!(refused(request, home, generation, CommandCancellation::new(), app).is_err());
            assert!(
                RunningProcessOwner::cancel_interrupted_exit_services(
                    owner,
                    request,
                    app,
                    |_, _| panic!("overlapping disposal"),
                )
                .is_err()
            );
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            if mode == "stale" {
                owner
                    .borrow_mut()
                    .test_replace_interrupted_exit_request(&foreign);
            } else if mode == "cancel" {
                cancellation.cancel();
            }
        })
        .unwrap();
        let result = receiver.await.unwrap();
        match mode {
            "stale" => {
                assert!(result.unwrap_err().contains("request changed"));
                owner
                    .borrow_mut()
                    .test_replace_interrupted_exit_request(request);
            }
            "cancel" => assert!(result.unwrap_err().contains("cancelled")),
            _ => result.unwrap(),
        }
        owner
            .borrow()
            .interrupted_exit_services_result(request)
            .unwrap();
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(std::sync::Arc::ptr_eq(
            &appearance,
            &owner.borrow().interrupted_exit_appearance(request).unwrap()
        ));
        assert!(owner.borrow().require_shutdown_session_ready().is_err());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
    }
}
