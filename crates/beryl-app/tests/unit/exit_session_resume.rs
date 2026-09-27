#[test]
fn interrupted_exit_resume_preserves_records_and_refuses_duplicate_execution() {
    for count in [1, 3, MAX_RESTORABLE_WINDOWS] {
        let faults = FaultController::new();
        let (_directory, home, session) = open_with_faults(count, faults.clone());
        faults.fail_next(FaultPoint::AfterPersist);
        let exit = execute_exit_session(&home, &session, placements(count)).unwrap();
        let original = exit.publication().clone();
        let (mut candidate, fresh) = recover_session(home);
        let before = fresh
            .minimal_bootstrap_candidate(&candidate.recovery_access().unwrap())
            .unwrap()
            .unwrap();
        let mut resume = InterruptedExitResume::new(InterruptedExit::Executed(exit));
        assert!(resume.execute(&mut candidate, &session).is_err());
        assert!(resume.outcome().is_none());
        resume.execute(&mut candidate, &fresh).unwrap();
        assert!(matches!(
            resume.outcome(),
            Some(ResumeSessionOutcome::Committed {
                later_failure: None,
                local_finalization: None,
                ..
            })
        ));
        let after = fresh
            .minimal_bootstrap_candidate(&candidate.recovery_access().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(after.header().exit_intent(), SessionExitIntent::Running);
        assert_eq!(Some(after.header().revision()), resume.result_revision());
        assert_eq!(before.windows(), after.windows());
        assert_eq!(before.header().fallback(), after.header().fallback());
        assert_eq!(resume.exit().publication(), &original);
        let revision = candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap();
        assert!(resume.execute(&mut candidate, &fresh).is_err());
        assert!(resume.reconcile(&mut candidate).is_err());
        assert_eq!(
            candidate
                .recovery_access()
                .unwrap()
                .home_revision()
                .unwrap(),
            revision
        );
        candidate.abort().close().unwrap();
    }
}

#[test]
fn interrupted_exit_resume_retains_each_failure_and_exact_reconciliation() {
    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterPersist,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let faults = FaultController::new();
        let (_directory, home, session) = open_with_faults(3, faults.clone());
        faults.fail_next(FaultPoint::AfterPersist);
        let exit = execute_exit_session(&home, &session, placements(3)).unwrap();
        let (mut candidate, fresh) = recover_session(home);
        let mut resume = InterruptedExitResume::new(InterruptedExit::Executed(exit));
        faults.fail_next(fault);
        resume.execute(&mut candidate, &fresh).unwrap();
        assert!(resume.execute(&mut candidate, &fresh).is_err());
        match fault {
            FaultPoint::BeforeCommit => assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::NotCommitted { .. })
            )),
            FaultPoint::AfterPersist => assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::Committed {
                    later_failure: Some(_),
                    local_finalization: Some(_),
                    ..
                })
            )),
            FaultPoint::AfterCommitBeforePersist => {
                assert!(matches!(
                    resume.outcome(),
                    Some(ResumeSessionOutcome::Indeterminate {
                        reconciliation: None,
                        ..
                    })
                ));
                assert_eq!(
                    candidate
                        .recovery_access()
                        .unwrap()
                        .pending_reconciliations()
                        .len(),
                    1
                );
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
                assert!(
                    candidate
                        .recovery_access()
                        .unwrap()
                        .home_revision()
                        .is_err()
                );
            }
            _ => unreachable!(),
        }
        let (mut candidate, fresh) = recover_session(candidate.abort());
        if matches!(fault, FaultPoint::AfterCommitBeforePersist) {
            let foreign_faults = FaultController::new();
            let (_foreign_directory, foreign, _) = open_with_faults(1, foreign_faults.clone());
            foreign_faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(foreign.home_revision().is_err());
            let (mut foreign, _) = recover_session(foreign);
            resume.reconcile(&mut foreign).unwrap();
            assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::Indeterminate {
                    reconciliation: Some(Err(_)),
                    ..
                })
            ));
            foreign.abort().close().unwrap();
            resume.reconcile(&mut candidate).unwrap();
            assert!(matches!(
                resume.outcome(),
                Some(ResumeSessionOutcome::Indeterminate {
                    reconciliation: Some(Ok(ReconciliationResolution::ExactNew { .. })),
                    ..
                })
            ));
            assert!(resume.reconcile(&mut candidate).is_err());
        }
        let snapshot = fresh
            .minimal_bootstrap_candidate(&candidate.recovery_access().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            snapshot.header().exit_intent(),
            if matches!(fault, FaultPoint::BeforeCommit) {
                SessionExitIntent::OrderlyExit
            } else {
                SessionExitIntent::Running
            }
        );
        assert!(resume.execute(&mut candidate, &fresh).is_err());
        candidate.abort().close().unwrap();
    }
}

#[test]
fn interrupted_exit_resume_refuses_noncommit_and_foreign_candidate() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(1, faults.clone());
    faults.fail_next(FaultPoint::BeforeCommit);
    let exit = execute_exit_session(&home, &session, placements(1)).unwrap();
    let (mut candidate, fresh) = recover_session(home);
    let mut resume = InterruptedExitResume::new(InterruptedExit::Executed(exit));
    let before = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(resume.execute(&mut candidate, &fresh).is_err());
    assert!(resume.result_revision().is_none());
    assert!(resume.outcome().is_none());
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        before
    );
    candidate.abort().close().unwrap();

    let (_directory, home, session) = open_with_faults(1, faults.clone());
    faults.fail_next(FaultPoint::AfterPersist);
    let exit = execute_exit_session(&home, &session, placements(1)).unwrap();
    let (candidate, _) = recover_session(home);
    let foreign_faults = FaultController::new();
    let (_foreign_directory, foreign, _) = open_with_faults(1, foreign_faults.clone());
    foreign_faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(foreign.home_revision().is_err());
    let (mut foreign, session) = recover_session(foreign);
    let mut resume = InterruptedExitResume::new(InterruptedExit::Executed(exit));
    assert!(resume.execute(&mut foreign, &session).is_err());
    assert!(resume.outcome().is_none());
    foreign.abort().close().unwrap();
    candidate.abort().close().unwrap();
}
