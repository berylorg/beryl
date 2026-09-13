use super::*;
use beryl_app::{cas_projection::SubmissionExecutionWake, composer_host::ComposerHostError};

fn request(execution: SubmissionExecutionWake) -> ComposerHostSubmissionRequest {
    ComposerHostSubmissionRequest::new(
        execution,
        SyndicDraftId::from_bytes([240; 16]),
        SyndicItemId::from_bytes([241; 16]),
        DraftComposerMaterializationOperationIdV1::from_bytes([242; 16]),
        DraftPieceOperationIdV1::from_bytes([243; 16]),
        SyndicTimestamp::from_unix_millis(1_000),
        admission_requirement(),
    )
}

#[test]
fn already_committed_acceptance_notifies_once_without_another_durable_write() {
    let (_directory, mut store, storage, thread) = fixture("already-accepted-wake", 231);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, binding) = activated(storage.clone(), &store, thread, 232, 233);
    commit_text(&mut host, &store, binding, 1, 0, 0, "once", 4, 1);
    let (execution, wakes) = SubmissionExecutionWake::test_for_home(&store);
    let ticket = host.begin_submission(request(execution)).unwrap();
    advance_to_accepting(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        operation_id(234),
        SyndicTimestamp::from_unix_millis(999),
    );
    assert_eq!(wakes.wake_count(), 0);
    let acceptance = host.test_submission_acceptance().unwrap();
    let beryl_app::input_admission::FirstAcceptanceCommand::Execute(command) =
        beryl_app::input_admission::first_acceptance_command(&store, &storage, &assets, acceptance)
            .unwrap()
    else {
        panic!("first acceptance already existed")
    };
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    let revision = store.home_revision().unwrap();
    assert!(matches!(
        drive_submission(&mut host, &store, assets, &seals, ticket, operation_id(234)),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. })
    ));
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(wakes.wake_count(), 1);
}

#[test]
fn cancellation_before_acceptance_does_not_notify_execution() {
    let (_directory, mut store, storage, thread) = fixture("cancel-wake", 231);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, binding) = activated(storage.clone(), &store, thread, 232, 233);
    commit_text(&mut host, &store, binding, 1, 0, 0, "keep", 4, 1);
    let (execution, wakes) = SubmissionExecutionWake::test_for_home(&store);
    let ticket = host.begin_submission(request(execution)).unwrap();
    let cancellation = CommandCancellation::new();
    for _ in 0..128 {
        if host.submission_diagnostics().stage() == Some(ComposerHostSubmissionStage::Accepting) {
            cancellation.cancel();
        }
        let outcome = host
            .advance_submission(
                &store,
                ticket,
                assets.clone(),
                &seals,
                operation_id(234),
                None,
                SyndicTimestamp::from_unix_millis(999),
                &cancellation,
            )
            .unwrap();
        assert_eq!(wakes.wake_count(), 0);
        if outcome == ComposerHostSubmissionAdvance::Cancelled {
            return;
        }
    }
    panic!("cancelled submission did not settle");
}

#[test]
fn foreign_home_capability_is_rejected_before_flush_and_foreign_advance_before_work() {
    let (_directory, mut store, storage, thread) = fixture("bound-wake", 231);
    let (_foreign_directory, foreign, _, _) = fixture("foreign-wake", 235);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, binding) = activated(storage.clone(), &store, thread, 232, 233);
    commit_text(&mut host, &store, binding, 1, 0, 0, "keep", 4, 1);
    let (foreign_execution, foreign_wakes) = SubmissionExecutionWake::test_for_home(&foreign);
    let revision = store.home_revision().unwrap();
    assert!(matches!(
        host.begin_submission(request(foreign_execution)),
        Err(beryl_app::composer_host::ComposerHostSubmissionError::Host(
            ComposerHostError::OldBinding
        ))
    ));
    assert!(!host.submission_diagnostics().pending());
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(foreign_wakes.wake_count(), 0);
    let (execution, wakes) = SubmissionExecutionWake::test_for_home(&store);
    let ticket = host.begin_submission(request(execution)).unwrap();
    assert!(matches!(
        host.advance_submission(
            &foreign,
            ticket,
            assets,
            &seals,
            operation_id(234),
            None,
            SyndicTimestamp::from_unix_millis(999),
            &CommandCancellation::new()
        ),
        Err(beryl_app::composer_host::ComposerHostSubmissionError::Host(
            ComposerHostError::OldBinding
        ))
    ));
    assert_eq!(wakes.wake_count(), 0);
    assert_eq!(store.home_revision().unwrap(), revision);
}

#[test]
fn previous_home_generation_capability_cannot_begin_on_recovered_home() {
    let (_directory, store, _storage, thread, faults) = base::fault_fixture("generation-wake", 231);
    let (execution, wakes) = SubmissionExecutionWake::test_for_home(&store);
    let old_generation = store.health().generation().unwrap();
    faults.fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let candidate = store.recover_same_home().unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let store = candidate.publish();
    assert_ne!(store.health().generation().unwrap(), old_generation);
    let (mut host, _) = activated(storage, &store, thread, 232, 233);
    let revision = store.home_revision().unwrap();
    assert!(matches!(
        host.begin_submission(request(execution)),
        Err(beryl_app::composer_host::ComposerHostSubmissionError::Host(
            ComposerHostError::OldBinding
        ))
    ));
    assert!(!host.submission_diagnostics().pending());
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(wakes.wake_count(), 0);
}
