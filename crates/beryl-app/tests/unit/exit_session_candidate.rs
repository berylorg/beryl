#[test]
fn interrupted_exit_convergence_preserves_outcomes_and_never_repeats_resume() {
    use crate::running_owner::RunningShutdownSession;
    for exit_fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterPersist,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        for resume_fault in [
            None,
            Some(FaultPoint::BeforeCommit),
            Some(FaultPoint::AfterPersist),
            Some(FaultPoint::AfterCommitBeforePersist),
        ] {
            let faults = FaultController::new();
            let (_directory, home, old) = open_with_faults(3, faults.clone());
            faults.fail_next(exit_fault);
            let outcome = execute_exit_session(&home, &old, placements(3)).unwrap();
            let evidence = outcome.publication().clone();
            if home.home_revision().is_ok() {
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
                assert!(home.home_revision().is_err());
            }
            let (mut candidate, fresh) = recover_session(home);
            let mut retained = RunningShutdownSession::Settled(Ok(outcome));
            let before = fresh
                .minimal_bootstrap_candidate(&candidate.recovery_access().unwrap())
                .unwrap()
                .unwrap();
            if let Some(fault) = resume_fault {
                faults.fail_next(fault);
            }
            let result = retained.converge_candidate(&mut candidate, &fresh);
            let noncommit = matches!(exit_fault, FaultPoint::BeforeCommit);
            let succeeds = noncommit
                || resume_fault.is_none()
                || matches!(resume_fault, Some(FaultPoint::AfterCommitBeforePersist));
            assert_eq!(
                result.is_ok(),
                succeeds,
                "{exit_fault:?}, {resume_fault:?}: {result:?}"
            );
            if noncommit {
                assert!(matches!(
                    &retained,
                    RunningShutdownSession::Settled(Ok(ExitSessionExecution::NotCommitted { .. }))
                ));
            } else {
                let RunningShutdownSession::Resuming(resume) = &retained else {
                    panic!()
                };
                assert_eq!(resume.exit().publication(), &evidence);
                assert!(resume.outcome().is_some());
                match resume_fault {
                    Some(FaultPoint::BeforeCommit) => assert!(matches!(
                        resume.outcome(),
                        Some(ResumeSessionOutcome::NotCommitted { .. })
                    )),
                    Some(FaultPoint::AfterPersist) => assert!(matches!(
                        resume.outcome(),
                        Some(ResumeSessionOutcome::Committed {
                            later_failure: Some(_),
                            ..
                        })
                    )),
                    Some(FaultPoint::AfterCommitBeforePersist) => assert!(matches!(
                        resume.outcome(),
                        Some(ResumeSessionOutcome::Indeterminate {
                            reconciliation: Some(Ok(ReconciliationResolution::ExactNew { .. })),
                            ..
                        })
                    )),
                    _ => {}
                }
            }
            let retained_before = format!("{retained:?}");
            assert_eq!(
                retained.converge_candidate(&mut candidate, &fresh).is_ok(),
                succeeds
            );
            assert_eq!(format!("{retained:?}"), retained_before);
            if succeeds {
                let after = fresh
                    .minimal_bootstrap_candidate(&candidate.recovery_access().unwrap())
                    .unwrap()
                    .unwrap();
                assert_eq!(after.header().exit_intent(), SessionExitIntent::Running);
                assert_eq!(after.windows(), before.windows());
                assert_eq!(after.header().fallback(), before.header().fallback());
                if noncommit {
                    assert_eq!(after, before);
                }
            }
            candidate.abort().close().unwrap();
        }
    }
}

#[test]
fn interrupted_exit_candidate_settlement_retains_original_and_revalidates() {
    use crate::running_owner::RunningShutdownSession;
    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterPersist,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let faults = FaultController::new();
        let (_directory, home, old) = open_with_faults(3, faults.clone());
        faults.fail_next(fault);
        let outcome = execute_exit_session(&home, &old, placements(3)).unwrap();
        let evidence = outcome.publication().clone();
        if home.home_revision().is_ok() {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(home.home_revision().is_err());
        }
        let (mut candidate, fresh) = recover_session(home);
        let mut retained = RunningShutdownSession::Settled(Ok(outcome));
        assert!(matches!(
            retained.settle_candidate(&mut candidate, &old),
            Err(ExitSessionValidationError::Read(_))
        ));
        let revision = candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap();
        let expected = if matches!(fault, FaultPoint::BeforeCommit) {
            ExitSessionValidation::UnchangedRunning
        } else {
            ExitSessionValidation::CommittedExit
        };
        assert_eq!(
            retained.settle_candidate(&mut candidate, &fresh).unwrap(),
            expected
        );
        let settled = format!("{retained:?}");
        assert_eq!(
            retained.settle_candidate(&mut candidate, &fresh).unwrap(),
            expected
        );
        assert_eq!(format!("{retained:?}"), settled);
        assert_eq!(
            candidate
                .recovery_access()
                .unwrap()
                .home_revision()
                .unwrap(),
            revision
        );
        let RunningShutdownSession::Settled(Ok(ref outcome)) = retained else {
            panic!()
        };
        assert_eq!(outcome.publication(), &evidence);
        if let ExitSessionExecution::Indeterminate(pending) = outcome {
            assert!(matches!(
                pending.candidate_resolution(),
                Some(Ok(ReconciliationResolution::ExactNew { .. }))
            ));
        }
        let home = candidate.publish().unwrap();
        let snapshot = fresh.minimal_bootstrap(&home).unwrap().unwrap();
        let change = if expected == ExitSessionValidation::UnchangedRunning {
            fresh.update_placement(
                fresh.revision(&home).unwrap(),
                UpdateWindowPlacement::new(
                    snapshot.header().revision(),
                    snapshot.windows()[0].window_id(),
                    snapshot.windows()[0].revision(),
                    placement(555),
                ),
            )
        } else {
            fresh.resume_after_exit(
                fresh.revision(&home).unwrap(),
                beryl_state::ResumeSessionAfterExit::new(
                    snapshot.header().revision(),
                    snapshot
                        .windows()
                        .iter()
                        .map(|w| (w.window_id(), w.revision()))
                        .collect(),
                )
                .unwrap(),
            )
        };
        execute(&home, change);
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(home.home_revision().is_err());
        let (mut candidate, fresh) = recover_session(home);
        assert!(matches!(
            retained.settle_candidate(&mut candidate, &fresh),
            Err(ExitSessionValidationError::Changed)
        ));
        assert_eq!(format!("{retained:?}"), settled);
        candidate.abort().close().unwrap();
    }
}

#[test]
fn interrupted_exit_candidate_refuses_foreign_and_retains_failed_reconciliation() {
    let faults = FaultController::new();
    let (_directory, home, old) = open_with_faults(1, faults.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let ExitSessionExecution::Indeterminate(mut pending) =
        execute_exit_session(&home, &old, placements(1)).unwrap()
    else {
        panic!()
    };
    let other_faults = FaultController::new();
    let (_other_directory, other, _) = open_with_faults(1, other_faults.clone());
    other_faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(other.home_revision().is_err());
    let (mut other, other_session) = recover_session(other);
    assert!(matches!(
        pending.settle_candidate(&mut other, &other_session),
        Err(ExitSessionValidationError::ForeignHome)
    ));
    assert!(pending.candidate_resolution().is_none());
    other.abort().close().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let (mut candidate, fresh) = recover_session(home);
    faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    assert!(matches!(
        pending.settle_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Unproven)
    ));
    assert!(matches!(pending.candidate_resolution(), Some(Err(_))));
    let before = format!("{pending:?}");
    assert!(matches!(
        pending.settle_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Unproven)
    ));
    assert_eq!(format!("{pending:?}"), before);
    let access = candidate.recovery_access().unwrap();
    for handle in access.pending_reconciliations() {
        access.retry_reconciliation(&handle).unwrap();
    }
    candidate.abort().close().unwrap();
}
