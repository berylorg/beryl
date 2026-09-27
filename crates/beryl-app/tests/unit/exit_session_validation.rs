fn recover_session(home: HomeStore) -> (beryl_home_store::HomeRecoveryCandidate, SessionState) {
    let candidate = home.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    (candidate, state.session())
}

#[test]
fn exit_candidate_validates_known_outcomes_without_writes_or_old_receipts() {
    for count in [1, 3, MAX_RESTORABLE_WINDOWS] {
        for committed in [false, true] {
            let faults = FaultController::new();
            let (_directory, home, old_session) = open_with_faults(count, faults.clone());
            faults.fail_next(if committed {
                FaultPoint::AfterPersist
            } else {
                FaultPoint::BeforeCommit
            });
            let outcome = execute_exit_session(&home, &old_session, placements(count)).unwrap();
            let (mut candidate, fresh) = recover_session(home);
            assert!(matches!(
                outcome.validate_candidate(&mut candidate, &old_session),
                Err(ExitSessionValidationError::Read(_))
            ));
            let before = candidate
                .recovery_access()
                .unwrap()
                .home_revision()
                .unwrap();
            let expected = if committed {
                ExitSessionValidation::CommittedExit
            } else {
                ExitSessionValidation::UnchangedRunning
            };
            assert_eq!(
                outcome.validate_candidate(&mut candidate, &fresh).unwrap(),
                expected
            );
            assert_eq!(
                candidate
                    .recovery_access()
                    .unwrap()
                    .home_revision()
                    .unwrap(),
                before
            );
            assert!(
                fresh
                    .minimal_bootstrap(&candidate.service_reference())
                    .is_err()
            );
            candidate.abort().close().unwrap();
        }
    }
}

#[test]
fn exit_candidate_refuses_foreign_missing_and_changed_sources() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(1, faults.clone());
    faults.fail_next(FaultPoint::BeforeCommit);
    let outcome = execute_exit_session(&home, &session, placements(1)).unwrap();
    let (mut candidate, fresh) = recover_session(home);
    let foreign_faults = FaultController::new();
    let (_foreign_directory, foreign, _) = open_with_faults(1, foreign_faults.clone());
    foreign_faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(foreign.home_revision().is_err());
    let (mut foreign_candidate, foreign_session) = recover_session(foreign);
    assert!(matches!(
        outcome.validate_candidate(&mut foreign_candidate, &foreign_session),
        Err(ExitSessionValidationError::ForeignHome)
    ));
    foreign_candidate.abort().close().unwrap();
    assert!(matches!(
        outcome.validate_candidate(&mut candidate, &foreign_session),
        Err(ExitSessionValidationError::Read(_))
    ));
    let home = candidate.publish().unwrap();
    let snapshot = fresh.minimal_bootstrap(&home).unwrap().unwrap();
    execute(
        &home,
        fresh.update_placement(
            fresh.revision(&home).unwrap(),
            UpdateWindowPlacement::new(
                snapshot.header().revision(),
                snapshot.windows()[0].window_id(),
                snapshot.windows()[0].revision(),
                placement(888),
            ),
        ),
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let (mut candidate, fresh) = recover_session(home);
    assert!(matches!(
        outcome.validate_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Changed)
    ));
    candidate.abort().close().unwrap();

    let faults = FaultController::new();
    let (_directory, home, _) = open_with_faults(0, faults.clone());
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let (mut candidate, fresh) = recover_session(home);
    let ExitSessionExecution::NotCommitted {
        evidence,
        mut publication,
    } = outcome
    else {
        panic!()
    };
    publication.configured_home = candidate.service_reference().configured_path().to_owned();
    let missing = ExitSessionExecution::NotCommitted {
        evidence,
        publication,
    };
    assert!(matches!(
        missing.validate_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Changed)
    ));
    candidate.abort().close().unwrap();
}

#[test]
fn exit_candidate_checks_every_predicted_record_and_read_confirmation() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(3, faults.clone());
    faults.fail_next(FaultPoint::AfterPersist);
    let mut outcome = execute_exit_session(&home, &session, placements(3)).unwrap();
    let original = outcome.publication().clone();
    let (mut candidate, fresh) = recover_session(home);
    for change in 0..5 {
        let ExitSessionExecution::Committed { publication, .. } = &mut outcome else {
            panic!()
        };
        **publication = original.clone();
        match change {
            0 => {
                publication.result_session_revision =
                    publication.result_session_revision.checked_next().unwrap()
            }
            1 => {
                publication.result_windows.pop();
            }
            2 => publication.result_windows[2].0 = WindowId::from_bytes([99; 16]),
            3 => publication.result_windows[2].1 = beryl_state::RecordRevision::new(999).unwrap(),
            _ => publication.result_windows[2].2 = placement(999),
        }
        assert!(
            matches!(
                outcome.validate_candidate(&mut candidate, &fresh),
                Err(ExitSessionValidationError::Changed)
            ),
            "change {change}"
        );
    }
    let ExitSessionExecution::Committed { publication, .. } = &mut outcome else {
        panic!()
    };
    **publication = original;
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        outcome.validate_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Read(_))
    ));
    candidate.abort().close().unwrap();
}

#[test]
fn exit_candidate_refuses_pending_and_terminal_outcomes_even_when_state_matches() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(1, faults.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = execute_exit_session(&home, &session, placements(1)).unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let (mut candidate, fresh) = recover_session(home);
    assert!(matches!(
        outcome.validate_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Unproven)
    ));
    let ExitSessionExecution::Indeterminate(pending) = outcome else {
        panic!()
    };
    let (_foreign_directory, foreign, _) = open(1);
    let pending = pending.reconcile(&foreign);
    assert!(matches!(
        pending.validate_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Unproven)
    ));
    foreign.close().unwrap();
    {
        let access = candidate.recovery_access().unwrap();
        for handle in access.pending_reconciliations() {
            assert!(matches!(
                access.reconcile(&handle).unwrap(),
                ReconciliationResolution::ExactNew { .. }
            ));
        }
    }
    candidate.abort().close().unwrap();

    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(1, faults.clone());
    faults.fail_next(FaultPoint::BeforeCommit);
    let outcome = execute_exit_session(&home, &session, placements(1)).unwrap();
    let (mut candidate, fresh) = recover_session(home);
    let ExitSessionExecution::NotCommitted {
        evidence,
        publication,
    } = outcome
    else {
        panic!()
    };
    let exact = ExitSessionReconciled::ExactOld {
        original_failure: evidence,
        publication,
    };
    assert_eq!(
        exact.validate_candidate(&mut candidate, &fresh).unwrap(),
        ExitSessionValidation::UnchangedRunning
    );
    let ExitSessionReconciled::ExactOld {
        original_failure,
        publication,
    } = exact
    else {
        panic!()
    };
    let blocked = ExitSessionReconciled::Blocked {
        original_failure,
        publication,
        resolution: ReconciliationResolution::Collision,
    };
    assert!(matches!(
        blocked.validate_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Unproven)
    ));
    candidate.abort().close().unwrap();
}

#[test]
fn exit_candidate_accepts_exact_new_but_refuses_successor_and_already_resumed_state() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(3, faults.clone());
    faults.fail_next(FaultPoint::AfterPersist);
    let outcome = execute_exit_session(&home, &session, placements(3)).unwrap();
    let (mut candidate, fresh) = recover_session(home);
    let ExitSessionExecution::Committed {
        receipt,
        later_failure: Some(original_failure),
        publication,
        ..
    } = outcome
    else {
        panic!()
    };
    let exact = ExitSessionReconciled::ExactNew {
        receipt,
        original_failure,
        publication,
    };
    assert_eq!(
        exact.validate_candidate(&mut candidate, &fresh).unwrap(),
        ExitSessionValidation::CommittedExit
    );
    let ExitSessionReconciled::ExactNew {
        receipt,
        original_failure,
        publication,
    } = exact
    else {
        panic!()
    };
    let blocked = ExitSessionReconciled::Blocked {
        original_failure,
        publication,
        resolution: ReconciliationResolution::ExactSuccessor {
            receipt: receipt.clone(),
        },
    };
    assert!(matches!(
        blocked.validate_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Unproven)
    ));
    let ExitSessionReconciled::Blocked {
        original_failure,
        publication,
        ..
    } = blocked
    else {
        panic!()
    };
    let exact = ExitSessionReconciled::ExactNew {
        receipt,
        original_failure,
        publication,
    };
    let home = candidate.publish().unwrap();
    let snapshot = fresh.minimal_bootstrap(&home).unwrap().unwrap();
    let resume = beryl_state::ResumeSessionAfterExit::new(
        snapshot.header().revision(),
        snapshot
            .windows()
            .iter()
            .map(|w| (w.window_id(), w.revision()))
            .collect(),
    )
    .unwrap();
    execute(
        &home,
        fresh.resume_after_exit(fresh.revision(&home).unwrap(), resume),
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let (mut candidate, fresh) = recover_session(home);
    assert!(matches!(
        exact.validate_candidate(&mut candidate, &fresh),
        Err(ExitSessionValidationError::Changed)
    ));
    candidate.abort().close().unwrap();
}
