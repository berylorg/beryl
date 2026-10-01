pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    fault: beryl_home_store::test_faults::FaultPoint,
    revision: beryl_model::SessionRevision,
    faults: &beryl_home_store::test_faults::FaultController,
    cx: &mut AsyncApp,
) {
    use crate::exit_session::ResumeSessionOutcome;
    use crate::running_owner::RecoveryPreparationFailure;
    use beryl_home_store::test_faults::FaultPoint;
    use std::cell::Cell;

    let delivered = Cell::new(0);
    let candidate_failures = Cell::new(0);
    match fault {
        FaultPoint::BeforeCommit => faults.fail_next(FaultPoint::BeforeCommit),
        FaultPoint::AfterCommitBeforePersist => {
            faults.fail_next(FaultPoint::BeforeReconciliationSnapshot)
        }
        _ => {}
    }
    let cancellation = CommandCancellation::new();
    let result = RunningProcessOwner::retry_interrupted_exit_preparation(
        owner,
        request,
        retired,
        configuration(),
        SyndicTimestamp::from_unix_millis(2),
        cancellation.clone(),
        |failure| {
            assert!(!owner.borrow().test_services_on_worker());
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            match (fault, failure) {
                (
                    FaultPoint::BeforeCommit,
                    RecoveryPreparationFailure::Resume(ResumeSessionOutcome::NotCommitted {
                        ..
                    }),
                ) => {
                    delivered.set(delivered.get() + 1);
                    if delivered.get() == 2 {
                        cancellation.cancel();
                    }
                }
                (
                    FaultPoint::AfterCommitBeforePersist,
                    RecoveryPreparationFailure::ResumeReconciliation(_),
                ) => {
                    delivered.set(delivered.get() + 1);
                    if delivered.get() == 2 {
                        owner
                            .borrow_mut()
                            .test_replace_interrupted_exit_request(&request.test_foreign());
                    }
                }
                (_, RecoveryPreparationFailure::Candidate(_)) => {
                    assert_eq!(delivered.get(), 1);
                    assert_eq!(candidate_failures.replace(1), 0);
                    let deadline = owner
                        .borrow()
                        .interrupted_exit_reopen_deadline(request)
                        .unwrap()
                        .unwrap();
                    owner
                        .borrow_mut()
                        .test_expire_interrupted_exit_reopen_deadline(deadline);
                }
                _ => panic!("unexpected prior resume outcome"),
            }
        },
        cx,
    )
    .await;
    match fault {
        FaultPoint::BeforeCommit => {
            assert!(result.unwrap_err().contains("cancelled"));
            assert_eq!(delivered.get(), 2);
            assert_eq!(candidate_failures.get(), 1);
        }
        FaultPoint::AfterCommitBeforePersist => {
            assert!(result.unwrap_err().contains("request changed"));
            assert_eq!(delivered.get(), 2);
            assert_eq!(candidate_failures.get(), 1);
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
        }
        FaultPoint::AfterPersist => {
            result.unwrap();
            assert_eq!(delivered.get(), 0);
        }
        _ => unreachable!(),
    }
    if fault != FaultPoint::BeforeCommit {
        owner
            .borrow()
            .interrupted_exit_services_result(request)
            .unwrap();
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            RunningProcessOwner::cancel_interrupted_exit_services(
                owner,
                request,
                app,
                move |_, _| {
                    sender.send(()).unwrap();
                },
            )
        })
        .unwrap()
        .unwrap();
        receiver.await.unwrap();
    }
    owner
        .borrow_mut()
        .take_interrupted_exit_preparation_failure(request, retired)
        .unwrap();
    let deadline = owner
        .borrow()
        .interrupted_exit_reopen_deadline(request)
        .unwrap()
        .unwrap();
    owner
        .borrow_mut()
        .test_expire_interrupted_exit_reopen_deadline(deadline);
    RunningProcessOwner::construct_and_settle_interrupted_exit(
        owner,
        request,
        retired,
        CommandCancellation::new(),
        cx,
    )
    .await
    .unwrap();
    {
        let retained = owner.borrow();
        retained.interrupted_exit_candidate_result(request).unwrap();
        assert!(retained.interrupted_exit_services_result(request).is_err());
        assert!(retained.test_services().graph().is_none());
        let session = retained.interrupted_exit_session().unwrap();
        let RunningShutdownSession::Resuming(resume) = &*session else {
            panic!("resume outcome lost")
        };
        assert_eq!(resume.result_revision(), Some(revision));
        match fault {
            FaultPoint::BeforeCommit => assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::Committed {
                    later_failure: None,
                    ..
                })
            )),
            FaultPoint::AfterCommitBeforePersist => assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::Indeterminate {
                    reconciliation: Some(Ok(
                        beryl_home_store::ReconciliationResolution::ExactNew { .. }
                    )),
                    ..
                })
            )),
            FaultPoint::AfterPersist => assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::Committed {
                    later_failure: Some(_),
                    ..
                })
            )),
            _ => unreachable!(),
        }
    }
    let mut retained = owner.borrow_mut();
    assert!(
        retained
            .take_previous_interrupted_exit_resume(&request.test_foreign())
            .is_err()
    );
    assert!(
        retained
            .take_previous_interrupted_exit_resume_reconciliation(&request.test_foreign())
            .is_err()
    );
    assert!(
        retained
            .take_previous_interrupted_exit_resume(request)
            .is_err()
    );
    assert!(
        retained
            .take_previous_interrupted_exit_resume_reconciliation(request)
            .is_err()
    );
}
