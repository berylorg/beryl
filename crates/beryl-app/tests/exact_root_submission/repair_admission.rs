use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{CasThreadId, CasTurnId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CasTurnSource, InputGateRecord, InputGateState, RepairCaptureGap, RepairCaptureGapReason,
    RepairRequestDisposition, RepairRequiredTarget, SourceEventPayload, SourceEventRecord,
    SourceEventSequence, TurnEndStatus, TurnIncompleteReason, TurnTerminalOutcome,
    test_faults::{FixtureBatch, FixtureRecord},
};

fn install_gate(
    store: &beryl_home_store::HomeStore,
    storage: &syndic_storage::SyndicStorage,
    thread: SyndicThreadId,
    repair: bool,
) {
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let state = if repair {
        let source = CasTurnSource::new(
            CasThreadId::new("repair-thread").unwrap(),
            CasTurnId::new("repair-turn").unwrap(),
        );
        let status = TurnEndStatus::new(
            TurnTerminalOutcome::Interrupted,
            Some(TurnIncompleteReason::StreamLost),
        )
        .unwrap();
        let event = SourceEventRecord::new(
            SyndicTurnId::from_bytes([245; 16]),
            SourceEventSequence::FIRST,
            Some(source.clone()),
            SourceEventPayload::TurnEnded(status),
        )
        .unwrap();
        InputGateState::RepairRequired(RepairRequiredTarget::new(
            event.turn_id(),
            source,
            RepairCaptureGap::new(
                event.repair_witness(),
                status,
                RepairCaptureGapReason::TerminalCaptureIncomplete,
                None,
            )
            .unwrap(),
            RepairRequestDisposition::Available,
        ))
    } else {
        InputGateState::Idle
    };
    let updated = InputGateRecord::new(
        thread,
        gate.revision().checked_next().unwrap(),
        state,
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::InputGate(updated)).unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(store).unwrap(), batch))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn request() -> ComposerHostSubmissionRequest {
    ComposerHostSubmissionRequest::new(
        beryl_app::cas_projection::SubmissionExecutionWake::storage_only_for_test(),
        SyndicDraftId::from_bytes([240; 16]),
        SyndicItemId::from_bytes([241; 16]),
        DraftComposerMaterializationOperationIdV1::from_bytes([242; 16]),
        DraftPieceOperationIdV1::from_bytes([243; 16]),
        SyndicTimestamp::from_unix_millis(1_000),
        admission_requirement(),
    )
}

#[test]
fn repair_required_refuses_new_acceptance_and_preserves_the_exact_draft() {
    let (_home, store, state, storage, thread) = fixture_with_state("repair-refusal", 231);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 232, 233);
    let binding = commit_text(&mut host, &store, empty, 1, 0, 0, "keep", 4, 1);
    install_gate(&store, &storage, thread, true);
    let ticket = host.begin_submission(request()).unwrap();
    let outcome = drive_submission(&mut host, &store, assets, &seals, ticket, operation_id(234));
    assert_eq!(
        outcome,
        ComposerHostSubmissionAdvance::SubmissionUnavailable(
            beryl_app::cas_projection::SubmissionUnavailable::RepairRequired
        )
    );
    assert_eq!(
        host.binding().unwrap().candidate().draft_id(),
        binding.candidate().draft_id()
    );
    assert_eq!(
        host.binding()
            .unwrap()
            .logical_extent()
            .logical_utf8_bytes(),
        4
    );
    assert!(!host.submission_diagnostics().pending());
    assert!(
        storage
            .accepted_input(
                &store,
                binding.candidate().draft_id().accepted_input_id(),
                point_limit()
            )
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .canonical_item(&store, SyndicItemId::from_bytes([241; 16]), point_limit())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .current_draft(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .draft()
            .id(),
        binding.candidate().draft_id()
    );
}

#[test]
fn repair_starting_after_capture_cannot_pass_final_typed_gate_admission() {
    let (_home, store, state, storage, thread) = fixture_with_state("repair-race", 231);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 232, 233);
    let binding = commit_text(&mut host, &store, empty, 1, 0, 0, "keep", 4, 1);
    let ticket = host.begin_submission(request()).unwrap();
    advance_to_accepting(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        operation_id(234),
        SyndicTimestamp::from_unix_millis(999),
    );
    install_gate(&store, &storage, thread, true);
    let result = host.advance_submission(
        &store,
        ticket,
        assets,
        &seals,
        operation_id(234),
        None,
        SyndicTimestamp::from_unix_millis(999),
        &CommandCancellation::new(),
    );
    assert_eq!(
        result.unwrap(),
        ComposerHostSubmissionAdvance::SubmissionUnavailable(
            beryl_app::cas_projection::SubmissionUnavailable::RepairRequired
        )
    );
    assert!(!host.submission_diagnostics().command_attempted());
    assert_eq!(
        host.binding().unwrap().candidate().draft_id(),
        binding.candidate().draft_id()
    );
    assert!(
        storage
            .canonical_item(&store, SyndicItemId::from_bytes([241; 16]), point_limit())
            .unwrap()
            .is_none()
    );
}

#[test]
fn repair_after_command_build_is_refused_by_the_final_home_writer() {
    let (_home, store, state, storage, thread) = fixture_with_state("repair-writer", 231);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 232, 233);
    let binding = commit_text(&mut host, &store, empty, 1, 0, 0, "keep", 4, 1);
    let ticket = host.begin_submission(request()).unwrap();
    advance_to_accepting(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        operation_id(234),
        SyndicTimestamp::from_unix_millis(999),
    );
    host.test_arm_submission_before_execute_fault(move |store, storage| {
        install_gate(store, &storage, thread, true);
    });
    assert_eq!(
        drive_submission(&mut host, &store, assets, &seals, ticket, operation_id(234)),
        ComposerHostSubmissionAdvance::SubmissionUnavailable(
            beryl_app::cas_projection::SubmissionUnavailable::RepairRequired
        )
    );
    assert_eq!(
        host.binding().unwrap().candidate().draft_id(),
        binding.candidate().draft_id()
    );
    assert!(
        storage
            .accepted_input(
                &store,
                binding.candidate().draft_id().accepted_input_id(),
                point_limit()
            )
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .canonical_item(&store, SyndicItemId::from_bytes([241; 16]), point_limit())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .current_draft(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .draft()
            .id(),
        binding.candidate().draft_id()
    );
}

#[test]
fn already_accepted_work_settles_even_when_repair_begins_after_its_commit() {
    let (_home, store, state, storage, thread) = fixture_with_state("accepted-repair", 231);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 232, 233);
    commit_text(&mut host, &store, empty, 1, 0, 0, "once", 4, 1);
    let ticket = host.begin_submission(request()).unwrap();
    advance_to_accepting(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        operation_id(234),
        SyndicTimestamp::from_unix_millis(999),
    );
    let acceptance = host.test_submission_acceptance().unwrap();
    let beryl_app::input_admission::FirstAcceptanceCommand::Execute(command) =
        beryl_app::input_admission::first_acceptance_command(&store, &storage, &assets, acceptance)
            .unwrap()
    else {
        panic!("new acceptance");
    };
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    install_gate(&store, &storage, thread, true);
    let revision = store.home_revision().unwrap();
    assert!(matches!(
        drive_submission(&mut host, &store, assets, &seals, ticket, operation_id(234)),
        ComposerHostSubmissionAdvance::ExactSuccess(_)
    ));
    assert_eq!(store.home_revision().unwrap(), revision);
}

#[test]
fn uncertain_acceptance_retains_settlement_custody_after_execution_retirement_and_cancellation() {
    use beryl_home_store::test_faults::FaultPoint;

    let (_home, store, state, storage, thread, faults) =
        base::fault_fixture_with_state("uncertain-retirement", 231);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 232, 233);
    commit_text(&mut host, &store, empty, 1, 0, 0, "once", 4, 1);
    let (execution, wakes) = beryl_app::cas_projection::SubmissionExecutionWake::test_for_home(
        &store,
        Default::default(),
    );
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            execution,
            SyndicDraftId::from_bytes([240; 16]),
            SyndicItemId::from_bytes([241; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([242; 16]),
            DraftPieceOperationIdV1::from_bytes([243; 16]),
            SyndicTimestamp::from_unix_millis(1_000),
            admission_requirement(),
        ))
        .unwrap();
    advance_to_accepting(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        operation_id(234),
        SyndicTimestamp::from_unix_millis(999),
    );
    host.test_arm_submission_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    let cancellation = CommandCancellation::new();
    assert_eq!(
        host.advance_submission(
            &store,
            ticket,
            assets.clone(),
            &seals,
            operation_id(234),
            None,
            SyndicTimestamp::from_unix_millis(999),
            &cancellation,
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::ReconciliationPending
    );
    assert_eq!(store.pending_reconciliations().len(), 1);
    assert!(host.submission_diagnostics().command_attempted());
    wakes.retire();
    cancellation.cancel();
    let revision = store.home_revision().unwrap();
    assert!(matches!(
        host.advance_submission(
            &store,
            ticket,
            assets,
            &seals,
            operation_id(234),
            None,
            SyndicTimestamp::from_unix_millis(999),
            &cancellation,
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::ExactSuccess(_)
    ));
    assert_eq!(store.home_revision().unwrap(), revision);
    assert!(store.pending_reconciliations().is_empty());
    assert!(!host.submission_diagnostics().pending());
    assert_eq!(wakes.wake_count(), 0);
}
