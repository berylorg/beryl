#[test]
fn phase83_service_membership_loss_terminals_the_whole_ledger() {
    let mut fixture = Phase83Fixture::new(215, 2, true);
    let candidate = fixture.candidate_ids()[0];
    fixture
        .ledger()
        .remove_adopted_service_membership_for_test();

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipMismatch
        ))
    );
    phase83_assert_terminal_ledger(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipMismatch,
    );
    let terminal = phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipMismatch,
    );
    drop(terminal);
    fixture.close();
}

#[test]
fn phase83_stable_connection_authentication_unavailable_is_terminal() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(217, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture.connection.poison_forwarding_epoch_barrier_for_test();

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::StableConnectionAuthenticationUnavailable
        ))
    );
    phase83_assert_terminal_ledger(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionAuthenticationUnavailable,
    );
    phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionAuthenticationUnavailable,
    )
    .dispose()
    .unwrap();
    fixture.close();
}

#[test]
fn phase83_service_membership_authentication_unavailable_is_terminal() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(218, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture
        .ledger()
        .poison_adopted_service_membership_for_test();

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipUnavailable
        ))
    );
    phase83_assert_terminal_ledger(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipUnavailable,
    );
    phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::ServiceMembershipUnavailable,
    )
    .dispose()
    .unwrap();
    fixture.close();
}

#[test]
fn phase83_loaded_registry_authentication_unavailable_is_terminal() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(219, 1, true);
    let candidate = fixture.candidate_ids()[0];
    crate::cas_projection::connection::registry::poison_loaded_registry_for_recovery_drop_test();

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::LoadedRegistryAuthenticationUnavailable
        ))
    );
    phase83_assert_terminal_ledger(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::LoadedRegistryAuthenticationUnavailable,
    );
    phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::LoadedRegistryAuthenticationUnavailable,
    )
    .dispose()
    .unwrap();
    crate::cas_projection::connection::registry::clear_loaded_registry_poison_for_test();
    fixture.close();
}
