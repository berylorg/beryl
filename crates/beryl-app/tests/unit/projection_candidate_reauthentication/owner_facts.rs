fn phase83_different_loaded_generation(
    fixture: &Phase83Fixture,
) -> beryl_model::CasLoadedSessionGeneration {
    let current = fixture.registry_before[0].loaded_generation();
    beryl_model::CasLoadedSessionGeneration::new(
        current.process(),
        beryl_model::CasLoadedThreadGeneration::new(current.thread().get() + 1).unwrap(),
    )
}

#[test]
fn phase83_registry_connection_identity_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(233, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture
        .ledger()
        .inject_registry_connection_identity_mismatch_for_test(candidate);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateConnectionMismatch,
    );
}

#[test]
fn phase83_registry_key_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(234, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture.ledger().replace_candidate_registry_key_for_test(
        candidate,
        CasThreadId::new("phase-83-registry-key-mismatch").unwrap(),
    );

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateConnectionMismatch,
    );
}

#[test]
fn phase83_registry_syndic_owner_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(235, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let replacement = SyndicThreadId::from_bytes([0; 16]);
    assert_ne!(replacement, fixture.syndic_thread_id);
    fixture
        .ledger()
        .replace_candidate_registry_owner_for_test(candidate, replacement);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateConnectionMismatch,
    );
}

#[test]
fn phase83_registry_loaded_generation_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(236, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let replacement = phase83_different_loaded_generation(&fixture);
    fixture
        .ledger()
        .replace_candidate_registry_loaded_generation_for_test(candidate, replacement);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateConnectionMismatch,
    );
}

#[test]
fn phase83_witness_home_id_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(237, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let replacement = BerylHomeId::from_bytes([0; 16]);
    assert_ne!(replacement, fixture.home.home_id());
    fixture
        .ledger()
        .replace_candidate_witness_home_id_for_test(candidate, replacement);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateWitnessMismatch,
    );
}

#[test]
fn phase83_witness_home_generation_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(238, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture
        .ledger()
        .replace_candidate_witness_home_generation_for_test(
            candidate,
            fixture.recovered_generation,
        );

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateWitnessMismatch,
    );
}

#[test]
fn phase83_witness_syndic_owner_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(239, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let replacement = SyndicThreadId::from_bytes([0; 16]);
    assert_ne!(replacement, fixture.syndic_thread_id);
    fixture
        .ledger()
        .inject_candidate_witness_owner_mismatch_for_test(candidate, replacement);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateWitnessMismatch,
    );
}

#[test]
fn phase83_witness_loaded_generation_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(240, 1, true);
    let candidate = fixture.candidate_ids()[0];
    let replacement = phase83_different_loaded_generation(&fixture);
    fixture
        .ledger()
        .replace_candidate_witness_loaded_generation_for_test(candidate, replacement);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateWitnessMismatch,
    );
}

#[test]
fn phase83_candidate_group_connection_key_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(241, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture
        .ledger()
        .inject_candidate_group_connection_key_mismatch_for_test(
            candidate,
            CasThreadId::new("phase-83-group-connection-key-mismatch").unwrap(),
        );

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateConnectionMismatch,
    );
}
