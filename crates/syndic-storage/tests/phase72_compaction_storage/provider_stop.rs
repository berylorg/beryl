use beryl_home_store::{CommandOutcome, CursorReadLimits};
use beryl_model::CasTurnId;
use syndic_storage::{
    AbandonStopOperation, CompactionOperationId, CompactionOperationState, CompactionProviderEvent,
    CompactionRecoveryCase, CompactionRequestDisposition, DeliveryRecoveryCase, InputGateState,
    SafelyReopenStopOperation, StopAdmissionRead, StopCause, StopCauseSet, StopOperationId,
    StopOperationNonce, StopOperationRecord, StopOperationState, StopOperationTransitionStatus,
    TurnEndStatus, TurnTerminalOutcome,
};
#[cfg(feature = "test-faults")]
use syndic_storage::{
    CompactionOperationRecord, CompactionOperationRevision, StopAbandonmentWitness,
    StopMatchingTerminalWitness, StopSafeReopenWitness,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

use super::compaction_support::{CompactionFixture, point_limit};

#[path = "provider_stop/exact_successors.rs"]
mod exact_successors;

fn admit_provider_stop(
    name: &str,
    seed: u8,
) -> (CompactionFixture, CompactionOperationId, StopOperationId) {
    let fixture = CompactionFixture::new(name, seed);
    let compaction_id = fixture.admit(seed.wrapping_add(20), 10);
    fixture.claim(compaction_id);
    fixture.publish_provider(
        compaction_id,
        CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Active),
        20,
    );
    fixture.publish_provider(
        compaction_id,
        CompactionProviderEvent::TurnStarted(
            CasTurnId::new(format!("provider-stop-{seed}")).unwrap(),
        ),
        21,
    );
    let before = fixture.gate();
    assert_eq!(before.live_next_turn_count(), 0);

    let StopAdmissionRead::Admissible(candidate) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("published provider operation must admit exact stop")
    };
    assert_eq!(
        candidate.target().turn_id(),
        compaction_id.provider_turn_id()
    );
    assert_eq!(candidate.selected_route_option(), None);
    let request = candidate.admission(
        StopOperationNonce::from_bytes([seed.wrapping_add(40); 16]),
        StopCauseSet::from(StopCause::SelectedOperationControl),
    );
    let stop_id = request.operation_id();
    match fixture
        .store
        .execute_current(fixture.storage.current_admit_stop_operation(request))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean provider-stop admission, got {outcome:?}"),
    }
    assert_eq!(fixture.gate().live_next_turn_count(), 0);
    (fixture, compaction_id, stop_id)
}

fn stop(fixture: &CompactionFixture, id: StopOperationId) -> StopOperationRecord {
    fixture
        .storage
        .stop_operation(&fixture.store, id, point_limit())
        .unwrap()
        .unwrap()
}

#[cfg(feature = "test-faults")]
fn replace_stop(
    fixture: &CompactionFixture,
    current: &StopOperationRecord,
    state: StopOperationState,
) {
    let forged = StopOperationRecord::new(
        current.id(),
        current.target().clone(),
        current.admission(),
        current.revision(),
        current.cause_first_revisions(),
        current.dispatch_claim(),
        state,
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::StopOperation(forged)).unwrap();
    crate::support::commit(&fixture.store, fixture.storage.clone(), batch);
}

fn abandon_provider_stop(fixture: &CompactionFixture) {
    let page = fixture
        .storage
        .delivery_recovery_startup_page(
            &fixture.store,
            None,
            CursorReadLimits::new(64, 64 * 1024).unwrap(),
        )
        .unwrap();
    let source = page
        .records()
        .iter()
        .find(|source| source.thread_id() == fixture.thread)
        .unwrap();
    let DeliveryRecoveryCase::Stopping(recovered) = fixture
        .storage
        .classify_delivery_recovery(&fixture.store, source, point_limit())
        .unwrap()
    else {
        panic!("provider stop must classify for startup abandonment")
    };
    let stale = recovered
        .startup_stale_binding(
            "provider compaction process generation was lost",
            recovered.minimum_timestamp(),
        )
        .unwrap();
    let request = AbandonStopOperation::new(
        recovered.operation_id(),
        recovered.target().clone(),
        recovered.current_gate_revision(),
        recovered.stop_revision(),
        recovered.current_state_revision(),
        recovered.startup_abandonment_reason(),
        stale,
    );
    match fixture
        .store
        .execute_current(fixture.storage.current_abandon_stop_operation(request))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean provider-stop abandonment, got {outcome:?}"),
    }
}

#[test]
fn committed_provider_stop_is_immediately_authenticated_without_an_ordinary_route() {
    let (fixture, compaction_id, stop_id) = admit_provider_stop("phase72-provider-stop-live", 181);
    let StopAdmissionRead::Stopping(live) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("provider stop must remain live immediately after admission")
    };
    assert_eq!(live.operation_id(), stop_id);
    assert_eq!(live.state(), StopOperationState::Admitted);
    assert_eq!(live.stopped_route(), None);
    assert_eq!(live.target().turn_id(), compaction_id.provider_turn_id());
    assert_eq!(fixture.gate().live_next_turn_count(), 0);
    assert_eq!(
        fixture.operation(compaction_id).state(),
        &CompactionOperationState::Stopping(stop_id.nonce())
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn provider_stop_accepts_higher_stopping_revision_with_ordered_provider_witness() {
    let (fixture, compaction_id, stop_id) =
        admit_provider_stop("phase72-provider-stop-observed-descendant", 187);
    let before = fixture.operation(compaction_id);
    fixture.publish_provider(
        compaction_id,
        CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Idle),
        30,
    );
    let after = fixture.operation(compaction_id);
    assert_eq!(after.revision(), before.revision().checked_next().unwrap());
    assert_eq!(
        after.state(),
        &CompactionOperationState::Stopping(stop_id.nonce())
    );
    assert_eq!(
        after.provider_frontier(),
        after.status().map(|status| status.sequence())
    );

    let StopAdmissionRead::Stopping(live) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("ordered provider evidence must authenticate the stopping descendant")
    };
    assert_eq!(live.operation_id(), stop_id);
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn provider_stop_reopens_and_classifies_as_nonreplayable_startup_authority() {
    let (fixture, compaction_id, stop_id) =
        admit_provider_stop("phase72-provider-stop-reopen", 182);
    let fixture = fixture.reopen();
    let StopAdmissionRead::Stopping(live) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("reopened provider stop must remain exact live authority")
    };
    assert_eq!(live.operation_id(), stop_id);
    assert_eq!(live.stopped_route(), None);
    assert!(matches!(
        fixture
            .storage
            .compaction_recovery_read(&fixture.store, compaction_id, point_limit())
            .unwrap(),
        Some(CompactionRecoveryCase::Stopping(_))
    ));

    let page = fixture
        .storage
        .delivery_recovery_startup_page(
            &fixture.store,
            None,
            CursorReadLimits::new(64, 64 * 1024).unwrap(),
        )
        .unwrap();
    let source = page
        .records()
        .iter()
        .find(|source| source.thread_id() == fixture.thread)
        .expect("provider stopping gate remains a startup source");
    let DeliveryRecoveryCase::Stopping(recovered) = fixture
        .storage
        .classify_delivery_recovery(&fixture.store, source, point_limit())
        .unwrap()
    else {
        panic!("startup must use the provider-specific stop classifier")
    };
    assert_eq!(recovered.operation_id(), stop_id);
    assert_eq!(recovered.stopped_route(), None);
    assert_eq!(fixture.gate().live_next_turn_count(), 0);
}

#[test]
fn provider_stop_abandonment_retains_exact_stopping_gate_receipt() {
    let (fixture, compaction_id, _) =
        admit_provider_stop("phase72-provider-stop-abandonment-receipt", 184);
    abandon_provider_stop(&fixture);

    let receipt = fixture
        .storage
        .compaction_settlement_receipt(&fixture.store, compaction_id, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        receipt.source_gate().state(),
        InputGateState::Stopping { .. }
    ));
    assert_eq!(receipt.successor_gate().state(), &InputGateState::Idle);
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    let fixture = fixture.reopen();
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert!(matches!(
        fixture
            .storage
            .compaction_recovery_read(&fixture.store, compaction_id, point_limit())
            .unwrap()
            .unwrap(),
        CompactionRecoveryCase::Settled(_)
    ));
}

#[test]
#[cfg(feature = "test-faults")]
fn provider_stop_preserves_deferred_accepted_next_through_abandonment() {
    let (fixture, compaction_id, stop_id) =
        admit_provider_stop("phase72-provider-stop-accepted-next", 193);
    fixture.inject_deferred_accepted_next(253, 30);

    let StopAdmissionRead::Stopping(live) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("path-neutral accepted-next admission must retain provider stop authority")
    };
    assert_eq!(live.operation_id(), stop_id);
    assert_eq!(fixture.gate().live_next_turn_count(), 1);

    abandon_provider_stop(&fixture);
    let receipt = fixture
        .storage
        .compaction_settlement_receipt(&fixture.store, compaction_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(receipt.source_gate().live_next_turn_count(), 1);
    assert_eq!(receipt.successor_gate().live_next_turn_count(), 1);
}

#[test]
#[cfg(feature = "test-faults")]
fn reopened_provider_stop_with_missing_compaction_pair_is_corruption() {
    let (fixture, compaction_id, _) = admit_provider_stop("phase72-provider-stop-corrupt", 183);
    let fixture = fixture.reopen();
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::CompactionOperation(compaction_id))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage.clone(), batch);

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(syndic_storage::SyndicReadError::Invariant(_))
    ));
    let error = fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("compaction operation is missing"),
        "unexpected validation error: {error}"
    );
}

#[test]
#[cfg(feature = "test-faults")]
fn provider_stop_rejects_impossible_stopping_compaction_revision() {
    let (fixture, compaction_id, _) =
        admit_provider_stop("phase72-provider-stop-impossible-revision", 185);
    let operation = fixture.operation(compaction_id);
    let forged_revision = CompactionOperationRevision::new(operation.revision().get() - 1).unwrap();
    let forged = CompactionOperationRecord::new(
        operation.id(),
        operation.home_id(),
        operation.target().clone(),
        forged_revision,
        operation.attempt(),
        operation.dispatch_claim(),
        operation.request(),
        operation.provider_frontier(),
        operation.status(),
        operation.cas_turn().cloned(),
        operation.marker().cloned(),
        operation.terminal(),
        operation.state().clone(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::CompactionOperation(forged))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage.clone(), batch);

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(syndic_storage::SyndicReadError::Invariant(_))
    ));
    let error = fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("live provider stop and compaction authority disagree"),
        "unexpected validation error: {error}"
    );
}

#[test]
#[cfg(feature = "test-faults")]
fn provider_stop_rejects_unwitnessed_higher_stopping_compaction_revision() {
    let (fixture, compaction_id, _) =
        admit_provider_stop("phase72-provider-stop-unwitnessed-revision", 186);
    let operation = fixture.operation(compaction_id);
    let forged = CompactionOperationRecord::new(
        operation.id(),
        operation.home_id(),
        operation.target().clone(),
        operation.revision().checked_next().unwrap(),
        operation.attempt(),
        operation.dispatch_claim(),
        operation.request(),
        operation.provider_frontier(),
        operation.status(),
        operation.cas_turn().cloned(),
        operation.marker().cloned(),
        operation.terminal(),
        operation.state().clone(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::CompactionOperation(forged))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage.clone(), batch);

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(syndic_storage::SyndicReadError::Invariant(_))
    ));
    let error = fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("live provider stop and compaction authority disagree"),
        "unexpected validation error: {error}"
    );
}
