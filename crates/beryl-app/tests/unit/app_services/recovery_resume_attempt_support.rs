pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    retired: beryl_home_store::HomeGeneration,
    fault: beryl_home_store::test_faults::FaultPoint,
    revision: beryl_model::SessionRevision,
    cx: &mut AsyncApp,
) {
    use crate::exit_session::ResumeSessionOutcome;
    use beryl_home_store::test_faults::FaultPoint;

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
    match fault {
        FaultPoint::BeforeCommit => assert!(matches!(
            retained
                .take_previous_interrupted_exit_resume(request)
                .unwrap(),
            ResumeSessionOutcome::NotCommitted { .. }
        )),
        FaultPoint::AfterCommitBeforePersist => {
            retained
                .take_previous_interrupted_exit_resume_reconciliation(request)
                .unwrap();
        }
        FaultPoint::AfterPersist => {}
        _ => unreachable!(),
    }
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
