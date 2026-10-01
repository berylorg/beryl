mod publication_continuation {
    use super::*;
    include!("threadless_publication_continuation_support.rs");
}

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
    for (supplied, cancellation) in [(&foreign, CommandCancellation::new()), (request, cancelled)] {
        assert!(
            RunningProcessOwner::recover_interrupted_exit(
                &owner,
                supplied,
                SyndicTimestamp::from_unix_millis(2),
                cancellation,
                |_| panic!("unexpected configuration factory"),
                |_| panic!("refused preparation failure"),
                cx,
            )
            .await
            .is_err()
        );
        assert!(
            owner
                .borrow()
                .test_threadless_recovery_appearance()
                .is_none()
        );
        assert!(owner.borrow().test_services().graph().is_some());
        assert!(!owner.borrow().test_services_on_worker());
        cx.update(|app| {
            assert!(previous.read(app).target().snapshot().active);
            assert!(!window.read(app).unwrap().test_shell_construction_retired());
        })
        .unwrap();
    }
    for supplied in [&foreign, request] {
        assert_retired_refused(&owner, supplied, CommandCancellation::new(), cx).await;
        assert!(
            RunningProcessOwner::recover_prepared_interrupted_exit(
                &owner,
                supplied,
                CommandCancellation::new(),
                cx,
            )
            .await
            .is_err()
        );
        publication_continuation::assert_refused(&owner, supplied, cx).await;
        assert!(
            RunningProcessOwner::recover_resident_interrupted_exit(
                &owner,
                supplied,
                CommandCancellation::new(),
                cx,
            )
            .await
            .is_err()
        );
    }

    faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
    let failures = std::cell::Cell::new(0);
    let cancellation = CommandCancellation::new();
    let failed = |failure| {
        let crate::running_owner::RecoveryPreparationFailure::Services(failure) = failure else {
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
    };
    let mut drive_cx = cx.clone();
    let mut drive: std::pin::Pin<Box<dyn Future<Output = Result<(), String>> + '_>> =
        Box::pin(RunningProcessOwner::recover_interrupted_exit(
            &owner,
            request,
            SyndicTimestamp::from_unix_millis(2),
            cancellation.clone(),
            |_| panic!("threadless configuration factory"),
            &failed,
            &mut drive_cx,
        ));
    let timeout = Instant::now() + Duration::from_secs(10);
    let mut prepared_continued = false;
    let mut retired_continued = false;
    loop {
        if matches!(delivery, RetirementDelivery::Ready)
            && !retired_continued
            && owner
                .borrow()
                .interrupted_exit_graph_retirement_result(request)
                .is_ok()
        {
            drop(drive);
            assert_retired_refused(&owner, &foreign, CommandCancellation::new(), cx).await;
            let cancelled = CommandCancellation::new();
            cancelled.cancel();
            assert_retired_refused(&owner, request, cancelled, cx).await;
            assert!(
                owner
                    .borrow()
                    .test_threadless_recovery_appearance()
                    .is_none()
            );
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_services_result(request)
                    .is_err()
            );
            assert_eq!(
                original,
                publication_evidence(&owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(!RunningProcessOwner::finish_exit(&owner, request));
            cx.update(|app| assert!(previous.read(app).target().snapshot().active))
                .unwrap();
            drive = Box::pin(RunningProcessOwner::recover_retired_interrupted_exit(
                &owner,
                request,
                SyndicTimestamp::from_unix_millis(2),
                cancellation.clone(),
                &failed,
                &mut drive_cx,
            ));
            retired_continued = true;
        }
        if matches!(delivery, RetirementDelivery::Ready)
            && !prepared_continued
            && owner
                .borrow()
                .interrupted_exit_services_result(request)
                .is_ok()
        {
            drop(drive);
            assert_retired_refused(&owner, request, CommandCancellation::new(), cx).await;
            assert!(
                owner
                    .borrow()
                    .test_threadless_recovery_appearance()
                    .is_none()
            );
            let prepared = owner.borrow().interrupted_exit_appearance(request).unwrap();
            let home = prepared.prepared().home();
            let services = owner.borrow();
            assert_eq!(
                services
                    .test_services()
                    .retired_service_generation_for_home_return(home.home_id())
                    .unwrap(),
                retired
            );
            assert!(
                services
                    .test_services()
                    .validate_retired_service_home_return(
                        home.home_generation(),
                        Some(home.home_id())
                    )
                    .is_err()
            );
            assert!(
                services
                    .test_services()
                    .retired_service_generation()
                    .is_err()
            );
            drop(services);
            let cancelled = CommandCancellation::new();
            cancelled.cancel();
            for (supplied, cancel) in [(&foreign, CommandCancellation::new()), (request, cancelled)]
            {
                assert!(
                    RunningProcessOwner::recover_prepared_interrupted_exit(
                        &owner, supplied, cancel, cx,
                    )
                    .await
                    .is_err()
                );
                assert!(
                    owner
                        .borrow()
                        .test_threadless_recovery_appearance()
                        .is_none()
                );
                assert!(
                    owner
                        .borrow()
                        .interrupted_exit_services_result(request)
                        .is_ok()
                );
                assert_eq!(
                    original,
                    publication_evidence(&owner.borrow().interrupted_exit_session().unwrap())
                );
                assert!(owner.borrow().exit_requested());
                assert!(!RunningProcessOwner::finish_exit(&owner, request));
                cx.update(|app| assert!(previous.read(app).target().snapshot().active))
                    .unwrap();
            }
            drive = Box::pin(RunningProcessOwner::recover_prepared_interrupted_exit(
                &owner,
                request,
                cancellation.clone(),
                &mut drive_cx,
            ));
            prepared_continued = true;
        }
        assert!(
            std::future::poll_fn(|task| Poll::Ready(drive.as_mut().poll(task)))
                .await
                .is_pending()
        );
        assert_reserved(&owner, request, retired, cx).await;
        assert!(
            RunningProcessOwner::recover_resident_interrupted_exit(
                &owner,
                request,
                CommandCancellation::new(),
                cx,
            )
            .await
            .unwrap_err()
            .contains("already being driven")
        );
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
    assert_eq!(
        prepared_continued,
        matches!(delivery, RetirementDelivery::Ready)
    );
    assert_eq!(failures.get(), 1);
    assert_eq!(
        retired_continued,
        matches!(delivery, RetirementDelivery::Ready)
    );
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
    let appearance = if matches!(delivery, RetirementDelivery::Ready) {
        owner.borrow().test_process_appearance()
    } else {
        owner
            .borrow()
            .test_threadless_recovery_appearance()
            .unwrap()
    };
    if !matches!(delivery, RetirementDelivery::Ready) {
        assert!(
            RunningProcessOwner::recover_prepared_interrupted_exit(
                &owner,
                request,
                CommandCancellation::new(),
                cx,
            )
            .await
            .unwrap_err()
            .contains("already retained")
        );
        assert!(
            RunningProcessOwner::recover_interrupted_exit(
                &owner,
                request,
                SyndicTimestamp::from_unix_millis(2),
                CommandCancellation::new(),
                |_| panic!("unexpected configuration factory"),
                |_| panic!("repeated initial preparation"),
                cx,
            )
            .await
            .unwrap_err()
            .contains("original service graph is unavailable")
        );
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
        let cancelled = CommandCancellation::new();
        cancelled.cancel();
        for (supplied, cancellation) in
            [(&foreign, CommandCancellation::new()), (request, cancelled)]
        {
            assert!(
                RunningProcessOwner::recover_resident_interrupted_exit(
                    &owner,
                    supplied,
                    cancellation,
                    cx,
                )
                .await
                .is_err()
            );
            assert_eq!(
                owner.borrow().test_threadless_recovery_appearance(),
                Some(appearance.clone())
            );
        }
        publication_continuation::verify(&owner, request, cx).await;
        assert_eq!(
            owner
                .borrow()
                .test_services()
                .graph()
                .unwrap()
                .home()
                .health()
                .generation()
                .unwrap(),
            generation
        );
        assert_eq!(
            owner
                .borrow()
                .test_services()
                .graph()
                .unwrap()
                .home()
                .home_id(),
            home
        );
    }
    assert!(owner.borrow().interrupted_exit_session().is_none());
    assert!(
        owner
            .borrow()
            .test_threadless_recovery_appearance()
            .is_none()
    );
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

async fn assert_retired_refused(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    cancellation: beryl_home_store::CommandCancellation,
    cx: &mut AsyncApp,
) {
    assert!(
        RunningProcessOwner::recover_retired_interrupted_exit(
            owner,
            request,
            SyndicTimestamp::from_unix_millis(2),
            cancellation,
            |_| panic!("refused retired threadless preparation"),
            cx,
        )
        .await
        .is_err()
    );
}

async fn assert_reserved(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    cx: &mut AsyncApp,
) {
    let window = owner.borrow().test_process().windows.shells()[0].window();
    assert_retired_refused(
        owner,
        request,
        beryl_home_store::CommandCancellation::new(),
        cx,
    )
    .await;
    assert!(
        RunningProcessOwner::recover_prepared_interrupted_exit(
            owner,
            request,
            beryl_home_store::CommandCancellation::new(),
            cx,
        )
        .await
        .unwrap_err()
        .contains("already being driven")
    );
    assert!(
        RunningProcessOwner::recover_interrupted_exit(
            owner,
            request,
            SyndicTimestamp::from_unix_millis(2),
            beryl_home_store::CommandCancellation::new(),
            |_| panic!("unexpected configuration factory"),
            |_| panic!("competing initial preparation"),
            cx,
        )
        .await
        .unwrap_err()
        .contains("already being driven")
    );
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
