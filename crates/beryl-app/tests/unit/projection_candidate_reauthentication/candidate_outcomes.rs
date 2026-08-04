#[test]
fn phase83_zero_candidate_connection_owner_seals() {
    let mut fixture = Phase83Fixture::new(201, 0, true);
    let metadata = fixture.ledger().metadata();
    assert_eq!(metadata.connection_owner_count(), 1);
    assert!(metadata.is_ready_to_seal());

    let converged = fixture.take_ledger().seal().unwrap();
    assert_eq!(converged.accepted_candidate_count(), 0);
    assert_eq!(converged.metadata().connection_count(), 1);
    drop(converged);
    fixture.close();
}

#[test]
fn phase83_exact_success_preserves_connection_registry_and_durable_revision() {
    let mut fixture = Phase83Fixture::new(202, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let stable_identity = fixture.stable_identity;
    let registry_before = fixture.registry_before.clone();
    let revision_before = fixture.home_revision;
    let recovered_generation = fixture.recovered_generation;

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.status(),
        ProjectionCandidateReauthenticationStatus::Accepted
    );
    let recovered = outcome.candidate().recovered().unwrap();
    assert_eq!(recovered.candidate_id(), candidate);
    assert_eq!(recovered.home_generation(), recovered_generation);
    phase83_assert_counts(fixture.ledger(), 0, 0, 1, 0);
    assert_eq!(fixture.connection.identity_observation(), stable_identity);
    assert_eq!(fixture.registry_now(), registry_before);
    assert_eq!(fixture.home.home_revision().unwrap().get(), revision_before);

    let converged = fixture.take_ledger().seal().unwrap();
    assert_eq!(converged.accepted_candidate_count(), 1);
    assert_eq!(fixture.connection.identity_observation(), stable_identity);
    assert_eq!(fixture.registry_now(), registry_before);
    assert_eq!(fixture.home.home_revision().unwrap().get(), revision_before);
    drop(converged);
    fixture.close();
}

#[test]
fn phase83_mismatched_pending_facts_remain_owning_retryable_and_block_seal() {
    let mut fixture = Phase83Fixture::new(203, 1, false);
    let candidate = fixture.candidate_ids()[0];
    let first = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        first.status(),
        ProjectionCandidateReauthenticationStatus::Rejected
    );
    assert_eq!(
        first.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::PendingOrdinaryProjectionMismatch)
    );
    phase83_assert_counts(fixture.ledger(), 0, 1, 0, 0);

    let seal_error = fixture
        .take_ledger()
        .seal()
        .unwrap_err()
        .into_retryable()
        .unwrap();
    assert_eq!(
        seal_error.reason(),
        ProjectionCandidateLedgerSealReason::OutstandingCandidate
    );
    assert_eq!(seal_error.candidate_id(), Some(candidate));
    fixture.ledger = Some(seal_error.into_ledger());
    let retry = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        retry.status(),
        ProjectionCandidateReauthenticationStatus::Rejected
    );
    assert_eq!(retry.rejection_reason(), first.rejection_reason());
    assert_eq!(fixture.registry_now(), fixture.registry_before);

    fixture.ledger_mut().dispose_candidate(candidate).unwrap();
    drop(fixture.take_ledger().seal().unwrap());
    fixture.close();
}

#[test]
fn phase83_explicit_disposition_revokes_exact_token_then_seals() {
    let mut fixture = Phase83Fixture::new(204, 1, true);
    let candidate = fixture.candidate_ids()[0];
    assert_eq!(fixture.registry_before.len(), 1);
    assert_eq!(fixture.replacement_workers, 3);

    let disposition = fixture.ledger_mut().dispose_candidate(candidate).unwrap();
    assert_eq!(disposition.candidate_id(), candidate);
    phase83_assert_counts(fixture.ledger(), 0, 0, 0, 1);
    assert!(fixture.registry_now().is_empty());
    assert_eq!(
        fixture
            .ledger()
            .replacement_worker_diagnostics_for_test()
            .active(),
        fixture.replacement_workers - 1
    );
    assert_eq!(
        fixture.ledger_mut().dispose_candidate(candidate),
        Err(ProjectionCandidateLedgerAccessError::CandidateDisposed)
    );

    let converged = fixture.take_ledger().seal().unwrap();
    assert_eq!(converged.accepted_candidate_count(), 0);
    drop(converged);
    fixture.close();
}

#[test]
fn phase83_multi_candidate_accepted_and_disposed_counts_are_exact() {
    let mut fixture = Phase83Fixture::new(205, 2, true);
    let candidates = fixture.candidate_ids();
    assert_eq!(fixture.registry_before.len(), 2);

    let accepted = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidates[0])
        .unwrap();
    assert_eq!(
        accepted.status(),
        ProjectionCandidateReauthenticationStatus::Accepted
    );
    assert_eq!(
        fixture
            .ledger()
            .replacement_worker_diagnostics_for_test()
            .active(),
        fixture.replacement_workers
    );
    fixture
        .ledger_mut()
        .dispose_candidate(candidates[1])
        .unwrap();
    phase83_assert_counts(fixture.ledger(), 0, 0, 1, 1);
    assert_eq!(fixture.registry_now().len(), 1);
    assert_eq!(
        fixture
            .ledger()
            .replacement_worker_diagnostics_for_test()
            .active(),
        fixture.replacement_workers - 1
    );

    let converged = fixture.take_ledger().seal().unwrap();
    assert_eq!(converged.accepted_candidate_count(), 1);
    drop(converged);
    fixture.close();
}
