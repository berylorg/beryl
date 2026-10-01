pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    mut previous_delay: u64,
    cx: &mut AsyncApp,
) -> u64 {
    use beryl_home_store::CommandCancellation;
    use std::{
        future::Future,
        task::Poll,
        time::{Duration, Instant},
    };

    for stage in ["constructed", "stale", "settled", "abandoned", "services"] {
        let cancellation = CommandCancellation::new();
        let mut drive_cx = cx.clone();
        let mut drive = Box::pin(RunningProcessOwner::prepare_retired_interrupted_exit(
            owner,
            request,
            retired,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            cancellation.clone(),
            &mut drive_cx,
        ));
        let timeout = Instant::now() + Duration::from_secs(5);
        loop {
            std::future::poll_fn(|task| {
                assert!(drive.as_mut().poll(task).is_pending());
                Poll::Ready(())
            })
            .await;
            loop {
                let ready = match stage {
                    "constructed" | "stale" => owner
                        .borrow()
                        .interrupted_exit_construction_result(request)
                        .is_ok(),
                    "settled" | "abandoned" => owner
                        .borrow()
                        .interrupted_exit_candidate_result(request)
                        .is_ok(),
                    _ => owner
                        .borrow()
                        .interrupted_exit_services_result(request)
                        .is_ok(),
                };
                if ready {
                    break;
                }
                assert!(
                    Instant::now() < timeout,
                    "preparation stage did not return: {stage}"
                );
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
                if !owner.borrow().test_services_on_worker()
                    && owner.borrow().interrupted_exit_session().is_some()
                {
                    break;
                }
            }
            let ready = match stage {
                "constructed" | "stale" => owner
                    .borrow()
                    .interrupted_exit_construction_result(request)
                    .is_ok(),
                "settled" | "abandoned" => owner
                    .borrow()
                    .interrupted_exit_candidate_result(request)
                    .is_ok(),
                _ => owner
                    .borrow()
                    .interrupted_exit_services_result(request)
                    .is_ok(),
            };
            if ready {
                break;
            }
        }
        let evidence = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
        cancellation.cancel();
        if stage == "stale" {
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&request.test_foreign());
            assert!(drive.await.unwrap_err().contains("request changed"));
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
            owner
                .borrow()
                .interrupted_exit_construction_result(request)
                .unwrap();
            let (sender, receiver) = futures_channel::oneshot::channel();
            cx.update(|app| {
                RunningProcessOwner::abort_constructed_exit_candidate(
                    owner,
                    request,
                    retired,
                    app,
                    move |_, _| {
                        let _ = sender.send(());
                    },
                )
            })
            .unwrap()
            .unwrap();
            receiver.await.unwrap();
        } else if stage == "abandoned" {
            std::future::poll_fn(|task| {
                assert!(drive.as_mut().poll(task).is_pending());
                Poll::Ready(())
            })
            .await;
            assert!(owner.borrow().test_services_on_worker());
            drop(drive);
            while owner.borrow().test_services_on_worker() {
                assert!(Instant::now() < timeout);
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
            }
        } else {
            assert!(drive.await.unwrap_err().contains("cancelled"));
        }
        assert!(!owner.borrow().test_services_on_worker());
        assert!(owner.borrow().test_services().graph().is_none());
        assert_eq!(
            evidence,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        if matches!(stage, "settled" | "abandoned") {
            assert!(
                owner
                    .borrow_mut()
                    .take_interrupted_exit_candidate_failure(&request.test_foreign())
                    .is_err()
            );
            assert!(
                owner
                    .borrow_mut()
                    .take_interrupted_exit_candidate_failure(request)
                    .unwrap()
                    .to_string()
                    .contains("cancelled")
            );
            assert!(
                owner
                    .borrow_mut()
                    .take_interrupted_exit_candidate_failure(request)
                    .is_err()
            );
        } else if stage == "services" {
            let failure = owner
                .borrow_mut()
                .take_interrupted_exit_preparation_failure(request, retired)
                .unwrap();
            let crate::app_services::recovery_graph::RecoveryServicePreparationError::App(failure) =
                failure
            else {
                panic!("unexpected cancellation failure");
            };
            assert!(matches!(failure.error(), AppServiceOpenError::Cancelled));
            assert!(failure.into_retry_parts().is_err());
        }
        let deadline = owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap();
        if matches!(stage, "constructed" | "stale") {
            assert!(deadline.is_none());
        } else {
            previous_delay = retry_delay::verify(
                owner,
                request,
                retired,
                deadline.unwrap(),
                Some(previous_delay),
                cx,
            )
            .await;
        }
    }
    previous_delay
}
