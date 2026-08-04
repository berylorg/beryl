#[test]
fn phase83_atomic_seal_transfer_fences_concurrent_connection_retirement() {
    let mut fixture = Phase83Fixture::new(211, 1, true);
    let candidate = fixture.candidate_ids()[0];
    assert_eq!(
        fixture
            .ledger_mut()
            .reauthenticate_candidate(candidate)
            .unwrap()
            .status(),
        ProjectionCandidateReauthenticationStatus::Accepted
    );
    let pause = fixture
        .ledger()
        .pause_candidate_set_seal_before_transfer_for_test();
    let retirement_attempt = fixture
        .connection
        .observe_next_retirement_gate_attempt_for_test();
    let connection = Arc::clone(&fixture.connection);
    let ledger = fixture.take_ledger();

    let converged = std::thread::scope(|scope| {
        let sealing = scope.spawn(move || ledger.seal().unwrap());
        pause.wait_until_paused(Duration::from_secs(5));
        let retirement =
            scope.spawn(move || connection.retire_authority_for_recovery_test().unwrap());
        retirement_attempt.wait(Duration::from_secs(5));
        pause.release();
        let converged = sealing.join().unwrap();
        assert!(matches!(
            retirement.join().unwrap(),
            crate::cas_projection::connection::ConnectionRetirementOutcome::FailureRetained(_)
        ));
        converged
    });

    assert_eq!(converged.accepted_candidate_count(), 1);
    assert_eq!(converged.retained_connection_owner_count_for_test(), 1);
    assert_eq!(fixture.registry_now(), fixture.registry_before);
    drop(converged);
    assert!(fixture.registry_now().is_empty());
    fixture.close();
}

#[test]
fn phase83_retirement_before_reauthentication_terminals_and_disposes_the_whole_attempt() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(212, 2, true);
    let candidate = fixture.candidate_ids()[0];
    assert_eq!(fixture.registry_before.len(), 2);
    assert!(matches!(
        fixture
            .connection
            .retire_authority_for_recovery_test()
            .unwrap(),
        crate::cas_projection::connection::ConnectionRetirementOutcome::FailureRetained(_)
    ));

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired
        ))
    );
    phase83_assert_terminal_ledger(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired,
    );
    let terminal = phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired,
    );
    assert_eq!(
        terminal.replacement_worker_diagnostics_for_test().active(),
        fixture.replacement_workers
    );
    terminal.dispose().unwrap();
    wait_until("the terminal replacement service to shut down", || {
        fixture.replacement_shutdowns.load(Ordering::SeqCst) == 1
    });
    assert!(fixture.registry_now().is_empty());
    assert_eq!(
        fixture
            .connection
            .retire_authority_for_recovery_test()
            .unwrap(),
        crate::cas_projection::connection::ConnectionRetirementOutcome::Complete
    );
    fixture.close();
}

#[test]
fn phase83_terminal_disposition_reports_ingester_join_failure_after_complete_cleanup() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(215, 2, true);
    fixture
        .connection
        .fail_current_epoch_ingester_join_for_test()
        .unwrap();
    let candidate = fixture.candidate_ids()[0];
    assert!(matches!(
        fixture
            .connection
            .retire_authority_for_recovery_test()
            .unwrap(),
        crate::cas_projection::connection::ConnectionRetirementOutcome::FailureRetained(_)
    ));
    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired
        ))
    );
    let terminal = phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired,
    );

    assert!(matches!(
        terminal.dispose(),
        Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
    ));
    wait_until(
        "the failed terminal disposition to close the replacement service",
        || fixture.replacement_shutdowns.load(Ordering::SeqCst) == 1,
    );
    assert!(fixture.registry_now().is_empty());
    assert_eq!(
        fixture
            .connection
            .retire_authority_for_recovery_test()
            .unwrap(),
        crate::cas_projection::connection::ConnectionRetirementOutcome::Complete
    );
    fixture.close();
}

#[test]
fn phase83_retirement_after_acceptance_demotes_every_entry_terminal() {
    let mut fixture = Phase83Fixture::new(213, 2, true);
    for candidate in fixture.candidate_ids() {
        assert_eq!(
            fixture
                .ledger_mut()
                .reauthenticate_candidate(candidate)
                .unwrap()
                .status(),
            ProjectionCandidateReauthenticationStatus::Accepted
        );
    }
    assert!(matches!(
        fixture
            .connection
            .retire_authority_for_recovery_test()
            .unwrap(),
        crate::cas_projection::connection::ConnectionRetirementOutcome::FailureRetained(_)
    ));

    let terminal = phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired,
    );
    let metadata = terminal.metadata();
    assert_eq!(metadata.unprocessed_count(), 0);
    assert_eq!(metadata.rejected_count(), 2);
    assert_eq!(metadata.accepted_count(), 0);
    assert_eq!(metadata.disposed_count(), 0);
    drop(terminal);
    fixture.close();
}

#[test]
fn phase83_zero_candidate_retirement_is_terminal_not_retryable() {
    let mut fixture = Phase83Fixture::new(214, 0, true);
    assert!(matches!(
        fixture
            .connection
            .retire_authority_for_recovery_test()
            .unwrap(),
        crate::cas_projection::connection::ConnectionRetirementOutcome::FailureRetained(_)
    ));

    let terminal = phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired,
    );
    let metadata = terminal.metadata();
    assert_eq!(metadata.connection_owner_count(), 1);
    assert_eq!(metadata.unprocessed_count(), 0);
    assert_eq!(metadata.rejected_count(), 0);
    assert_eq!(metadata.accepted_count(), 0);
    assert_eq!(metadata.disposed_count(), 0);
    drop(terminal);
    fixture.close();
}
