use crate::running_owner::RunningShutdownStatus;

pub(super) async fn exercise(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    context: crate::running_owner::ShutdownConfirmationContext,
    choice: Choice,
    cx: &mut gpui::AsyncApp,
) {
    let invoking = context.invoking();
    let intent = context.intent();
    let duplicate_observation = context.observation().clone();
    if choice == Choice::AdmissionAba {
        let resident = owner
            .borrow()
            .test_process()
            .services
            .windows
            .reserve_main_window(beryl_model::WindowId::from_bytes([232; 16]))
            .unwrap();
        drop(resident);
        assert!(
            owner
                .borrow_mut()
                .begin_confirmed_shutdown(context)
                .is_err()
        );
        assert!(owner.borrow().shutdown_status().is_none());
        return;
    }
    owner
        .borrow_mut()
        .begin_confirmed_shutdown(context)
        .unwrap();
    assert_eq!(
        owner.borrow().shutdown_status(),
        Some((invoking, intent, RunningShutdownStatus::AwaitingObservation))
    );
    assert!(
        owner
            .borrow()
            .test_process()
            .services
            .windows
            .reserve_main_window(beryl_model::WindowId::from_bytes([232; 16]))
            .is_err()
    );
    assert!(
        cx.update(|app| RunningProcessOwner::begin_shutdown_confirmation(
            owner,
            invoking,
            ShutdownIntent::ApplicationExit,
            duplicate_observation,
            app,
        ))
        .unwrap()
        .is_err()
    );
    let job = owner
        .borrow_mut()
        .prepare_confirmed_shutdown_observation()
        .unwrap();
    assert_eq!(
        owner.borrow().shutdown_status().unwrap().2,
        RunningShutdownStatus::Observing
    );
    assert!(
        owner
            .borrow_mut()
            .prepare_confirmed_shutdown_observation()
            .is_err()
    );
    assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
    if choice == Choice::AdmissionCancel {
        let completion = cx
            .background_executor()
            .spawn(async move {
                let cancelled = ProjectionCancellationToken::new();
                cancelled.cancel();
                job.collect(&cancelled)
            })
            .await;
        assert!(
            owner
                .borrow_mut()
                .complete_confirmed_shutdown_observation(completion)
                .is_err()
        );
        assert_eq!(
            owner.borrow().shutdown_status().unwrap().2,
            RunningShutdownStatus::AwaitingObservation
        );
        let job = owner
            .borrow_mut()
            .prepare_confirmed_shutdown_observation()
            .unwrap();
        let cancellation = ProjectionCancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let completion = cx
            .background_executor()
            .spawn(async move { job.collect(&worker_cancellation) })
            .await;
        let duplicate = completion.test_duplicate_success();
        cancellation.cancel();
        assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
        owner
            .borrow_mut()
            .discard_confirmed_shutdown_observation(completion)
            .unwrap();
        assert!(
            owner
                .borrow_mut()
                .discard_confirmed_shutdown_observation(duplicate)
                .is_err()
        );
        assert_eq!(
            owner.borrow().shutdown_status().unwrap().2,
            RunningShutdownStatus::AwaitingObservation
        );
        owner.borrow_mut().end_unadmitted_shutdown().unwrap();
        assert!(owner.borrow().shutdown_status().is_none());
        drop(
            owner
                .borrow()
                .test_process()
                .services
                .windows
                .reserve_main_window(beryl_model::WindowId::from_bytes([232; 16]))
                .unwrap(),
        );
        return;
    }
    let completion = cx
        .background_executor()
        .spawn(async move { job.collect(&ProjectionCancellationToken::new()) })
        .await;
    let stale = completion.test_duplicate_success();
    let stale_discard = completion.test_duplicate_success();
    let permit = owner
        .borrow()
        .test_process()
        .services
        .process
        .execution_permit();
    let unsettled = permit.reserve().unwrap();
    assert!(
        owner
            .borrow_mut()
            .complete_confirmed_shutdown_observation(completion)
            .is_err()
    );
    permit.commit(|| ()).unwrap();
    assert_eq!(
        owner.borrow().shutdown_status().unwrap().2,
        RunningShutdownStatus::AwaitingObservation
    );
    drop(unsettled);
    let mut job = owner
        .borrow_mut()
        .prepare_confirmed_shutdown_observation()
        .unwrap();
    assert!(
        owner
            .borrow_mut()
            .discard_confirmed_shutdown_observation(stale_discard)
            .is_err()
    );
    assert!(
        owner
            .borrow_mut()
            .complete_confirmed_shutdown_observation(stale)
            .is_err()
    );
    assert_eq!(
        owner.borrow().shutdown_status().unwrap().2,
        RunningShutdownStatus::Observing
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let completion = cx
            .background_executor()
            .spawn(async move { job.collect(&ProjectionCancellationToken::new()) })
            .await;
        let result = owner
            .borrow_mut()
            .complete_confirmed_shutdown_observation(completion);
        if result.is_ok() {
            break;
        }
        permit.commit(|| ()).unwrap();
        assert!(
            Instant::now() < deadline,
            "confirmed admission did not settle: {result:?}"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
        job = owner
            .borrow_mut()
            .prepare_confirmed_shutdown_observation()
            .unwrap();
    }
    assert_eq!(
        owner.borrow().shutdown_status(),
        Some((invoking, intent, RunningShutdownStatus::Admitted))
    );
    assert!(
        owner
            .borrow_mut()
            .prepare_confirmed_shutdown_observation()
            .is_err()
    );
    assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
}

pub(super) fn reopen_for_disposal(mut services: ProcessServiceOwner) -> ProcessServiceOwner {
    let cancelled = ProjectionCancellationToken::new();
    cancelled.cancel();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match services.poll_shutdown(&cancelled).unwrap() {
            AppServiceShutdownProgress::Failed { reopened: true, .. } => return services,
            AppServiceShutdownProgress::Failed {
                reopened: false, ..
            } => {
                assert!(Instant::now() < deadline, "test shutdown did not reopen");
                std::thread::yield_now();
            }
            progress => panic!("unexpected cancellation progress: {progress:?}"),
        }
    }
}
