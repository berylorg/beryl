fn phase83_revoke_at_pause(
    fixture: &mut Phase83Fixture,
    candidate: ProjectionCandidateId,
    after_stable_read: bool,
) -> ProjectionCandidateReauthenticationOutcome {
    let pause = if after_stable_read {
        fixture
            .ledger()
            .pause_candidate_after_stable_read_for_test(candidate)
    } else {
        fixture
            .ledger()
            .pause_candidate_after_pre_authentication_for_test(candidate)
    };
    let cas_thread_id = fixture.registry_before[0].key().cas_thread_id.clone();
    let connection = Arc::clone(&fixture.connection);
    let ledger = fixture.ledger_mut();
    std::thread::scope(|scope| {
        let worker = scope.spawn(move || ledger.reauthenticate_candidate(candidate));
        pause.wait_until_paused(Duration::from_secs(5));
        let closed = connection.record_thread_closed(&cas_thread_id).unwrap();
        assert!(!closed.connection_retired());
        assert!(closed.registry_authority_revoked());
        pause.release();
        worker.join().unwrap().unwrap()
    })
}

fn phase83_assert_post_auth_revocation(mut fixture: Phase83Fixture, after_stable_read: bool) {
    let candidate = fixture.candidate_ids()[0];
    let outcome = phase83_revoke_at_pause(&mut fixture, candidate, after_stable_read);
    assert_eq!(
        outcome.status(),
        ProjectionCandidateReauthenticationStatus::Rejected
    );
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::PostReadRegistryTokenMismatch)
    );
    phase83_assert_counts(fixture.ledger(), 0, 1, 0, 0);
    assert!(fixture.registry_now().is_empty());

    fixture.ledger_mut().dispose_candidate(candidate).unwrap();
    drop(fixture.take_ledger().seal().unwrap());
    fixture.close();
}

#[test]
fn phase83_revocation_before_pre_read_authentication_is_exactly_retryable() {
    let fixture = Phase83Fixture::new(242, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let cas_thread_id = fixture.registry_before[0].key().cas_thread_id.clone();
    let closed = fixture
        .connection
        .record_thread_closed(&cas_thread_id)
        .unwrap();
    assert!(!closed.connection_retired());
    assert!(closed.registry_authority_revoked());
    assert!(fixture.registry_now().is_empty());

    phase83_assert_retryable_fact_rejection_with_registry(
        fixture,
        ProjectionCandidateReauthenticationReason::PreReadRegistryTokenMismatch,
        Vec::new(),
    );
}

#[test]
fn phase83_revocation_after_pre_authentication_rejects_the_exact_owner() {
    phase83_assert_post_auth_revocation(Phase83Fixture::new(206, 1, true), false);
}

#[test]
fn phase83_revocation_after_stable_read_rejects_the_exact_owner() {
    phase83_assert_post_auth_revocation(Phase83Fixture::new(207, 1, true), true);
}

#[test]
fn phase83_unavailable_stable_read_rejects_without_losing_ownership() {
    let mut fixture = Phase83Fixture::new(208, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let registry_before = fixture.registry_before.clone();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.status(),
        ProjectionCandidateReauthenticationStatus::Rejected
    );
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::FinalRecoveredHomeMismatch
        ))
    );
    phase83_assert_counts(fixture.ledger(), 0, 1, 0, 0);
    assert_eq!(fixture.registry_now(), registry_before);

    assert_ne!(fixture.home.health().state(), HomeHealthState::Healthy);
    phase83_assert_terminal_ledger(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::FinalRecoveredHomeMismatch,
    );
    let terminal = phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::FinalRecoveredHomeMismatch,
    );
    drop(terminal);
    fixture.close();
}

#[test]
fn phase83_failed_stable_read_does_not_mask_post_read_connection_retirement() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(220, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    let pause = fixture
        .ledger()
        .pause_candidate_after_stable_read_for_test(candidate);
    let connection = Arc::clone(&fixture.connection);
    let ledger = fixture.ledger_mut();

    let outcome = std::thread::scope(|scope| {
        let worker = scope.spawn(move || ledger.reauthenticate_candidate(candidate));
        pause.wait_until_paused(Duration::from_secs(5));
        assert!(matches!(
            connection.retire_authority_for_recovery_test().unwrap(),
            crate::cas_projection::connection::ConnectionRetirementOutcome::FailureRetained(_)
        ));
        pause.release();
        worker.join().unwrap().unwrap()
    });

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
    phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionRetired,
    )
    .dispose()
    .unwrap();
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
fn phase83_unexpected_unwind_remains_owned_for_poison_safe_disposition() {
    let mut fixture = Phase83Fixture::new(209, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let registry_before = fixture.registry_before.clone();
    fixture
        .faults
        .panic_next(FaultPoint::BeforeReadConfirmation);

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.status(),
        ProjectionCandidateReauthenticationStatus::Rejected
    );
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::UnexpectedUnwind)
    );
    phase83_assert_counts(fixture.ledger(), 0, 1, 0, 0);
    assert_eq!(fixture.registry_now(), registry_before);

    crate::cas_projection::connection::registry::poison_loaded_registry_for_recovery_drop_test();

    let disposition = fixture.ledger_mut().dispose_candidate(candidate).unwrap();
    assert_eq!(disposition.candidate_id(), candidate);
    crate::cas_projection::connection::registry::clear_loaded_registry_poison_for_test();
    assert!(fixture.registry_now().is_empty());
    phase83_assert_counts(fixture.ledger(), 0, 0, 0, 1);

    fixture.close();
}

#[test]
fn phase83_seal_revocation_demotes_the_exact_accepted_candidate() {
    let mut fixture = Phase83Fixture::new(210, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.status(),
        ProjectionCandidateReauthenticationStatus::Accepted
    );
    let cas_thread_id = fixture.registry_before[0].key().cas_thread_id.clone();
    let closed = fixture
        .connection
        .record_thread_closed(&cas_thread_id)
        .unwrap();
    assert!(closed.registry_authority_revoked());

    let seal_error = fixture
        .take_ledger()
        .seal()
        .unwrap_err()
        .into_retryable()
        .unwrap();
    assert_eq!(
        seal_error.reason(),
        ProjectionCandidateLedgerSealReason::AcceptedCandidateAuthenticationFailed
    );
    assert_eq!(seal_error.candidate_id(), Some(candidate));
    fixture.ledger = Some(seal_error.into_ledger());
    let rejected = fixture.ledger().candidate(candidate).unwrap();
    assert_eq!(
        rejected.status(),
        ProjectionCandidateReauthenticationStatus::Rejected
    );
    assert_eq!(
        rejected.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::SealRegistryTokenMismatch)
    );

    fixture.ledger_mut().dispose_candidate(candidate).unwrap();
    drop(fixture.take_ledger().seal().unwrap());
    fixture.close();
}
