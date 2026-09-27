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
        assert!(matches!(
            resume.validate_candidate(&mut candidate, &fresh),
            Err(ExitSessionValidationError::Unproven)
        ));
        assert!(resume.execute(&mut candidate, &session).is_err());
        assert!(resume.outcome().is_none());
        resume.execute(&mut candidate, &fresh).unwrap();
        assert!(matches!(
            resume.validate_candidate(&mut candidate, &session),
            Err(ExitSessionValidationError::Read(_))
        ));
        assert_eq!(
            resume.validate_candidate(&mut candidate, &fresh).unwrap(),
            ResumeSessionValidation::ResumedRunning
        );
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
            assert!(matches!(
                resume.validate_candidate(&mut candidate, &fresh),
                Err(ExitSessionValidationError::Unproven)
            ));
            let foreign_faults = FaultController::new();
            let (_foreign_directory, foreign, _) = open_with_faults(1, foreign_faults.clone());
            foreign_faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(foreign.home_revision().is_err());
            let (mut foreign, _) = recover_session(foreign);
            resume.reconcile(&mut foreign).unwrap();
            assert!(matches!(
                resume.validate_candidate(&mut candidate, &fresh),
                Err(ExitSessionValidationError::Unproven)
            ));
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
            resume.validate_candidate(&mut candidate, &fresh).unwrap(),
            if matches!(fault, FaultPoint::BeforeCommit) {
                ResumeSessionValidation::UnchangedExit
            } else {
                ResumeSessionValidation::ResumedRunning
            }
        );
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

#[test]
fn resumed_session_validation_refuses_drift_foreign_home_and_failed_reads() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(3, faults.clone());
    faults.fail_next(FaultPoint::AfterPersist);
    let exit = execute_exit_session(&home, &session, placements(3)).unwrap();
    let original = exit.publication().clone();
    let (mut candidate, fresh) = recover_session(home);
    let mut resume = InterruptedExitResume::new(InterruptedExit::Executed(exit));
    resume.execute(&mut candidate, &fresh).unwrap();
    let revision = resume.result_revision().unwrap();
    let before = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    for change in 0..6 {
        let mut evidence = original.clone();
        let mut expected_revision = revision;
        let mut intent = SessionExitIntent::Running;
        match change {
            0 => expected_revision = revision.checked_next().unwrap(),
            1 => {
                evidence.result_windows.pop();
            }
            2 => evidence.result_windows[2].0 = WindowId::from_bytes([99; 16]),
            3 => evidence.result_windows[2].1 = beryl_state::RecordRevision::new(999).unwrap(),
            4 => evidence.result_windows[2].2 = placement(999),
            _ => intent = SessionExitIntent::OrderlyExit,
        }
        assert!(
            matches!(
                super::validation::validate_published(
                    &evidence,
                    intent,
                    expected_revision,
                    &mut candidate,
                    &fresh
                ),
                Err(ExitSessionValidationError::Changed)
            ),
            "change {change}"
        );
    }
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        before
    );
    let foreign_faults = FaultController::new();
    let (_foreign_directory, foreign, _) = open_with_faults(0, foreign_faults.clone());
    foreign_faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(foreign.home_revision().is_err());
    let (mut foreign, foreign_session) = recover_session(foreign);
    assert!(matches!(
        resume.validate_candidate(&mut foreign, &foreign_session),
        Err(ExitSessionValidationError::ForeignHome)
    ));
    let mut missing = original.clone();
    missing.configured_home = foreign.service_reference().configured_path().to_owned();
    assert!(matches!(
        super::validation::validate_published(
            &missing,
            SessionExitIntent::Running,
            revision,
            &mut foreign,
            &foreign_session
        ),
        Err(ExitSessionValidationError::Changed)
    ));
    foreign.abort().close().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        resume.validate_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Read(_))
    ));
    assert!(matches!(
        resume.outcome(),
        Some(ResumeSessionOutcome::Committed { .. })
    ));
    assert_eq!(resume.exit().publication(), &original);
    candidate.abort().close().unwrap();
}

#[test]
fn resume_outcome_classification_requires_exact_reconciliation() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(1, faults.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let exit = execute_exit_session(&home, &session, placements(1)).unwrap();
    let ExitSessionExecution::Indeterminate(pending) = exit else {
        panic!()
    };
    let handle = home.pending_reconciliations().pop().unwrap();
    let ExitSessionReconciled::ExactNew {
        receipt,
        original_failure,
        ..
    } = pending.reconcile(&home)
    else {
        panic!()
    };
    let mut outcome = ResumeSessionOutcome::Indeterminate {
        original_failure,
        handle,
        reconciliation: None,
    };
    assert_eq!(outcome.known_commit(), None);
    for (resolution, expected) in [
        (ReconciliationResolution::ExactOld, Some(false)),
        (
            ReconciliationResolution::ExactNew {
                receipt: receipt.clone(),
            },
            Some(true),
        ),
        (ReconciliationResolution::ExactSuccessor { receipt }, None),
        (ReconciliationResolution::Collision, None),
    ] {
        let ResumeSessionOutcome::Indeterminate { reconciliation, .. } = &mut outcome else {
            panic!()
        };
        *reconciliation = Some(Ok(resolution));
        assert_eq!(outcome.known_commit(), expected);
    }
    home.close().unwrap();
}
