#[test]
fn phase83_candidate_witness_mismatch_remains_owned_and_retryable() {
    let mut fixture = Phase83Fixture::new(221, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture
        .ledger_mut()
        .replace_candidate_witness_owner_for_test(
            candidate,
            SyndicThreadId::from_bytes([0; 16]),
        );

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateWitnessMismatch,
    );
}

#[test]
fn phase83_candidate_connection_mismatch_remains_owned_and_retryable() {
    let mut fixture = Phase83Fixture::new(222, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture
        .ledger_mut()
        .replace_candidate_connection_key_for_test(
            candidate,
            CasThreadId::new("phase-83-mismatched-candidate-key").unwrap(),
        );

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::CandidateConnectionMismatch,
    );
}

#[test]
fn phase83_stable_connection_identity_mismatch_terminals_the_fixed_set() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(230, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture
        .ledger_mut()
        .corrupt_candidate_stable_connection_identity_for_test(candidate);

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::StableConnectionMismatch
        ))
    );
    phase83_assert_terminal_ledger(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionMismatch,
    );
    phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionMismatch,
    )
    .dispose()
    .unwrap();
    assert!(fixture.registry_now().is_empty());
    fixture.close();
}

#[test]
fn phase83_unhealthy_recovered_home_terminals_the_fixed_set_before_read() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(232, 1, true);
    let candidate = fixture.candidate_ids()[0];
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.home.home_revision().is_err());
    assert_ne!(fixture.home.health().state(), HomeHealthState::Healthy);

    let outcome = fixture
        .ledger_mut()
        .reauthenticate_candidate(candidate)
        .unwrap();
    assert_eq!(
        outcome.rejection_reason(),
        Some(ProjectionCandidateReauthenticationReason::ServiceTerminal(
            TerminalAdoptedProjectionConnectionServiceReason::RecoveredHomeNotHealthy
        ))
    );
    phase83_assert_terminal_ledger(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::RecoveredHomeNotHealthy,
    );
    phase83_take_terminal_service(
        &mut fixture,
        TerminalAdoptedProjectionConnectionServiceReason::RecoveredHomeNotHealthy,
    )
    .dispose()
    .unwrap();
    assert!(fixture.registry_now().is_empty());
    fixture.close();
}

#[test]
fn phase83_pending_turn_fact_mismatch_remains_owned_and_retryable() {
    let fixture = Phase83Fixture::new(223, 1, true);
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::InputGate(InputGateRecord::idle(
            fixture.syndic_thread_id,
        )))
        .unwrap();
    fixture.apply_fixture_batch(batch);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::PendingOrdinaryTurnUnavailable,
    );
}

#[test]
fn phase83_missing_sealed_input_remains_owned_and_retryable() {
    let fixture = Phase83Fixture::new(224, 1, true);
    let input = fixture
        .pending_item()
        .presentation_content()
        .expect("the pending user item carries sealed input");
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::ContentManifest(input.id()))
        .unwrap();
    fixture.apply_fixture_batch(batch);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::PendingOrdinaryInputContentUnavailable,
    );
}

#[test]
fn phase83_missing_pending_state_remains_owned_and_retryable() {
    let fixture = Phase83Fixture::new(225, 1, true);
    let turn_id = fixture.pending_item().turn_id();
    let mut batch = FixtureBatch::new();
    batch.delete(FixtureDelete::TurnState(turn_id)).unwrap();
    fixture.apply_fixture_batch(batch);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::PendingOrdinaryInvariant,
    );
}

#[test]
fn phase83_missing_asset_manifest_remains_owned_as_a_retryable_read_failure() {
    let fixture = Phase83Fixture::new(226, 1, true);
    let item = fixture.pending_item();
    let input = item
        .presentation_content()
        .expect("the pending user item carries sealed input");
    let source = input.sealed_marker_summary().unwrap();
    assert_eq!(source.marker_count(), 0);
    let proof = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([226; 16]),
        source,
        0,
        AssetReferenceSetDigest::from_bytes([226; 32]),
    )
    .unwrap();
    let changed = CanonicalItemRecord::local_user_input(
        item.id(),
        item.turn_id(),
        item.ordinal(),
        item.revision(),
        input,
        Some(proof),
    );
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::CanonicalItem(changed)).unwrap();
    fixture.apply_fixture_batch(batch);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::PendingOrdinaryReadUnavailable,
    );
}

#[test]
fn phase83_input_asset_proof_mismatch_remains_owned_and_retryable() {
    let fixture = Phase83Fixture::new(229, 1, true);
    let item = fixture.pending_item();
    let input = item
        .presentation_content()
        .expect("the pending user item carries sealed input");
    let source = input.sealed_marker_summary().unwrap();
    assert_eq!(source.marker_count(), 0);
    let proof = fixture.seal_empty_asset_reference_set(source, 229);
    let changed = CanonicalItemRecord::local_user_input(
        item.id(),
        item.turn_id(),
        item.ordinal(),
        item.revision(),
        input,
        Some(proof),
    );
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::CanonicalItem(changed)).unwrap();
    fixture.apply_fixture_batch(batch);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::PendingOrdinaryInputAssetReferenceSetMismatch,
    );
}

#[test]
fn phase83_candidate_set_authority_gate_poison_retains_the_terminal_whole_attempt() {
    let mut fixture = Phase83Fixture::new(227, 1, true);
    let candidate = fixture.candidate_ids()[0];
    assert_eq!(
        fixture
            .ledger_mut()
            .reauthenticate_candidate(candidate)
            .unwrap()
            .status(),
        ProjectionCandidateReauthenticationStatus::Accepted
    );
    fixture.connection.poison_authority_for_recovery_test();

    let terminal = fixture
        .take_ledger()
        .seal()
        .unwrap_err()
        .into_terminal()
        .unwrap();
    assert_eq!(
        terminal.reason(),
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionAuthenticationUnavailable
    );
    let metadata = terminal.metadata();
    assert_eq!(metadata.connection_owner_count(), 1);
    assert_eq!(metadata.rejected_count(), 1);
    assert_eq!(metadata.accepted_count(), 0);
    drop(terminal);
    assert!(fixture.registry_now().is_empty());
    fixture.close();
}

#[test]
fn phase83_candidate_set_topology_mismatch_retains_the_terminal_whole_attempt() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(231, 1, true);
    let candidate = fixture.candidate_ids()[0];
    assert_eq!(
        fixture
            .ledger_mut()
            .reauthenticate_candidate(candidate)
            .unwrap()
            .status(),
        ProjectionCandidateReauthenticationStatus::Accepted
    );
    fixture
        .ledger_mut()
        .force_candidate_set_topology_mismatch_for_test();

    let terminal = fixture
        .take_ledger()
        .seal()
        .unwrap_err()
        .into_terminal()
        .unwrap();
    assert_eq!(
        terminal.reason(),
        TerminalAdoptedProjectionConnectionServiceReason::StableConnectionMismatch
    );
    let metadata = terminal.metadata();
    assert_eq!(metadata.connection_owner_count(), 1);
    assert_eq!(metadata.rejected_count(), 1);
    terminal.dispose().unwrap();
    assert!(fixture.registry_now().is_empty());
    fixture.close();
}

#[test]
fn phase83_seal_registry_poison_retains_the_terminal_whole_attempt() {
    let mut fixture = Phase83Fixture::new_for_explicit_terminal_disposition(228, 1, true);
    let candidate = fixture.candidate_ids()[0];
    assert_eq!(
        fixture
            .ledger_mut()
            .reauthenticate_candidate(candidate)
            .unwrap()
            .status(),
        ProjectionCandidateReauthenticationStatus::Accepted
    );
    crate::cas_projection::connection::registry::poison_loaded_registry_for_recovery_drop_test();

    let terminal = fixture
        .take_ledger()
        .seal()
        .unwrap_err()
        .into_terminal()
        .unwrap();
    assert_eq!(
        terminal.reason(),
        TerminalAdoptedProjectionConnectionServiceReason::LoadedRegistryAuthenticationUnavailable
    );
    let metadata = terminal.metadata();
    assert_eq!(metadata.connection_owner_count(), 1);
    assert_eq!(metadata.rejected_count(), 1);
    crate::cas_projection::connection::registry::clear_loaded_registry_poison_for_test();
    terminal.dispose().unwrap();
    assert!(fixture.registry_now().is_empty());
    fixture.close();
}
