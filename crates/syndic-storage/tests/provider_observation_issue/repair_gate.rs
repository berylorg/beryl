use super::*;
use beryl_home_store::{CursorReadLimits, RecordVersion, WholeHomeScrubTrigger};
use beryl_model::InputGateRevision;
use syndic_storage::test_faults::{
    RepairTargetReplacementForTest, decode_input_gate_for_test,
    inject_repair_target_replacement_for_test, input_gate_codec_bytes,
};

fn gate(fixture: &Fixture) -> InputGateRecord {
    fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap()
}

#[test]
fn repair_blocks_binding_publication_but_unrelated_thread_remains_mutable() {
    let fixture = setup("repair-gate-exclusion");
    let target = super::repair_retained::terminal_target(&fixture, false);
    committed_command(enter(&fixture, target.clone(), gate(&fixture).revision()));
    let current = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("terminal fixture must retain valid binding");
    };
    let request = PublishValidBinding::new(
        fixture.thread,
        current.binding().revision(),
        current.binding().selected_path(),
        usable.execution().clone(),
        usable.cas_thread_id().clone(),
        usable.represented_prefix(),
        usable.native_turn_count(),
        usable.tool_profile(),
        usable.lineage(),
    );
    let before = fixture.storage.revision(&fixture.store).unwrap();
    let rejected = not_committed_command(execute(
        &fixture.store,
        fixture.storage.publish_valid_binding(before, request),
    ));
    assert!(matches!(
        typed_error(&rejected),
        SyndicMutationError::RepairTargetConflict
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
    committed_command(fixture.store.execute_current(
        fixture.storage.current_remove_repair_fact_for_test(
            fixture.thread,
            target,
            syndic_storage::test_faults::RepairTargetFactForTest::Terminal,
        ),
    ));
    committed_command(execute(
        &fixture.store,
        fixture.storage.create_thread(
            fixture.storage.revision(&fixture.store).unwrap(),
            CreateThread::ordinary(
                SyndicThreadId::from_bytes([70; 16]),
                SyndicDraftId::from_bytes([71; 16]),
                exact_cas::execution_binding(),
                timestamp(12),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
}

#[test]
fn missing_retained_evidence_rejects_gate_admission_atomically() {
    use syndic_storage::test_faults::RepairTargetFactForTest as Fact;
    for fact in [
        Fact::Thread,
        Fact::Turn,
        Fact::State,
        Fact::CasThread,
        Fact::CasTurn,
        Fact::Terminal,
        Fact::Issue,
        Fact::Observation,
    ] {
        let fixture = setup("repair-gate-missing");
        let target = super::repair_retained::terminal_target(&fixture, true);
        let gate_revision = gate(&fixture).revision();
        committed_command(fixture.store.execute_current(
            fixture.storage.current_remove_repair_fact_for_test(
                fixture.thread,
                target.clone(),
                fact,
            ),
        ));
        let before = fixture.storage.revision(&fixture.store).unwrap();
        let rejected = not_committed_command(enter(&fixture, target, gate_revision));
        assert!(matches!(
            typed_error(&rejected),
            SyndicMutationError::RepairTargetConflict
        ));
        assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
    }
}

fn enter(
    fixture: &Fixture,
    target: RepairRequiredTarget,
    revision: InputGateRevision,
) -> CommandOutcome {
    fixture
        .store
        .execute_current(fixture.storage.current_require_terminal_repair(
            RequireTerminalRepair::new(fixture.thread, revision, target),
        ))
}

#[test]
fn repair_gate_authenticates_persists_and_defers_recovery_without_dispatch() {
    for issue in [false, true] {
        let mut fixture = setup("repair-gate-reopen");
        let target = super::repair_retained::terminal_target(&fixture, issue);
        let before = gate(&fixture);
        committed_command(enter(&fixture, target.clone(), before.revision()));
        let admitted = gate(&fixture);
        assert_eq!(
            admitted.revision(),
            before.revision().checked_next().unwrap()
        );
        assert_eq!(
            admitted.state(),
            &InputGateState::RepairRequired(target.clone())
        );
        assert_eq!(admitted.accepted_high_water(), before.accepted_high_water());
        assert_eq!(admitted.selected_route(), before.selected_route());
        assert_eq!(
            fixture
                .storage
                .non_idle_gate_source(&fixture.store, fixture.thread, limit())
                .unwrap(),
            Some(NonIdleGateSourceRecord::new(
                fixture.thread,
                admitted.revision()
            ))
        );
        fixture
            .store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
        fixture.store.close().unwrap();
        let mut reopened = open(fixture.home.path());
        fixture.storage = SyndicStorage::register_with_schema_validation(&mut reopened).unwrap();
        fixture.store = reopened
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(gate(&fixture), admitted);
        let page = fixture
            .storage
            .delivery_recovery_startup_page(
                &fixture.store,
                None,
                CursorReadLimits::new(64, 2_000_000).unwrap(),
            )
            .unwrap();
        let source = page
            .records()
            .iter()
            .find(|row| row.thread_id() == fixture.thread)
            .unwrap();
        assert_eq!(
            fixture
                .storage
                .classify_delivery_recovery(&fixture.store, source, limit())
                .unwrap(),
            DeliveryRecoveryCase::DeferredRepair {
                thread_id: fixture.thread,
                turn_id: fixture.turn
            }
        );
        assert!(matches!(
            fixture
                .storage
                .stop_admission_read(&fixture.store, fixture.thread, limit())
                .unwrap(),
            StopAdmissionRead::Ineligible(StopAdmissionIneligibility::RepairRequired { .. })
        ));
        let before_replay = fixture.storage.revision(&fixture.store).unwrap();
        let rejected = not_committed_command(enter(&fixture, target.clone(), before.revision()));
        assert!(matches!(
            typed_error(&rejected),
            SyndicMutationError::InputGateRevisionConflict { .. }
        ));
        let rejected = not_committed_command(enter(&fixture, target, admitted.revision()));
        assert!(matches!(
            typed_error(&rejected),
            SyndicMutationError::RepairTargetConflict
        ));
        assert_eq!(
            fixture.storage.revision(&fixture.store).unwrap(),
            before_replay
        );
    }
}

#[test]
fn repaired_target_cannot_be_rearmed_even_when_request_was_available() {
    let fixture = setup("repair-gate-resolved");
    let target = super::repair_retained::terminal_target(&fixture, true);
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap()
        .with_resolved_repair(ResolvedRepair::new(
            target.clone(),
            RepairResolution::Incomplete(TurnIncompleteReason::CompletionMismatch),
        ))
        .unwrap();
    inject_repair_target_replacement_for_test(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        &target,
        RepairTargetReplacementForTest::State(state),
    )
    .unwrap();
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    let error = not_committed_command(enter(&fixture, target, gate(&fixture).revision()));
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::RepairTargetConflict
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), revision);
}

#[test]
fn forged_witness_and_preconsumed_request_cannot_enter_repair() {
    let fixture = setup("repair-gate-forgery");
    let original = super::repair_retained::terminal_target(&fixture, false);
    let revision = gate(&fixture).revision();
    let forged_gap = RepairCaptureGap::new(
        RepairSourceEventWitness::new(
            original.gap().terminal().sequence(),
            RepairSourceEventDigest::from_bytes([99; 32]),
        ),
        original.gap().status(),
        original.gap().reason(),
        original.gap().issue(),
    )
    .unwrap();
    for target in [
        RepairRequiredTarget::new(
            original.turn_id(),
            original.source().clone(),
            forged_gap,
            RepairRequestDisposition::Available,
        ),
        RepairRequiredTarget::new(
            original.turn_id(),
            original.source().clone(),
            original.gap(),
            RepairRequestDisposition::Consumed(
                ConsumedRepairRequest::new(
                    RepairRequestAttemptNonce::from_bytes([7; 16]),
                    revision,
                    revision.checked_next().unwrap(),
                )
                .unwrap(),
            ),
        ),
    ] {
        let before = fixture.storage.revision(&fixture.store).unwrap();
        let error = not_committed_command(enter(&fixture, target, revision));
        assert!(matches!(
            typed_error(&error),
            SyndicMutationError::RepairTargetConflict
        ));
        assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
    }
}

#[test]
fn repair_gate_codec_rejects_truncation_trailing_tags_and_future_claims() {
    let fixture = setup("repair-gate-codec");
    let target = super::repair_retained::terminal_target(&fixture, false);
    let revision = gate(&fixture).revision();
    let consumed = ConsumedRepairRequest::new(
        RepairRequestAttemptNonce::from_bytes([8; 16]),
        revision,
        revision.checked_next().unwrap(),
    )
    .unwrap();
    for request in [
        RepairRequestDisposition::Available,
        RepairRequestDisposition::Consumed(consumed),
    ] {
        let target = RepairRequiredTarget::new(
            target.turn_id(),
            target.source().clone(),
            target.gap(),
            request,
        );
        let make = |rev, steering| {
            InputGateRecord::new(
                fixture.thread,
                rev,
                InputGateState::RepairRequired(target.clone()),
                0,
                None,
                None,
                steering,
                0,
                0,
            )
        };
        let gate = make(revision.checked_next().unwrap(), 0).unwrap();
        assert!(make(revision.checked_next().unwrap(), 1).is_err());
        if matches!(request, RepairRequestDisposition::Consumed(_)) {
            assert!(make(revision, 0).is_err());
        }
        let (version, bytes) = input_gate_codec_bytes(&gate);
        assert_eq!(version, RecordVersion::new(5));
        assert_eq!(bytes[24], 8);
        assert_eq!(decode_input_gate_for_test(&bytes), Some(gate));
        for end in 0..bytes.len() {
            assert!(decode_input_gate_for_test(&bytes[..end]).is_none());
        }
        let mut invalid = bytes.clone();
        invalid.push(0);
        assert!(decode_input_gate_for_test(&invalid).is_none());
        let mut invalid = bytes;
        invalid[24] = 255;
        assert!(decode_input_gate_for_test(&invalid).is_none());
    }
}
