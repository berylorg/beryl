use super::*;

fn safely_reopen(
    fixture: &CompactionFixture,
    stop_id: StopOperationId,
) -> SafelyReopenStopOperation {
    let current = stop(fixture, stop_id);
    let request = SafelyReopenStopOperation::new(
        stop_id,
        current.target().clone(),
        fixture.gate().revision(),
        current.revision(),
    );
    fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_safely_reopen_stop_operation(request.clone()),
        )
        .unwrap();
    request
}

#[test]
fn safe_reopen_authenticates_a_later_provider_observation_descendant() {
    let (fixture, compaction_id, stop_id) =
        admit_provider_stop("phase72-provider-stop-safe-reopen-descendant", 188);
    let request = safely_reopen(&fixture, stop_id);
    let immediate = fixture.operation(compaction_id).revision();
    fixture.publish_provider(
        compaction_id,
        CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Idle),
        31,
    );
    assert!(fixture.operation(compaction_id).revision() > immediate);
    assert_eq!(
        fixture
            .storage
            .safe_stop_reopen_status(&fixture.store, &request, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    fixture.store.validate_registered_domains().unwrap();
    fixture
        .reopen()
        .store
        .validate_registered_domains()
        .unwrap();
}

#[test]
fn matching_terminal_authenticates_a_late_request_descendant() {
    let (fixture, compaction_id, stop_id) =
        admit_provider_stop("phase72-provider-stop-terminal-descendant", 189);
    fixture.publish_provider(
        compaction_id,
        CompactionProviderEvent::Terminal(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        31,
    );
    let immediate = fixture.operation(compaction_id).revision();
    assert!(matches!(
        stop(&fixture, stop_id).state(),
        StopOperationState::MatchingTerminal(_)
    ));
    fixture.publish_request(compaction_id, CompactionRequestDisposition::Accepted);
    assert!(fixture.operation(compaction_id).revision() > immediate);
    fixture.store.validate_registered_domains().unwrap();
    fixture
        .reopen()
        .store
        .validate_registered_domains()
        .unwrap();
}

#[test]
fn coherently_shifted_safe_reopen_compaction_pair_is_corruption() {
    let (fixture, _, stop_id) =
        admit_provider_stop("phase72-provider-stop-safe-reopen-forged", 190);
    let request = safely_reopen(&fixture, stop_id);
    let current = stop(&fixture, stop_id);
    let StopOperationState::SafeReopened(StopSafeReopenWitness::ProviderOperation {
        source,
        successor_gate_revision,
        source_compaction_revision,
        successor_compaction_revision,
    }) = current.state()
    else {
        panic!("provider safe reopen must retain provider witness")
    };
    replace_stop(
        &fixture,
        &current,
        StopOperationState::SafeReopened(StopSafeReopenWitness::provider_operation(
            source,
            successor_gate_revision,
            source_compaction_revision.checked_next().unwrap(),
            successor_compaction_revision.checked_next().unwrap(),
        )),
    );
    assert!(!matches!(
        fixture
            .storage
            .safe_stop_reopen_status(&fixture.store, &request, point_limit()),
        Ok(StopOperationTransitionStatus::Exact)
    ));
    assert!(fixture.store.validate_registered_domains().is_err());
}

#[test]
fn coherently_shifted_matching_terminal_compaction_pair_is_corruption() {
    let (fixture, compaction_id, stop_id) =
        admit_provider_stop("phase72-provider-stop-terminal-forged", 191);
    fixture.publish_provider(
        compaction_id,
        CompactionProviderEvent::Terminal(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        31,
    );
    let current = stop(&fixture, stop_id);
    let StopOperationState::MatchingTerminal(StopMatchingTerminalWitness::ProviderOperation {
        source,
        successor_gate_revision,
        successor_turn_state_revision,
        source_compaction_revision,
        successor_compaction_revision,
    }) = current.state()
    else {
        panic!("provider terminal must retain provider witness")
    };
    replace_stop(
        &fixture,
        &current,
        StopOperationState::MatchingTerminal(StopMatchingTerminalWitness::provider_operation(
            source,
            successor_gate_revision,
            successor_turn_state_revision,
            source_compaction_revision.checked_next().unwrap(),
            successor_compaction_revision.checked_next().unwrap(),
        )),
    );
    assert!(fixture.store.validate_registered_domains().is_err());
}

#[test]
fn coherently_shifted_abandonment_compaction_pair_is_corruption() {
    let (fixture, _, stop_id) =
        admit_provider_stop("phase72-provider-stop-abandonment-forged", 192);
    abandon_provider_stop(&fixture);
    let current = stop(&fixture, stop_id);
    let StopOperationState::Abandoned(StopAbandonmentWitness::ProviderOperation {
        source,
        reason,
        successor_gate_revision,
        retired_binding_revision,
        successor_turn_state_revision,
        source_compaction_revision,
        successor_compaction_revision,
    }) = current.state()
    else {
        panic!("provider abandonment must retain provider witness")
    };
    replace_stop(
        &fixture,
        &current,
        StopOperationState::Abandoned(StopAbandonmentWitness::provider_operation(
            source,
            reason,
            successor_gate_revision,
            retired_binding_revision,
            successor_turn_state_revision,
            source_compaction_revision.checked_next().unwrap(),
            successor_compaction_revision.checked_next().unwrap(),
        )),
    );
    assert!(fixture.store.validate_registered_domains().is_err());
}
