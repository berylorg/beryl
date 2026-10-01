#[test]
fn native_exit_threadless_recovery_drives_failed_graph_to_running() {
    run_composed(FaultPoint::BeforeCommit, RetirementDelivery::Ready);
}

#[test]
fn native_exit_threadless_recovery_drives_committed_exit_to_running() {
    run_composed(FaultPoint::AfterPersist, RetirementDelivery::Ready);
}

#[test]
fn native_exit_threadless_recovery_continues_cancelled_attachment() {
    run_composed(FaultPoint::BeforeCommit, RetirementDelivery::Cancelled);
}

#[test]
fn native_exit_threadless_recovery_continues_stale_attachment() {
    run_composed(FaultPoint::BeforeCommit, RetirementDelivery::Stale);
}

#[test]
fn native_exit_threadless_recovery_continues_dropped_attachment() {
    run_composed(FaultPoint::BeforeCommit, RetirementDelivery::Dropped);
}

fn run_composed(fault: FaultPoint, delivery: RetirementDelivery) {
    run_with_recovery_delivery(
        Some(fault),
        true,
        false,
        RecoveryPublicationDelivery::Whole(delivery),
    );
}

pub(super) async fn verify_and_dispose(
    owner: Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    delivery: RetirementDelivery,
    faults: &FaultController,
    cx: &mut AsyncApp,
) {
    use crate::theme_runtime::AppearancePublicationTarget;
    use beryl_home_store::CommandCancellation;
    use std::{
        future::Future,
        task::Poll,
        time::{Duration, Instant},
    };

    let (home, retired) = {
        let owner = owner.borrow();
        let graph = owner.test_services().graph().unwrap();
        (
            graph.home().home_id(),
            graph.home().health().generation().unwrap(),
        )
    };
    let original = publication_evidence(&owner.borrow().interrupted_exit_session().unwrap());
    let previous = owner.borrow().test_process_appearance();
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let foreign = request.test_foreign();
    cx.update(|app| {
        let result = owner
            .borrow_mut()
            .interrupted_exit_resident_windows(request, app, |_| {
                panic!("threadless window configuration")
            });
        assert_eq!(
            result.err().unwrap(),
            "Interrupted Exit requires selected windows"
        );
    })
    .unwrap();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    for (supplied, cancellation, occupied) in [
        (&foreign, CommandCancellation::new(), false),
        (request, cancelled, false),
        (request, CommandCancellation::new(), true),
    ] {
        let mut appearance = occupied.then(|| previous.clone());
        assert!(
            RunningProcessOwner::recover_interrupted_exit_threadless(
                &owner,
                supplied,
                retired,
                window,
                configuration(),
                SyndicTimestamp::from_unix_millis(2),
                cancellation,
                |_| panic!("refused preparation failure"),
                &mut appearance,
                cx,
            )
            .await
            .is_err()
        );
        assert_eq!(appearance, occupied.then(|| previous.clone()));
        assert!(owner.borrow().test_services().graph().is_some());
        assert!(!owner.borrow().test_services_on_worker());
        cx.update(|app| {
            assert!(previous.read(app).target().snapshot().active);
            assert!(!window.read(app).unwrap().test_shell_construction_retired());
        })
        .unwrap();
    }

    faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
    let failures = std::cell::Cell::new(0);
    let cancellation = CommandCancellation::new();
    let mut appearance = None;
    let mut drive_cx = cx.clone();
    let mut drive = Box::pin(RunningProcessOwner::recover_interrupted_exit_threadless(
        &owner,
        request,
        retired,
        window,
        configuration(),
        SyndicTimestamp::from_unix_millis(2),
        cancellation.clone(),
        |failure| {
            let crate::running_owner::RecoveryPreparationFailure::Services(failure) = failure
            else {
                panic!("unexpected recovery outcome");
            };
            let crate::app_services::recovery_graph::RecoveryServicePreparationError::App(failure) =
                failure
            else {
                panic!("unexpected service failure");
            };
            assert!(matches!(failure.error(), AppServiceOpenError::Theme(_)));
            assert!(failure.into_retry_parts().is_err());
            failures.set(failures.get() + 1);
            assert_eq!(failures.get(), 1);
            assert_eq!(
                original,
                publication_evidence(&owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(!RunningProcessOwner::finish_exit(&owner, request));
        },
        &mut appearance,
        &mut drive_cx,
    ));
    let timeout = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            std::future::poll_fn(|task| Poll::Ready(drive.as_mut().poll(task)))
                .await
                .is_pending()
        );
        assert_reserved(&owner, request, retired, cx).await;
        if !cx
            .update(|app| previous.read(app).target().snapshot().active)
            .unwrap()
        {
            break;
        }
        assert!(
            Instant::now() < timeout,
            "recovery did not reach attachment"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert_eq!(failures.get(), 1);
    let mut drive = Some(drive);
    if matches!(delivery, RetirementDelivery::Dropped) {
        drop(drive.take());
    }
    while owner
        .borrow()
        .interrupted_exit_services_result(request)
        .is_err()
    {
        assert!(Instant::now() < timeout);
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    if drive.is_some() {
        assert_reserved(&owner, request, retired, cx).await;
    }
    match delivery {
        RetirementDelivery::Cancelled => cancellation.cancel(),
        RetirementDelivery::Stale => owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(&foreign),
        _ => {}
    }
    if let Some(drive) = drive.take() {
        let result = drive.await;
        match delivery {
            RetirementDelivery::Ready => result.unwrap(),
            RetirementDelivery::Cancelled => assert!(result.unwrap_err().contains("cancelled")),
            RetirementDelivery::Stale => {
                assert!(result.unwrap_err().contains("request changed"));
                owner
                    .borrow_mut()
                    .test_replace_interrupted_exit_request(request);
            }
            RetirementDelivery::Dropped => unreachable!(),
        }
    }
    drop(drive);
    let appearance = appearance.unwrap();
    if !matches!(delivery, RetirementDelivery::Ready) {
        assert_eq!(
            original,
            publication_evidence(&owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(owner.borrow().test_services().graph().is_none());
        assert!(owner.borrow().exit_requested());
        assert!(!RunningProcessOwner::finish_exit(&owner, request));
        cx.update(|app| {
            assert_eq!(appearance.read(app).target().snapshot().count, 0);
            assert_eq!(
                window.read(app).unwrap().test_exit_presentation().0,
                "Exiting…"
            );
        })
        .unwrap();
        let generation = owner
            .borrow()
            .interrupted_exit_appearance(request)
            .unwrap()
            .prepared()
            .home()
            .home_generation();
        RunningProcessOwner::attach_and_complete_interrupted_exit_threadless(
            &owner,
            request,
            home,
            retired,
            generation,
            window,
            &appearance,
            CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap();
    }
    assert!(owner.borrow().interrupted_exit_session().is_none());
    assert!(owner.borrow().shutdown_status().is_none());
    assert!(!owner.borrow().exit_requested());
    assert!(!RunningProcessOwner::finish_exit(&owner, request));
    assert_eq!(owner.borrow().test_process_appearance(), appearance);
    cx.update(|app| {
        assert_eq!(
            owner.borrow().test_process().windows.shells()[0].window(),
            window
        );
        assert_eq!(window.read(app).unwrap().test_exit_presentation().0, "Exit");
        assert!(!window.read(app).unwrap().test_notices_inert());
        assert_eq!(appearance.read(app).target().snapshot().count, 1);
    })
    .unwrap();
    let running = Rc::try_unwrap(owner)
        .ok()
        .unwrap()
        .into_inner()
        .test_into_process();
    dispose_failed_fixture(running, true, false, Some(appearance), cx).await;
}

fn publication_evidence(session: &RunningShutdownSession) -> String {
    let publication = match session {
        RunningShutdownSession::Settled(Ok(outcome)) => outcome.publication(),
        RunningShutdownSession::Reconciled(outcome) => outcome.publication(),
        RunningShutdownSession::Resuming(resume) => resume.exit().publication(),
        _ => panic!("expected retained original Exit evidence"),
    };
    format!("{publication:?}")
}

async fn assert_reserved(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    cx: &mut AsyncApp,
) {
    assert!(
        RunningProcessOwner::retry_interrupted_exit_preparation(
            owner,
            request,
            retired,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            beryl_home_store::CommandCancellation::new(),
            |_| panic!("competing preparation"),
            cx,
        )
        .await
        .unwrap_err()
        .contains("already being driven")
    );
    assert!(
        RunningProcessOwner::await_interrupted_exit_completion(
            owner,
            request,
            beryl_home_store::CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap_err()
        .contains("already being driven")
    );
}
